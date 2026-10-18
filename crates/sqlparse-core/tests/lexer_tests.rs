//! Lexer unit tests — every assertion below mirrors an output verified
//! against REAL sqlparse 0.6.0 (empirical probes), or follows directly from
//! the ported rule semantics. The differential test is the full referee.

use sqlparse_core::tokenize;

fn types(input: &str) -> Vec<&'static str> {
    tokenize(input).into_iter().map(|(t, _)| t).collect()
}

fn pairs(input: &str) -> Vec<(&'static str, String)> {
    tokenize(input)
        .into_iter()
        .map(|(t, v)| (t, v.to_string()))
        .collect()
}

#[test]
fn select_one() {
    assert_eq!(
        pairs("SELECT 1"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Literal.Number.Integer", "1".into()),
        ]
    );
}

#[test]
fn from_is_keyword_via_rule13_not_dml() {
    // rule 13 fires before the word rule -> plain Keyword, NOT the dict's DML
    assert_eq!(types("FROM"), vec!["Token.Keyword"]);
    assert_eq!(
        types("SELECT x FROM t"),
        vec![
            "Token.Keyword.DML",
            "Token.Text.Whitespace",
            "Token.Name",
            "Token.Text.Whitespace",
            "Token.Keyword",
            "Token.Text.Whitespace",
            "Token.Name",
        ]
    );
}

#[test]
fn name_after_dot_rule16_is_live() {
    assert_eq!(
        pairs("t.from"),
        vec![
            ("Token.Name", "t".into()),
            ("Token.Punctuation", ".".into()),
            ("Token.Keyword", "from".into()), // rule 13 beats rule 16
        ]
    );
    assert_eq!(
        types("t.select"),
        vec!["Token.Name", "Token.Punctuation", "Token.Name"] // rule 16 keeps select a Name
    );
}

#[test]
fn numbers() {
    assert_eq!(types("1"), vec!["Token.Literal.Number.Integer"]);
    assert_eq!(types("1.5"), vec!["Token.Literal.Number.Float"]);
    assert_eq!(types("1."), vec!["Token.Literal.Number.Float"]);
    assert_eq!(types(".5"), vec!["Token.Literal.Number.Float"]);
    assert_eq!(types("1E5"), vec!["Token.Literal.Number.Float"]);
    assert_eq!(types("1e"), vec!["Token.Name"]); // guard rejects -> word
    assert_eq!(
        pairs("1.5x"),
        vec![
            ("Token.Literal.Number.Float", "1.".into()), // backtracking shrink
            ("Token.Name", "5x".into()),
        ]
    );
    assert_eq!(types("0x1F"), vec!["Token.Literal.Number.Hexadecimal"]);
    assert_eq!(types("-1"), vec!["Token.Literal.Number.Integer"]);
    assert_eq!(
        pairs("12.5.6"),
        vec![
            ("Token.Literal.Number.Float", "12.5".into()),
            ("Token.Literal.Number.Float", ".6".into()),
        ]
    );
}

#[test]
fn strings_and_unclosed() {
    // unclosed string: regex needs the closing quote -> Error + plain lexing
    assert_eq!(
        pairs("SELECT 'abc"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Error", "'".into()),
            ("Token.Name", "abc".into()),
        ]
    );
    assert_eq!(
        pairs("SELECT \"abc"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Error", "\"".into()),
            ("Token.Name", "abc".into()),
        ]
    );
    // backtracking: greedy pair consumption fails at EOF, closes earlier
    assert_eq!(
        pairs("SELECT 'a''b"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Literal.String.Single", "'a'".into()),
            ("Token.Error", "'".into()),
            ("Token.Name", "b".into()),
        ]
    );
    assert_eq!(
        pairs("SELECT 'a\\'b'"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Literal.String.Single", "'a\\'b'".into()),
        ]
    );
    assert_eq!(types("\"q id\""), vec!["Token.Literal.String.Symbol"]);
}

#[test]
fn operator_order_quirks() {
    assert_eq!(pairs("a+-b")[1], ("Token.Operator", "+-".into()));
    assert_eq!(
        types("a-+b"),
        vec![
            "Token.Name",
            "Token.Operator",
            "Token.Operator",
            "Token.Name"
        ]
    );
    assert_eq!(pairs("a->>b")[1], ("Token.Operator", "->>".into()));
    assert_eq!(
        pairs("a#>>b"),
        vec![
            ("Token.Name", "a#".into()), // '#' is a word-continuation char
            ("Token.Operator.Comparison", ">>".into()),
            ("Token.Name", "b".into()),
        ]
    );
    assert_eq!(
        pairs("a?|b"),
        vec![
            ("Token.Name", "a".into()),
            ("Token.Name.Placeholder", "?".into()), // rule 9 beats operators
            ("Token.Operator", "|".into()),
            ("Token.Name", "b".into()),
        ]
    );
    assert_eq!(pairs("a#-b")[1], ("Token.Operator", "-".into()));
}

#[test]
fn multiword_keywords_and_orders() {
    assert_eq!(
        pairs("CASE WHEN"),
        vec![
            ("Token.Keyword", "CASE".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Keyword", "WHEN".into())
        ]
    );
    assert_eq!(
        pairs("LEFT JOIN t")[0],
        ("Token.Keyword", "LEFT JOIN".into())
    );
    assert_eq!(pairs("END IF")[0], ("Token.Keyword", "END IF".into()));
    assert_eq!(pairs("ORDER BY x")[0], ("Token.Keyword", "ORDER BY".into()));
    assert_eq!(types("ASC"), vec!["Token.Keyword.Order"]);
    assert_eq!(
        pairs("DESC NULLS LAST")[0],
        ("Token.Keyword.Order", "DESC NULLS LAST".into())
    );
    assert_eq!(types("GO 2"), vec!["Token.Keyword"]);
    assert_eq!(types("GO"), vec!["Token.Keyword"]);
    assert_eq!(
        pairs("AT TIME ZONE 'UTC'")[0],
        ("Token.Keyword.TZCast", "AT TIME ZONE 'UTC'".into())
    );
    assert_eq!(types("NOT LIKE"), vec!["Token.Operator.Comparison"]);
    assert_eq!(types("IF NOT EXISTS"), vec!["Token.Keyword"]);
    assert_eq!(types("\\dt"), vec!["Token.Generic.Command"]);
    assert_eq!(types("VALUES(1)")[0], "Token.Keyword"); // rule 13 beats name-before-paren
    assert_eq!(types("count(1)")[0], "Token.Name"); // name-before-paren rule
    assert_eq!(
        pairs("CREATE OR REPLACE VIEW")[0],
        ("Token.Keyword.DDL", "CREATE OR REPLACE".into())
    );
    assert_eq!(types("UNION ALL"), vec!["Token.Keyword"]);
    assert_eq!(
        types("NATIONAL CHAR"),
        vec![
            "Token.Keyword",
            "Token.Text.Whitespace",
            "Token.Name.Builtin"
        ]
    );
}

#[test]
fn placeholders() {
    assert_eq!(
        types("%s ? %(n)s :name"),
        vec![
            "Token.Name.Placeholder",
            "Token.Text.Whitespace",
            "Token.Name.Placeholder",
            "Token.Text.Whitespace",
            "Token.Name.Placeholder",
            "Token.Text.Whitespace",
            "Token.Name.Placeholder",
        ]
    );
}

#[test]
fn dollar_quotes_and_multiline_comments() {
    assert_eq!(types("$$body$$"), vec!["Token.Literal"]);
    assert_eq!(types("$tag$body$tag$"), vec!["Token.Literal"]);
    assert_eq!(
        pairs("SELECT $$unclosed"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Error", "$".into()),
            ("Token.Name.Placeholder", "$unclosed".into()),
        ]
    );
    assert_eq!(
        pairs("SELECT $$x$y$$"),
        vec![
            ("Token.Keyword.DML", "SELECT".into()),
            ("Token.Text.Whitespace", " ".into()),
            ("Token.Literal", "$$x$y$$".into()),
        ]
    );
    assert_eq!(
        pairs("/* c */ SELECT")[0],
        ("Token.Comment.Multiline", "/* c */".into())
    );
    assert_eq!(
        pairs("/*+ hint */ SELECT")[0],
        ("Token.Comment.Multiline.Hint", "/*+ hint */".into())
    );
    assert_eq!(
        pairs("SELECT 1 -- x").last(),
        Some(&("Token.Comment.Single", "-- x".to_string()))
    );
}

#[test]
fn brackets_whitespace_error() {
    assert_eq!(
        pairs("[t1].col"),
        vec![
            ("Token.Name", "[t1]".into()),
            ("Token.Punctuation", ".".into()),
            ("Token.Name", "col".into()),
        ]
    );
    assert_eq!(
        types("a.b"),
        vec!["Token.Name", "Token.Punctuation", "Token.Name"]
    );
    // \s+? is lazy: ONE whitespace char per token
    assert_eq!(
        types("a  b")
            .iter()
            .filter(|t| **t == "Token.Text.Whitespace")
            .count(),
        2
    );
    // for-else: single-char Error tokens
    assert_eq!(tokenize("}}}}").len(), 4);
}

#[test]
fn unicode_words_are_names() {
    assert_eq!(types("中文"), vec!["Token.Name"]);
    assert_eq!(types("SELECT 'café'")[2], "Token.Literal.String.Single");
}

#[test]
fn losslessness() {
    let cases = [
        "SELECT 1; SELECT 2;",
        "INSERT INTO t (a, b) VALUES ('x''y', 1.5E3);",
        "-- comment\r\nSELECT [col] FROM \"tbl\" WHERE a > 1 AND b <> 2 -- trail",
        "CREATE FUNCTION f() RETURNS void AS $$ BEGIN RETURN; END; $$ LANGUAGE plpgsql;",
        "weird }}{{ \\\\ \\uo stuff %%% ??? ::=:",
        "中文 idents + ß",
    ];
    for c in cases {
        let joined: String = tokenize(c).iter().map(|(_, v)| *v).collect();
        assert_eq!(joined, c, "lossless round-trip failed for {c:?}");
    }
}
