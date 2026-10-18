//! Splitter unit tests — expectations are the ORACLE's outputs
//! (tools/oracle.py split against real sqlparse 0.6.0), not guesses.

use sqlparse_core::split;

#[test]
fn basic_split() {
    assert_eq!(split("SELECT 1; SELECT 2;"), vec!["SELECT 1;", "SELECT 2;"]);
    assert_eq!(split("SELECT 1;"), vec!["SELECT 1;"]);
    // trailing whitespace-only tail is dropped by the final flush
    assert_eq!(split("SELECT 1;\n"), vec!["SELECT 1;"]);
    // boundary whitespace is stripped by Python's str(stmt).strip()
    assert_eq!(
        split("SELECT 1;\nSELECT 2;\n"),
        vec!["SELECT 1;", "SELECT 2;"]
    );
}

#[test]
fn parens_defer_split() {
    assert_eq!(split("SELECT (1; 2);"), vec!["SELECT (1; 2);"]);
    // unbalanced ')' floors at zero
    assert_eq!(split("SELECT 1));"), vec!["SELECT 1));"]);
}

#[test]
fn standalone_begin_splits() {
    assert_eq!(split("BEGIN; SELECT 1;"), vec!["BEGIN;", "SELECT 1;"]);
}

#[test]
fn begin_transaction_is_one_statement() {
    // issue 826: transaction keyword after BEGIN -> not a block
    assert_eq!(
        split("BEGIN TRANSACTION; SELECT 1;"),
        vec!["BEGIN TRANSACTION;", "SELECT 1;"]
    );
    assert_eq!(split("BEGIN WORK;"), vec!["BEGIN WORK;"]);
}

#[test]
fn begin_end_block_keeps_semicolons() {
    assert_eq!(
        split("BEGIN INSERT 1; INSERT 2; END;"),
        vec!["BEGIN INSERT 1; INSERT 2; END;"]
    );
}

#[test]
fn go_splits() {
    assert_eq!(
        split("SELECT 1 GO 2 SELECT 2"),
        vec!["SELECT 1 GO 2", "SELECT 2"]
    );
}

#[test]
fn case_end_is_not_a_block() {
    // CASE does NOT protect semicolons (only BEGIN blocks do); bare END
    // pops a non-existent stack and still returns -1 -> splits
    assert_eq!(
        split("SELECT CASE WHEN a THEN 1; ELSE 2; END; SELECT 9;"),
        vec!["SELECT CASE WHEN a THEN 1;", "ELSE 2;", "END;", "SELECT 9;"]
    );
}

#[test]
fn comment_before_next_statement() {
    // the comment belongs to stmt1; the Newline (non-EOS) yields stmt1 and
    // opens stmt2; strip() removes the boundary newline
    assert_eq!(
        split("SELECT 1; -- trailing\nSELECT 2;"),
        vec!["SELECT 1; -- trailing", "SELECT 2;"]
    );
}

#[test]
fn create_declare_begin_end() {
    assert_eq!(
        split("CREATE FUNCTION f() AS DECLARE BEGIN RETURN; END;"),
        vec!["CREATE FUNCTION f() AS DECLARE BEGIN RETURN; END;"]
    );
}
