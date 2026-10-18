//! Token type names — the exact `str(ttype)` strings Python's
//! `sqlparse.tokens` prints, so differential tests compare plain strings.

pub const COMMENT_SINGLE_HINT: &str = "Token.Comment.Single.Hint";
pub const COMMENT_SINGLE: &str = "Token.Comment.Single";
pub const COMMENT_MULTILINE: &str = "Token.Comment.Multiline";
pub const COMMENT_MULTILINE_HINT: &str = "Token.Comment.Multiline.Hint";
pub const NEWLINE: &str = "Token.Text.Whitespace.Newline";
pub const WHITESPACE: &str = "Token.Text.Whitespace";
pub const ASSIGNMENT: &str = "Token.Assignment";
pub const PUNCTUATION: &str = "Token.Punctuation";
pub const WILDCARD: &str = "Token.Wildcard";
pub const NAME: &str = "Token.Name";
pub const NAME_PLACEHOLDER: &str = "Token.Name.Placeholder";
pub const GENERIC_COMMAND: &str = "Token.Generic.Command";
pub const KEYWORD: &str = "Token.Keyword";
pub const KEYWORD_DML: &str = "Token.Keyword.DML";
pub const KEYWORD_DDL: &str = "Token.Keyword.DDL";
pub const KEYWORD_CTE: &str = "Token.Keyword.CTE";
pub const KEYWORD_ORDER: &str = "Token.Keyword.Order";
pub const KEYWORD_TZCAST: &str = "Token.Keyword.TZCast";
pub const NAME_BUILTIN: &str = "Token.Name.Builtin";
pub const NUMBER_HEX: &str = "Token.Literal.Number.Hexadecimal";
pub const NUMBER_FLOAT: &str = "Token.Literal.Number.Float";
pub const NUMBER_INTEGER: &str = "Token.Literal.Number.Integer";
pub const STRING_SINGLE: &str = "Token.Literal.String.Single";
pub const STRING_SYMBOL: &str = "Token.Literal.String.Symbol";
pub const LITERAL: &str = "Token.Literal";
pub const OPERATOR: &str = "Token.Operator";
pub const OPERATOR_COMPARISON: &str = "Token.Operator.Comparison";
pub const ERROR: &str = "Token.Error";

/// Hierarchical `ttype in T.Keyword` (Python TokenType containment):
/// Keyword, .DML, .DDL, .Order, .TZCast, ... all count.
pub fn is_keyword_family(t: &str) -> bool {
    t.starts_with("Token.Keyword")
}

/// `Token.is_whitespace` in sqlparse.sql: `ttype in T.Whitespace`
/// hierarchically — Whitespace and Newline count, Comment.Single does not.
pub fn is_ws_like(t: &str) -> bool {
    t == WHITESPACE || t == NEWLINE
}

/// EOS_TTYPE in statement_splitter.process: (Whitespace, Comment.Single),
/// compared by EQUALITY (tuple membership) — Newline is NOT EOS.
pub fn is_eos(t: &str) -> bool {
    t == WHITESPACE || t == COMMENT_SINGLE
}
