// src/keywords.rs — SQL keyword tables and regex patterns
// Ported from sqlparse/keywords.py

use std::collections::HashMap;

/// Token type as a simple string path, e.g. "Token.Keyword.DML"
pub type TType = &'static str;

// ── canonical token type constants ──────────────────────────────────────────
pub const T_WHITESPACE: TType = "Token.Text.Whitespace";
pub const T_NEWLINE: TType = "Token.Text.Whitespace.Newline";
pub const T_ERROR: TType = "Token.Error";
pub const T_KEYWORD: TType = "Token.Keyword";
pub const T_KEYWORD_DML: TType = "Token.Keyword.DML";
pub const T_KEYWORD_DDL: TType = "Token.Keyword.DDL";
pub const T_KEYWORD_CTE: TType = "Token.Keyword.CTE";
pub const T_KEYWORD_ORDER: TType = "Token.Keyword.Order";
pub const T_KEYWORD_TZ: TType = "Token.Keyword.TZCast";
pub const T_NAME: TType = "Token.Name";
pub const T_NAME_PLACEHOLDER: TType = "Token.Name.Placeholder";
pub const T_NAME_BUILTIN: TType = "Token.Name.Builtin";
pub const T_LITERAL: TType = "Token.Literal";
pub const T_STRING: TType = "Token.Literal.String";
pub const T_STRING_SINGLE: TType = "Token.Literal.String.Single";
pub const T_STRING_SYMBOL: TType = "Token.Literal.String.Symbol";
pub const T_NUMBER: TType = "Token.Literal.Number";
pub const T_NUMBER_INT: TType = "Token.Literal.Number.Integer";
pub const T_NUMBER_FLOAT: TType = "Token.Literal.Number.Float";
pub const T_NUMBER_HEX: TType = "Token.Literal.Number.Hexadecimal";
pub const T_PUNCTUATION: TType = "Token.Punctuation";
pub const T_OPERATOR: TType = "Token.Operator";
pub const T_COMPARISON: TType = "Token.Operator.Comparison";
pub const T_WILDCARD: TType = "Token.Wildcard";
pub const T_COMMENT_SINGLE: TType = "Token.Comment.Single";
pub const T_COMMENT_MULTI: TType = "Token.Comment.Multiline";
pub const T_ASSIGNMENT: TType = "Token.Assignment";
pub const T_COMMAND: TType = "Token.Generic.Command";

/// A regex rule: (pattern, token_type_or_PROCESS_AS_KEYWORD)
/// PROCESS_AS_KEYWORD means look up value in keyword dict
pub const PROCESS_AS_KEYWORD: TType = "__PROCESS_AS_KEYWORD__";

/// The ordered list of (regex_str, ttype) — same order as sqlparse SQL_REGEX
pub fn sql_regex_rules() -> Vec<(&'static str, TType)> {
    vec![
        // Whitespace / newlines
        (r"([ \t\r]+)", T_WHITESPACE),
        (r"(\n)", T_NEWLINE),
        // Single-line comments
        (r"(--.+?(?:\n|$))", T_COMMENT_SINGLE),
        // Multi-line / inline comments
        (r"(/\*[\s\S]*?\*/)", T_COMMENT_MULTI),
        // Dollar-quoted strings (PostgreSQL)  — handled by span resolver in lexer
        // Bit-strings
        (r"(0x[0-9a-fA-F]+)", T_NUMBER_HEX),
        // Numbers
        (r"(\d+\.\d*|\.\d+)([eE][+-]?\d+)?", T_NUMBER_FLOAT),
        (r"(\d+)[eE][+-]?\d+", T_NUMBER_FLOAT),
        (r"(\d+)", T_NUMBER_INT),
        // Placeholder parameters  :name  %(name)s  ?  $1
        (r"(\$\d+)", T_NAME_PLACEHOLDER),
        (r"(%(?:\(\w+\))?s)", T_NAME_PLACEHOLDER),
        (r"(\?)", T_NAME_PLACEHOLDER),
        (r"(:[\w$]+)", T_NAME_PLACEHOLDER),
        // String literals
        (r"(N?'(?:''|[^'])*')", T_STRING_SINGLE),
        (r"(\"(?:\"\"|[^\"])*\")", T_STRING_SYMBOL),
        (r"(`(?:``|[^`])*`)", T_STRING_SYMBOL),
        (r"(\[(?:\]\]|[^\]])*\])", T_STRING_SYMBOL),
        // Operators / comparison
        (r"(!=|<>|<=|>=|!<|!>)", T_COMPARISON),
        (r"(=|<|>)", T_COMPARISON),
        (r"(\|\||&&|::|->>?|#>>?|@>|<@|&&|&<|&>|<<|>>|<->|#-|@\?|@@)", T_OPERATOR),
        (r"([+\-*/%&|^~])", T_OPERATOR),
        // Assignment
        (r"(:=)", T_ASSIGNMENT),
        // Punctuation
        (r"([;,()\[\]\{\}\.:])", T_PUNCTUATION),
        // Wildcard
        (r"(\*)", T_WILDCARD),
        // Identifiers / keywords — matched last
        (r"([^\s\[\](){},.;=<>+\-*/%&|^~!?:]+)", PROCESS_AS_KEYWORD),
    ]
}

// ── keyword dictionaries ─────────────────────────────────────────────────────

pub fn build_keyword_map() -> HashMap<&'static str, TType> {
    let mut m: HashMap<&'static str, TType> = HashMap::new();

    // DML
    for kw in &[
        "SELECT", "INSERT", "UPDATE", "DELETE", "REPLACE", "MERGE",
    ] {
        m.insert(kw, T_KEYWORD_DML);
    }

    // DDL
    for kw in &[
        "ALTER", "ANALYZE", "AUDIT", "COMMENT", "CREATE", "DROP",
        "GRANT", "NOAUDIT", "RENAME", "REVOKE", "TRUNCATE",
    ] {
        m.insert(kw, T_KEYWORD_DDL);
    }

    // CTE
    m.insert("WITH", T_KEYWORD_CTE);

    // ORDER direction
    for kw in &["ASC", "DESC", "NULLS"] {
        m.insert(kw, T_KEYWORD_ORDER);
    }

    // Timezone cast
    for kw in &["AT TIME ZONE"] {
        m.insert(kw, T_KEYWORD_TZ);
    }

    // Builtin type names
    for kw in &[
        "ARRAY", "BIGINT", "BINARY", "BIT", "BLOB", "BOOL", "BOOLEAN",
        "CHAR", "CHARACTER", "DATE", "DATETIME", "DECIMAL", "DOUBLE",
        "ENUM", "FLOAT", "INT", "INTEGER", "JSON", "JSONB", "LONG",
        "LONGBLOB", "LONGTEXT", "MEDIUMBLOB", "MEDIUMINT", "MEDIUMTEXT",
        "NCHAR", "NTEXT", "NUMERIC", "NVARCHAR", "REAL", "SERIAL",
        "SET", "SMALLINT", "TEXT", "TIME", "TIMESTAMP", "TIMESTAMPTZ",
        "TIMETZ", "TINYBLOB", "TINYINT", "TINYTEXT", "UNIQUEIDENTIFIER",
        "UUID", "VARBINARY", "VARCHAR", "XML", "YEAR",
    ] {
        m.insert(kw, T_NAME_BUILTIN);
    }

    // General keywords (not DML/DDL/CTE)
    for kw in &[
        "ABORT", "ABS", "ABSOLUTE", "ACCESS", "ACTION", "ADA", "ADD",
        "ADMIN", "AFTER", "AGGREGATE", "ALIAS", "ALL", "ALLOCATE",
        "AND", "ANY", "ARE", "ARITHABORT", "AS", "ASSERTION",
        "AT", "AUTHORIZATION", "AUTO_INCREMENT", "AVG",
        "BACKUP", "BEFORE", "BEGIN", "BETWEEN", "BIT_LENGTH", "BOTH",
        "BY",
        "CASCADE", "CASCADED", "CASE", "CAST", "CATALOG", "CHECK",
        "CLOSE", "COALESCE", "COLLATE", "COLLATION", "COLUMN",
        "COMMIT", "COMMITTED", "CONCURRENTLY", "CONNECT", "CONNECTION",
        "CONSTRAINT", "CONSTRAINTS", "CONTINUE", "CONVERT",
        "CORRESPONDING", "COUNT", "CROSS", "CURRENT",
        "CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP",
        "CURRENT_USER", "CURSOR",
        "DATABASES", "DATALENGTH", "DAY", "DEALLOCATE",
        "DECLARE", "DEFAULT", "DEFERRABLE", "DEFERRED",
        "DELAYED", "DIAGNOSTICS", "DISCONNECT", "DISTINCT",
        "DOMAIN", "DUPLICATE",
        "EACH", "ELSE", "ELSEIF", "ELSIF", "END", "END-EXEC", "ESCAPE",
        "EVERY", "EXCEPT", "EXCEPTION", "EXEC", "EXECUTE",
        "EXISTS", "EXIT", "EXPLAIN", "EXTERNAL",
        "FALSE", "FETCH", "FILTER", "FIRST", "FOR",
        "FOREIGN", "FOUND", "FROM", "FULL",
        "GENERAL", "GENERATED", "GET", "GLOBAL", "GO", "GOTO", "GROUP",
        "GROUPING",
        "HAVING", "HEADER", "HIGH_PRIORITY", "HOUR",
        "IDENTIFIED", "IDENTITY", "IF", "IGNORE", "ILIKE", "IMMEDIATE",
        "IN", "INDEX", "INDEXED", "INNER", "INTO", "IS",
        "INCLUDE", "INCREMENT", "INITIALLY", "INPUT", "INSENSITIVE",
        "INTERSECT", "INTERVAL",
        "JOIN",
        "KEY", "KEYS",
        "LANGUAGE", "LAST", "LEADING", "LEFT", "LIKE", "LIMIT",
        "LOCAL", "LOWER",
        "MATCH", "MAX", "MAXVALUE", "MIN", "MINUTE", "MINVALUE",
        "MONTH",
        "NAMES", "NATURAL", "NEXT", "NO", "NOCHECK", "NOT", "NOTHING",
        "NOTIFY", "NULL", "NULLIF",
        "OBJECT", "OCTET_LENGTH", "OF", "OFFSET", "ON", "ONLY",
        "OPEN", "OPTION", "OR", "ORDER", "ORDINALITY", "OUTER",
        "OUTPUT", "OVER", "OVERLAPS",
        "PARTIAL", "PARTITION", "PLACING", "POSITION", "PRECEDING",
        "PRIMARY", "PROCEDURE",
        "RAISE", "RANGE", "READ", "RECURSIVE", "REFERENCES", "REPLACE",
        "RESTRICT", "RETURN", "RETURNING", "RETURNS", "RIGHT",
        "ROLLBACK", "ROLLUP", "ROW", "ROWS",
        "SAVEPOINT", "SCHEMA", "SECOND", "SELECT", "SERIALIZABLE",
        "SESSION", "SESSION_USER", "SOME", "SPECIFIC", "SQL",
        "SQLSTATE", "STABLE", "STRICT", "SUBSTRING", "SUM",
        "SYSTEM_USER",
        "TABLE", "TABLES", "TEMPORARY", "THEN", "TO", "TOP",
        "TRAILING", "TRANSACTION", "TRIM", "TRUE",
        "UNBOUNDED", "UNION", "UNIQUE", "UNKNOWN", "UNLISTEN",
        "UNTIL", "UPDATE", "UPPER", "USAGE", "USING",
        "VACUUM", "VALUE", "VALUES", "VARYING", "VIEW", "VOLATILE",
        "WHEN", "WHERE", "WHILE", "WINDOW", "WITH", "WITHIN",
        "WITHOUT", "WORK", "WRITE",
        "YEAR",
        "ZONE",
        // PostgreSQL / dialect extras
        "COPY", "LOCALTIME", "LOCALTIMESTAMP", "NULLS",
        "OVERRIDING", "RETURNING",
        "LANGUAGE", "VOLATILE", "IMMUTABLE", "STABLE",
        "FOREACH", "LOOP", "END LOOP", "END IF", "END CASE",
        "ELSIF", "ELSEIF",
        "RAISE", "NOTICE", "EXCEPTION", "SQLSTATE",
        "PERFORM",
        // MySQL extras
        "STRAIGHT_JOIN", "SQL_CALC_FOUND_ROWS", "SQL_SMALL_RESULT",
        "SQL_BIG_RESULT", "SQL_BUFFER_RESULT",
        "USE", "FORCE", "IGNORE",
        "LOW_PRIORITY", "HIGH_PRIORITY",
        // Snowflake/BigQuery/MSSQL extras
        "ILIKE", "RLIKE", "QUALIFY", "SAMPLE", "TABLESAMPLE",
        "PIVOT", "UNPIVOT", "LATERAL", "APPLY", "CROSS APPLY",
        "OUTER APPLY", "TOP", "NOLOCK", "WITH (NOLOCK)",
        "CONVERT", "IIF",
        "EXCEPT ALL", "INTERSECT ALL", "UNION ALL",
    ] {
        m.entry(kw).or_insert(T_KEYWORD);
    }

    m
}
