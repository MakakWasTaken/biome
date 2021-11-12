//! Extended AST node definitions for statements which are unique and special enough to generate code for manually

use crate::{ast::*, syntax_node::SyntaxNode, SyntaxKind, SyntaxKind::*, SyntaxNodeExt, T};

/// Either a statement or a declaration such as a function
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StmtListItem {
	Stmt(JsAnyStatement),
	Decl(Decl),
}

impl AstNode for StmtListItem {
	fn can_cast(kind: SyntaxKind) -> bool {
		JsAnyStatement::can_cast(kind) || Decl::can_cast(kind)
	}

	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if JsAnyStatement::can_cast(syntax.kind()) {
			Some(StmtListItem::Stmt(JsAnyStatement::cast(syntax)?))
		} else {
			Some(StmtListItem::Decl(Decl::cast(syntax)?))
		}
	}

	fn syntax(&self) -> &SyntaxNode {
		match self {
			StmtListItem::Stmt(stmt) => stmt.syntax(),
			StmtListItem::Decl(decl) => decl.syntax(),
		}
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum JsVariableKind {
	Const,
	Let,
	Var,
}

impl JsVariableDeclaration {
	/// Whether the declaration is a const declaration
	pub fn is_const(&self) -> bool {
		self.variable_kind() == Ok(JsVariableKind::Const)
	}

	/// Whether the declaration is a let declaration
	pub fn is_let(&self) -> bool {
		self.variable_kind() == Ok(JsVariableKind::Let)
	}

	/// Whether the declaration is a let declaration
	pub fn is_var(&self) -> bool {
		self.variable_kind() == Ok(JsVariableKind::Const)
	}

	pub fn variable_kind(&self) -> SyntaxResult<JsVariableKind> {
		let token_kind = self.kind_token().map(|t| t.kind())?;

		Ok(match token_kind {
			T![const] => JsVariableKind::Const,
			T![let] => JsVariableKind::Let,
			T![var] => JsVariableKind::Var,
			_ => unreachable!(),
		})
	}
}

impl ImportDecl {
	/// The source of the import, such as `import a from "a"` ("a"), or `import "foo"` ("foo")
	pub fn source(&self) -> Option<Literal> {
		self.syntax()
			.children()
			.find_map(|x| x.try_to::<Literal>().filter(|x| x.is_string()))
	}
}

impl ExportDecl {
	/// The source of the export, such as `export a from "a"` ("a"), or `export "foo"` ("foo")
	pub fn source(&self) -> Option<Literal> {
		self.syntax().children().find_map(|x| {
			x.children()
				.find_map(|x| x.try_to::<Literal>().filter(|x| x.is_string()))
		})
	}
}

impl Specifier {
	pub fn as_token(&self) -> Option<SyntaxToken> {
		self.syntax()
			.children_with_tokens()
			.filter_map(|x| x.into_token())
			.nth(1)
	}

	pub fn alias(&self) -> Option<Name> {
		self.syntax().children().nth(1).and_then(|x| x.try_to())
	}
}

impl WildcardImport {
	pub fn alias(&self) -> Option<Name> {
		self.syntax().children().find_map(|x| x.try_to())
	}
}

impl JsAnySwitchClause {
	pub fn into_case(self) -> Option<JsCaseClause> {
		if let JsAnySwitchClause::JsCaseClause(clause) = self {
			Some(clause)
		} else {
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::*;

	#[test]
	fn var_decl_let_token() {
		let parsed = parse_text("/* */let a = 5;", 0).tree();
		let var_decl = parsed
			.statements()
			.iter()
			.find_map(|stmt| ast::JsVariableDeclarationStatement::cast(stmt.syntax().clone()));

		assert!(var_decl.is_some());
	}
}

impl TsEnumMember {
	pub fn string_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, STRING)
	}
}
