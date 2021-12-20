///! Provides traits for parsing pattern like nodes
use crate::parser::ParserProgress;
use crate::syntax::expr::{expr_or_assignment, EXPR_RECOVERY_SET};
use crate::ParsedSyntax::{Absent, Present};
use crate::{CompletedMarker, Invalid, ParseRecovery, ParsedSyntax, Parser, ParserState, Valid};
use crate::{ConditionalSyntax, TokenSet};
use rslint_errors::Diagnostic;
use rslint_syntax::SyntaxKind::{EOF, JS_ARRAY_HOLE};
use rslint_syntax::{SyntaxKind, T};
use std::ops::Range;

/// Trait for parsing a pattern with an optional default of the form `pattern = default`
pub(crate) trait ParseWithDefaultPattern {
	/// The syntax kind of the node for a pattern with a default value
	fn pattern_with_default_kind() -> SyntaxKind;

	/// Creates a diagnostic for the case where the pattern is missing. For example, if the
	/// code only contains ` = default`
	fn expected_pattern_error(p: &Parser, range: Range<usize>) -> Diagnostic;

	/// Parses a pattern (without its default value)
	fn parse_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker>;

	/// Parses a pattern and wraps it in a pattern with default if a `=` token follows the pattern
	fn parse_pattern_with_optional_default(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker> {
		let pattern = self.parse_pattern(p);

		if p.at(T![=]) {
			let with_default =
				pattern.precede_or_missing_with_error(p, Self::expected_pattern_error);
			p.bump_any(); // eat the = token

			if expr_or_assignment(p).is_none() {
				p.missing();
			}

			Present(with_default.complete(p, Self::pattern_with_default_kind()))
		} else {
			pattern
		}
	}
}

/// Trait for parsing an array like pattern of the form `[a, b = "c", { }]`
pub(crate) trait ParseArrayPattern<P: ParseWithDefaultPattern> {
	/// The kind of an unknown pattern. Used in case the pattern contains elements that aren't valid patterns
	fn unknown_pattern_kind() -> SyntaxKind;
	/// The kind of the array like pattern (array assignment or array binding)
	fn array_pattern_kind() -> SyntaxKind;
	/// The kind of the rest pattern
	fn rest_pattern_kind() -> SyntaxKind;
	/// The kind of the list
	fn list_kind() -> SyntaxKind;
	///  Creates a diagnostic saying that the parser expected an element at the position passed as an argument.
	fn expected_element_error(p: &Parser, range: Range<usize>) -> Diagnostic;
	/// Creates a pattern with default instance. Used to parse the array elements.
	fn pattern_with_default(&self) -> P;

	/// Tries to parse an array like pattern
	fn parse_array_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker> {
		if !p.at(T!['[']) {
			return Absent;
		}

		let m = p.start();

		p.bump(T!['[']);
		let elements = p.start();
		let mut progress = ParserProgress::default();

		{
			let guard = &mut *p.with_state(ParserState {
				expr_recovery_set: EXPR_RECOVERY_SET.union(token_set![T![,], T![...], T![=]]),
				..p.state.clone()
			});

			while !guard.at(EOF) && !guard.at(T![']']) {
				progress.assert_progressing(guard);

				let recovery = ParseRecovery::new(
					Self::unknown_pattern_kind(),
					token_set!(EOF, T![,], T![']'], T![=], T![;], T![...], T![')']),
				)
				.enable_recovery_on_line_break();

				let element = self
					.parse_any_array_element(guard, &recovery)
					.or_invalid_to_unknown(guard, Self::unknown_pattern_kind())
					.or_recover(guard, &recovery, Self::expected_element_error);

				if element.is_err() {
					// Failed to recover
					break;
				}

				if !guard.at(T![']']) {
					guard.expect_required(T![,]);
				}
			}
		}

		elements.complete(p, Self::list_kind());
		p.expect_required(T![']']);

		Present(m.complete(p, Self::array_pattern_kind()))
	}

	/// Parses a single array element
	fn parse_any_array_element(
		&self,
		p: &mut Parser,
		recovery: &ParseRecovery,
	) -> ParsedSyntax<ConditionalSyntax> {
		match p.cur() {
			T![,] => Present(Valid(p.start().complete(p, JS_ARRAY_HOLE))),
			T![...] => self
				.parse_rest_pattern(p)
				.map(|rest_pattern| validate_rest_pattern(p, rest_pattern, T![']'], recovery)),
			_ => self
				.pattern_with_default()
				.parse_pattern_with_optional_default(p)
				.into_valid(),
		}
	}

	/// Parses a rest element
	fn parse_rest_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker> {
		if !p.at(T![...]) {
			return Absent;
		}

		let m = p.start();
		let rest_end = p.cur_tok().range.end;
		p.bump(T![...]);

		let with_default = self.pattern_with_default();

		with_default
			.parse_pattern(p)
			.or_missing_with_error(p, |p, _| P::expected_pattern_error(p, rest_end..rest_end));

		Present(m.complete(p, Self::rest_pattern_kind()))
	}
}

/// Trait for parsing an object pattern like node of the form `{ a, b: c}`
pub(crate) trait ParseObjectPattern {
	/// Kind used when recovering from invalid properties.
	fn unknown_pattern_kind() -> SyntaxKind;
	/// The kind of the pattern like node this trait parses
	fn object_pattern_kind() -> SyntaxKind;
	/// The kind of the property list
	fn list_kind() -> SyntaxKind;
	/// Creates a diagnostic saying that a property is expected at the passed in range that isn't present.
	fn expected_property_pattern_error(p: &Parser, range: Range<usize>) -> Diagnostic;

	/// Parses the object pattern like node
	fn parse_object_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker> {
		if !p.at(T!['{']) {
			return Absent;
		}

		let m = p.start();

		p.bump(T!['{']);
		let elements = p.start();
		let mut progress = ParserProgress::default();

		{
			// TODO remove after migrating expression to `ParsedSyntax`
			let guard = &mut *p.with_state(ParserState {
				expr_recovery_set: EXPR_RECOVERY_SET.union(token_set![T![,], T![...]]),
				..p.state.clone()
			});

			while !guard.at(T!['}']) {
				progress.assert_progressing(guard);

				if guard.at(T![,]) {
					// missing element
					guard.missing();
					guard.error(Self::expected_property_pattern_error(
						guard,
						guard.cur_tok().range,
					));
					guard.bump_any(); // bump ,
					continue;
				}
				let recovery_set = ParseRecovery::new(
					Self::unknown_pattern_kind(),
					token_set!(EOF, T![,], T!['}'], T![...], T![;], T![')']),
				)
				.enable_recovery_on_line_break();

				let recover_result = self
					.parse_any_property_pattern(guard, &recovery_set)
					.or_invalid_to_unknown(guard, Self::unknown_pattern_kind())
					.or_recover(guard, &recovery_set, Self::expected_property_pattern_error);

				if recover_result.is_err() {
					break;
				}

				if !guard.at(T!['}']) {
					guard.expect_required(T![,]);
				}
			}
		}

		elements.complete(p, Self::list_kind());
		p.expect(T!['}']);

		Present(m.complete(p, Self::object_pattern_kind()))
	}

	/// Parses a single property
	fn parse_any_property_pattern(
		&self,
		p: &mut Parser,
		recovery: &ParseRecovery,
	) -> ParsedSyntax<ConditionalSyntax> {
		if p.at(T![...]) {
			self.parse_rest_property_pattern(p)
				.map(|rest_pattern| validate_rest_pattern(p, rest_pattern, T!['}'], recovery))
		} else {
			self.parse_property_pattern(p).into_valid()
		}
	}

	/// Parses a shorthand `{ a }` or a "named" `{ a: b }` property
	fn parse_property_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker>;

	/// Parses a rest property `{ ...a }`
	fn parse_rest_property_pattern(&self, p: &mut Parser) -> ParsedSyntax<CompletedMarker>;
}

/// Validates if the parsed completed rest marker is a valid rest element inside of a
/// array or object assignment target and converts it to an unknown assignment target if not.
/// A rest element must be:
///
/// * the last element
/// * not followed by a trailing comma
/// * not have a default value
fn validate_rest_pattern(
	p: &mut Parser,
	rest: CompletedMarker,
	end_token: SyntaxKind,
	recovery: &ParseRecovery,
) -> ConditionalSyntax {
	if p.at(end_token) {
		return Valid(rest);
	}

	if p.at(T![=]) {
		let rest_range = rest.range(p);
		let rest_marker = rest.undo_completion(p);
		let default_start = p.cur_tok().range.start;
		let kind = rest.kind();
		p.bump(T![=]);

		if let Ok(recovered) = recovery.recover(p) {
			recovered.undo_completion(p).abandon(p); // append recovered content to parent
		}
		p.error(
			p.err_builder("rest element cannot have a default")
				.primary(
					default_start..p.cur_tok().range.start,
					"Remove the default value here",
				)
				.secondary(rest_range, "Rest element"),
		);

		Invalid(rest_marker.complete(p, kind).into())
	} else if p.at(T![,]) && p.nth_at(1, end_token) {
		p.error(
			p.err_builder("rest element may not have a trailing comma")
				.primary(p.cur_tok().range, "Remove the trailing comma here")
				.secondary(rest.range(p), "Rest element"),
		);
		Invalid(rest.into())
	} else {
		p.error(
			p.err_builder("rest element must be the last element")
				.primary(
					rest.range(p),
					&format!(
							"Move the rest element to the end of the pattern, right before the closing {}",
							end_token.to_string().unwrap(),
						),
				),
		);
		Invalid(rest.into())
	}
}