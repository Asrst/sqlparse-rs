"""Independence contract tests — the default is NO patching.

These tests pin the architecture sqlparse-rs promises:

  1. `import sqlparse_rs` never imports `sqlparse` (the pure-Python
     engine) — neither directly nor transitively.
  2. `import sqlparse_rs` never modifies sqlparse's engine, even when
     sqlparse is already loaded in the same interpreter.
  3. `sqlparse_rs.tokens` stays pure — it never imports sqlparse.
  4. The package works in environments where sqlparse is absent or even
     UNIMPORTABLE.
  5. Patching is an explicit, user-driven action with an exact
     patch()/unpatch() roundtrip.
  6. SQLPARSE_RS_NO_PATCH=1 is a kill-switch that makes patch() a no-op.

Every check runs in a fresh subprocess so no other test module's imports
can contaminate the observation. The source tree (python/) is put on
PYTHONPATH when it carries a built extension; otherwise the installed
package is used.
"""
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "python"
_HAS_SRC_SO = any(SRC.glob("sqlparse_rs/sqlparse_rs.*.so"))
RS_ENV = {**os.environ}
if _HAS_SRC_SO:
    RS_ENV["PYTHONPATH"] = str(SRC)


def _run(code, env=None):
    return subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True, text=True, timeout=120,
        cwd=str(REPO), env=env if env is not None else RS_ENV,
    )


def test_import_does_not_import_sqlparse():
    """Chain-import the whole package; sqlparse must stay out of sys.modules."""
    r = _run(
        "import sys\n"
        "import sqlparse_rs\n"
        "import sqlparse_rs.tokens\n"
        "import sqlparse_rs.sql\n"
        "import sqlparse_rs._stream\n"
        "print('sqlparse' in sys.modules)\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "False", (
        "importing sqlparse_rs pulled in the pure-Python sqlparse package"
    )


def test_engine_untouched_by_import():
    """With sqlparse already loaded, importing sqlparse_rs changes nothing."""
    r = _run(
        "import sqlparse, sqlparse.lexer\n"
        "before = (sqlparse.parse, sqlparse.format, sqlparse.parsestream,\n"
        "          sqlparse.lexer.tokenize, sqlparse.lexer.Lexer.get_tokens)\n"
        "import sqlparse_rs, sqlparse_rs.tokens, sqlparse_rs.sql\n"
        "after = (sqlparse.parse, sqlparse.format, sqlparse.parsestream,\n"
        "         sqlparse.lexer.tokenize, sqlparse.lexer.Lexer.get_tokens)\n"
        "print(before == after)\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "True", (
        "importing sqlparse_rs mutated the pure-Python engine"
    )


def test_tokens_module_is_pure():
    """sqlparse_rs.tokens must not import sqlparse under any import order."""
    r = _run(
        "import sys\n"
        "import sqlparse_rs.tokens as T\n"
        "print('sqlparse' in sys.modules)\n"
        "assert str(T.Keyword.DML) == 'Token.Keyword.DML'\n"
        "assert T.Keyword.DML in T.Keyword\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.splitlines()[0].strip() == "False"


def test_works_when_sqlparse_unimportable():
    """Even a hard ImportError for sqlparse must not break sqlparse_rs.

    sys.modules[m] = None makes `import m` raise ImportError immediately,
    simulating an environment where the pure-Python package is absent.
    """
    r = _run(
        "import sys\n"
        "for m in ('sqlparse', 'sqlparse.lexer', 'sqlparse.tokens',\n"
        "          'sqlparse.sql', 'sqlparse.exceptions'):\n"
        "    sys.modules[m] = None\n"
        "import sqlparse_rs\n"
        "import sqlparse_rs.tokens as T\n"
        "import sqlparse_rs.sql\n"
        "toks = list(sqlparse_rs.tokenize('select 1'))\n"
        "assert toks[0] == ('Token.Keyword.DML', 'select'), toks\n"
        "assert sqlparse_rs.split('a; b') == ['a;', 'b']\n"
        "print('ok')\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "ok"


def test_patch_unpatch_roundtrip():
    """Explicit patch() redirects the engine; unpatch() restores it exactly."""
    r = _run(
        "import sqlparse, sqlparse.lexer\n"
        "orig = (sqlparse.parse, sqlparse.format, sqlparse.parsestream,\n"
        "        sqlparse.lexer.tokenize)\n"
        "import sqlparse_rs\n"
        "assert not sqlparse_rs.is_patched()\n"
        "sqlparse_rs.patch()\n"
        "assert sqlparse_rs.is_patched()\n"
        "assert sqlparse.parse is not orig[0], 'parse not redirected'\n"
        "assert 'rust' in sqlparse.lexer.tokenize.__qualname__.lower()\n"
        "# the patched engine actually works end to end\n"
        "assert sqlparse.parse('select 1')[0].get_type() == 'SELECT'\n"
        "assert sqlparse_rs.tokens.Keyword is sqlparse.tokens.Keyword\n"
        "sqlparse_rs.unpatch()\n"
        "assert not sqlparse_rs.is_patched()\n"
        "assert sqlparse.parse is orig[0]\n"
        "assert sqlparse.format is orig[1]\n"
        "assert sqlparse.parsestream is orig[2]\n"
        "assert sqlparse.lexer.tokenize is orig[3]\n"
        "print('ok')\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "ok"


def test_patch_honors_no_patch_env():
    """SQLPARSE_RS_NO_PATCH=1 makes patch() a no-op (extra kill-switch)."""
    env = dict(RS_ENV)
    env["SQLPARSE_RS_NO_PATCH"] = "1"
    r = _run(
        "import sqlparse\n"
        "import sqlparse_rs\n"
        "orig = sqlparse.parse\n"
        "sqlparse_rs.patch()\n"
        "assert sqlparse.parse is orig, 'patch() ignored the env kill-switch'\n"
        "assert not sqlparse_rs.is_patched()\n"
        "print('ok')\n",
        env,
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "ok"


def test_patch_noop_without_sqlparse():
    """patch() on a machine without sqlparse must be a clean no-op."""
    r = _run(
        "import sys\n"
        "sys.modules['sqlparse'] = None  # unimportable\n"
        "import sqlparse_rs\n"
        "sqlparse_rs.patch()\n"
        "assert not sqlparse_rs.is_patched()\n"
        "sqlparse_rs.unpatch()  # must also be safe\n"
        "print('ok')\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "ok"


def test_dual_engine_same_interpreter():
    """Both engines live side by side and agree on the same input."""
    r = _run(
        "import sqlparse, sqlparse_rs\n"
        "sql = 'SELECT a FROM t WHERE x = 1'\n"
        "py = [(str(t), v) for t, v in sqlparse.lexer.tokenize(sql)]\n"
        "rs = list(sqlparse_rs.tokenize(sql))\n"
        "assert py == rs, 'engines disagree'\n"
        "print('ok')\n"
    )
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip() == "ok"
