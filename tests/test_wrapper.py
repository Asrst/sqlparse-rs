import pytest

sqlparse_rs = pytest.importorskip("sqlparse_rs")
sqlparse = pytest.importorskip("sqlparse")

import sqlparse.lexer  # noqa: E402


def py_tokens(sql):
    return [(str(t), v) for t, v in sqlparse.lexer.tokenize(sql)]


CASES = [
    "SELECT 1",
    "SELECT abc FROM t;\nselect xyz where q;",
    "SELECT 1, 1.5, 1., .5, 1E5, 1e, 1.5x, 0x1F, -1, 12.5.6;",
    "SELECT 'plain', 'it''s', 'unclosed, \"dq\", \"q\"\"dq\", 'a\\'b'",
    "-- line\r\nSELECT 1; # c\n--+ hint\n# + h\n# plain\nSELECT 2 /* x */ -- t",
    "SELECT /* c */ 1, /*+ h */ 2, /* unclosed, /*/ weird */, /**/, /**/*;",
    "SELECT $$body$$, $tag$x$tag$, $1, $$, $$x$y$$, a$b, $$unclosed, $$$$;",
    "CASE IN VALUES USING FROM AS; SELECT x FROM t WHERE a IN (1,2);",
    "SELECT * FROM a LEFT JOIN b ON x CROSS JOIN c; END IF; ORDER BY x ASC;",
    "INSERT INTO t VALUES (%s, %(name)s, ?, :name, $1, @name);",
    "SELECT [t1].col, a[t2], \\dt;",
    "SELECT AT TIME ZONE 'UTC', x WITH' TIME ZONE 'PST';",
    "a->b, a->>b, a#>>b, a@>b, a<@b, a?|b, a#-b, a+-b, a-+b, a<>b;",
    "SELECT 中文, café, ß, '日本語';",
    "}}}} ,,, ::: := ::=: %%% ?? @@x #x;",
    "SELECT (a; b); x));",
    "BEGIN; SELECT 1; BEGIN TRANSACTION; BEGIN\nINSERT 1;\nEND;",
    "SELECT $$ never closed; SELECT more;",
    "GO 2\nSELECT 1\nGO\nSELECT 2\n",
    "SeLeCt x FrOm t WhErE a In (1) AnD b LiKe 'x' OrDeR bY 1 DeSc;",
    "SELECT 1;\r\nSELECT 2;\rSELECT 3;\n\t SELECT \t 4;\x0c;\n",
    "SELECT " + "("*15 + "1" + ")"*15 + ";",
]


def test_tokenize_parity_vs_sqlparse():
    for sql in CASES:
        assert sqlparse_rs.tokenize(sql) == py_tokens(sql), f"parity failed for {sql!r}"


def test_losslessness():
    for sql in CASES:
        joined = "".join(v for _, v in sqlparse_rs.tokenize(sql))
        assert joined == sql, f"not lossless for {sql!r}"


def test_split_parity_vs_sqlparse():
    split_cases = [c for c in CASES if ";" in c or "GO" in c]
    split_cases += [
        "BEGIN; SELECT 1;",
        "BEGIN INSERT 1; INSERT 2; END;",
        "SELECT 1 GO 2 SELECT 2",
        "SELECT CASE WHEN a THEN 1; ELSE 2; END; SELECT 9;",
        "SELECT 1; -- trailing\nSELECT 2;",
        "CREATE FUNCTION f() AS DECLARE BEGIN RETURN; END;",
        "SELECT 1;\r\nSELECT 2;\r",
    ]
    for sql in split_cases:
        assert sqlparse_rs.split(sql) == sqlparse.split(sql), f"split parity failed for {sql!r}"


def test_types():
    out = sqlparse_rs.tokenize("SELECT 1")
    assert isinstance(out, list)
    assert all(isinstance(t, tuple) and len(t) == 2 for t in out)
    assert all(isinstance(a, str) and isinstance(b, str) for a, b in out)
    assert out[0] == ("Token.Keyword.DML", "SELECT")


def test_version():
    assert isinstance(sqlparse_rs.__version__, str)
