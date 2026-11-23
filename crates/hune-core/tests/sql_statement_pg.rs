//! Tests of the PostgreSQL statement check (7.25, test PS-statement of 16.9a).
//!
//! The file has every example of the table of 7.25 and the bypass attempts: comments between
//! tokens, case, quoted names, nested CTEs, dollar quotes, `E` strings and Unicode escapes.
//! The check also runs with the `clickhouse` dialect unchanged (a guard for 7.12).
//!
//! The tokens come from the lexer of the dialect `postgres` (7.20).

use hune_core::sql::lexer::lex_all_for;
use hune_core::sql::statement::{REFUSED_FUNCTIONS, check_statement_for, check_statement_postgres};
use hune_core::sql::{Dialect, StatementFault, Token, TokenKind, check_statement, lex_all};

/// The tokens of a text. The lexer of the dialect `postgres` (7.20).
fn lex(src: &str) -> Vec<Token> {
    lex_all_for(Dialect::Postgres, src)
}

fn pg(src: &str) -> Result<(), StatementFault> {
    check_statement_postgres(src, &lex(src)).map_err(|e| e.fault)
}

fn accepted(src: &str) {
    assert_eq!(pg(src), Ok(()), "must be accepted: {src:?}");
}

fn refused(src: &str) -> StatementFault {
    match pg(src) {
        Ok(()) => panic!("must be refused: {src:?}"),
        Err(f) => f,
    }
}

fn refused_word(src: &str, word: &str) {
    match refused(src) {
        StatementFault::RefusedWord(r) => assert_eq!(r.name, word, "{src:?}"),
        f => panic!("{src:?}: expected the word {word}, got {f:?}"),
    }
}

fn refused_function(src: &str, name: &str) {
    match refused(src) {
        StatementFault::RefusedFunction(r) => assert_eq!(r.name, name, "{src:?}"),
        f => panic!("{src:?}: expected the function {name}, got {f:?}"),
    }
}

#[test]
fn table_of_7_25_accepted() {
    accepted("SELECT 1");
    accepted("(SELECT 1) UNION ALL (SELECT 2)");
    accepted("WITH x AS (SELECT 1) SELECT * FROM x;");
    accepted("VALUES (1), (2)");
    accepted("TABLE metrics.node_load");
    accepted("SELECT 1 AS \"update\"");
}

#[test]
fn table_of_7_25_refused() {
    assert_eq!(refused("SELECT 1; SELECT 2"), StatementFault::SecondStatement);
    for first in [
        "SHOW server_version",
        "EXPLAIN SELECT 1",
        "DO $$ BEGIN END $$",
        "CALL p()",
        "SET statement_timeout = 0",
        "LISTEN ch",
        "NOTIFY ch",
        "LOCK TABLE t",
        "PREPARE p AS SELECT 1",
        "COPY t TO PROGRAM 'id'",
        "INSERT INTO t VALUES (1)",
        "UPDATE t SET a = 1",
        "DELETE FROM t",
        "MERGE INTO t USING s ON true WHEN MATCHED THEN DELETE",
        "TRUNCATE t",
        "CREATE TABLE t (a int)",
        "DROP TABLE t",
        "BEGIN",
        "COMMIT",
        "ROLLBACK",
        "DECLARE c CURSOR FOR SELECT 1",
        "FETCH ALL FROM c",
        "VACUUM",
        "ANALYZE",
        "REFRESH MATERIALIZED VIEW m",
        "DISCARD ALL",
        "RESET ALL",
        "EXECUTE p",
        "GRANT SELECT ON t TO u",
        "ALTER SYSTEM SET work_mem = 1",
        "SELECTX 1",
        "'SELECT' 1",
        "\"select\" 1",
        "1 SELECT",
    ] {
        assert_eq!(refused(first), StatementFault::FirstWordPostgres, "{first:?}");
    }
    refused_word("SELECT * INTO t2 FROM t", "into");
    refused_word("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d", "delete");
    refused_word("SELECT * FROM t FOR UPDATE", "update");
    assert_eq!(refused("SELECT * FROM t FOR SHARE"), StatementFault::LockingClause);
    refused_function("SELECT pg_sleep(10)", "pg_sleep");
    refused_function("SELECT pg_catalog.\"pg_sleep\" (10)", "pg_sleep");
    refused_function("SELECT set_config('statement_timeout', '0', false)", "set_config");
    assert_eq!(refused("SELECT $1"), StatementFault::Parameter);
    assert_eq!(refused("SELECT U&\"d\\0061t\\0061\""), StatementFault::UnicodeEscape);
    assert_eq!(refused("-- only a comment"), StatementFault::NoFirstWord);
}

#[test]
fn first_word_rules() {
    accepted("select 1");
    accepted("  \n\t-- c\n /* c */ (( (SELECT 1)))");
    accepted("values (1)");
    accepted("table t");
    accepted("TaBlE t");
    assert_eq!(refused(""), StatementFault::NoFirstWord);
    assert_eq!(refused("  ((  "), StatementFault::NoFirstWord);
    assert_eq!(refused("/* a */"), StatementFault::NoFirstWord);
    // A Unicode space is a word byte (7.20), so it joins the first word.
    assert_eq!(refused("\u{a0}SELECT 1"), StatementFault::FirstWordPostgres);
    // A `$__x` macro that was not expanded is not a first word.
    assert_eq!(refused("$__x SELECT 1"), StatementFault::FirstWordPostgres);
}

#[test]
fn one_statement() {
    accepted("SELECT 1;");
    accepted("SELECT 1 ; -- c\n /* d */");
    accepted("SELECT ';'");
    accepted("SELECT \";\" FROM t");
    accepted("SELECT 1 /* ; */ -- ;\n");
    accepted("SELECT $$;$$");
    accepted("SELECT E'a;\\';'");
    assert_eq!(refused("SELECT 1;;"), StatementFault::SecondStatement);
    assert_eq!(refused("SELECT 1; ;"), StatementFault::SecondStatement);
    assert_eq!(refused("SELECT 1 /* a */ ; /* b */ SELECT 2"), StatementFault::SecondStatement);
    assert_eq!(refused("SELECT 1 )) ; SELECT 2"), StatementFault::SecondStatement);
    assert_eq!(refused("(SELECT 1; SELECT 2)"), StatementFault::SecondStatement);
    // A `;` inside a bracket is stricter than 7.25 text: the server refuses it too.
    assert_eq!(refused("SELECT (1; 2)"), StatementFault::SecondStatement);
    assert_eq!(refused("SELECT 1; DROP TABLE t"), StatementFault::SecondStatement);
    assert_eq!(refused("SELECT 1;\nDELETE FROM t"), StatementFault::SecondStatement);
}

#[test]
fn refused_words_at_any_depth() {
    for (src, word) in [
        ("SELECT 1 INTO t", "into"),
        ("SELECT (SELECT 1 INTO t)", "into"),
        ("WITH i AS (INSERT INTO t VALUES (1) RETURNING *) SELECT * FROM i", "insert"),
        ("WITH u AS (UPDATE t SET a = 1 RETURNING *) SELECT * FROM u", "update"),
        ("WITH m AS (MERGE INTO t USING s ON true WHEN MATCHED THEN DO NOTHING RETURNING *) SELECT 1", "merge"),
        ("SELECT * FROM (SELECT * FROM (SELECT 1 AS delete) a) b", "delete"),
        ("SELECT 1 AS insert", "insert"),
        ("SELECT t.update FROM t", "update"),
        ("SELECT update FROM t", "update"),
        ("SELECT [1, into]", "into"),
        ("SELECT f({delete})", "delete"),
    ] {
        refused_word(src, word);
    }
    // Case, comments, blanks and line ends between tokens.
    for src in [
        "SELECT * iNtO t FROM s",
        "SELECT *\nINTO/**/t FROM s",
        "SELECT * /* a /* nested */ b */ INTO t",
        "SELECT * -- c\n INTO t",
        "SELECT *\u{0b}INTO\u{0c}t",
        "WITH a AS (WITH b AS (WITH c AS (DELETE FROM t RETURNING 1) SELECT * FROM c) SELECT * FROM b) SELECT * FROM a",
        "WITH RECURSIVE r AS (SELECT 1 UNION ALL SELECT 2) SELECT * FROM r INTO x",
        "WITH x AS (SELECT 1) DELETE FROM t",
    ] {
        assert!(matches!(refused(src), StatementFault::RefusedWord(_) | StatementFault::FirstWordPostgres), "{src:?}");
    }
    // The word is a name, a string or a comment: accepted.
    accepted("SELECT \"insert\", \"INTO\", \"delete\" FROM \"update\"");
    accepted("SELECT 'insert into t', 'FOR UPDATE'");
    accepted("SELECT 1 -- INTO t\n");
    accepted("SELECT 1 /* DELETE */");
    accepted("SELECT 1 AS into_x, 2 AS updated, 3 AS delete_flag, 4 AS merge$ FROM inserts");
    accepted("SELECT x$into FROM t");
}

#[test]
fn locking_clauses() {
    for src in [
        "SELECT * FROM t FOR SHARE",
        "SELECT * FROM t FOR NO KEY UPDATE",
        "SELECT * FROM t FOR KEY SHARE",
        "SELECT * FROM t for share",
        "SELECT * FROM t FOR /* c */ SHARE",
        "SELECT * FROM t FOR\n-- c\nSHARE NOWAIT",
        "SELECT * FROM t FOR SHARE OF t SKIP LOCKED",
        "SELECT * FROM (SELECT * FROM t FOR SHARE) s",
        "WITH s AS (SELECT * FROM t FOR KEY SHARE) SELECT * FROM s",
        "SELECT * FROM t FOR NO KEY UPDATE OF t",
    ] {
        assert_eq!(refused(src), StatementFault::LockingClause, "{src:?}");
    }
    refused_word("SELECT * FROM t FOR UPDATE NOWAIT", "update");
    refused_word("SELECT * FROM t FOR\n UPDATE", "update");
    // `FOR` alone is legal SQL (a substring).
    accepted("SELECT substring('abc' FROM 1 FOR 2)");
    accepted("SELECT * FROM t FOR \"share\"");
    accepted("SELECT \"for\" FROM t");
}

#[test]
fn parameters() {
    for src in [
        "SELECT $1",
        "SELECT $12",
        "SELECT a FROM t WHERE b = $2",
        "SELECT ($1)",
        "SELECT 1 /* x */ , $1",
        "SELECT $1::int",
    ] {
        assert_eq!(refused(src), StatementFault::Parameter, "{src:?}");
    }
    accepted("SELECT '$1'");
    accepted("SELECT \"$1\"");
    accepted("SELECT $$ $1 $$");
    accepted("SELECT 1 -- $1\n");
    accepted("SELECT $a$ $1 $a$");
    // A word that starts with `$` and has no digit only is a word, not a parameter.
    accepted("SELECT $x1");
}

#[test]
fn unicode_escape_forms() {
    for src in [
        "SELECT U&\"d\\0061t\\0061\"",
        "SELECT u&\"x\"",
        "SELECT U&'d\\0061t\\0061'",
        "SELECT u&'x'",
        "SELECT 1 FROM U&\"t\"",
        "SELECT (U&\"a\")",
        "SELECT 1 AS U&\"update\"",
        "SELECT * FROM t WHERE U&\"a\" = 1",
        "SELECT U&\"\\0070g_sleep\"(10)",
    ] {
        assert_eq!(refused(src), StatementFault::UnicodeEscape, "{src:?}");
    }
    // The prefix is read at the start of a token only.
    accepted("SELECT xU FROM t");
    accepted("SELECT 'U&' AS u");
    accepted("SELECT u & 1");
    accepted("SELECT \"U&\"");
    accepted("SELECT u&&1");
    // The word `U`, a gap-free `&` and a string that a faulty lexer split: refused as a guard.
    let src = "SELECT U&'a'";
    let kind = |kind, start, end| Token { kind, start, end, closed: true };
    let split = [
        kind(TokenKind::Word, 0, 6),
        kind(TokenKind::Blank, 6, 7),
        kind(TokenKind::Word, 7, 8),
        kind(TokenKind::Operator, 8, 9),
        kind(TokenKind::String, 9, src.len()),
    ];
    assert_eq!(check_statement_postgres(src, &split).map_err(|e| e.fault), Err(StatementFault::UnicodeEscape));
}

#[test]
fn refused_functions_all_names() {
    // Every name of the table of 7.25, in the forms of a plain call.
    let names = [
        "pg_sleep",
        "pg_sleep_for",
        "pg_sleep_until",
        "set_config",
        "pg_cancel_backend",
        "pg_terminate_backend",
        "pg_notify",
        "pg_advisory_lock",
        "pg_advisory_xact_lock",
        "pg_advisory_unlock_all",
        "pg_try_advisory_lock",
        "pg_try_advisory_xact_lock_shared",
        "pg_read_file",
        "pg_read_binary_file",
        "pg_ls_dir",
        "pg_stat_file",
        "lo_import",
        "lo_export",
        "dblink",
        "dblink_connect",
        "dblink_exec",
        "dblink_connect_u",
        "query_to_xml",
        "query_to_xml_and_xmlschema",
        "query_to_xmlschema",
        "cursor_to_xml",
        "cursor_to_xmlschema",
        "pg_reload_conf",
        "pg_rotate_logfile",
        "pg_switch_wal",
        "pg_promote",
        "pg_create_restore_point",
        "pg_logical_emit_message",
        "ts_stat",
        "ts_rewrite",
        "crosstab",
        "crosstab2",
        "crosstab4",
        "connectby",
        "xpath_table",
    ];
    for name in names {
        let expected = REFUSED_FUNCTIONS
            .iter()
            .find(|r| if r.prefix { name.starts_with(r.name) } else { name == r.name })
            .unwrap_or_else(|| panic!("{name} has no entry"))
            .name;
        for src in [
            format!("SELECT {name}(1)"),
            format!("SELECT {name} (1)"),
            format!("SELECT {name}\n\t(1)"),
            format!("SELECT {name}/**/(1)"),
            format!("SELECT {name} -- c\n (1)"),
            format!("SELECT {name} /* a /* b */ c */ (1)"),
            format!("SELECT {}(1)", name.to_ascii_uppercase()),
            format!("SELECT \"{name}\"(1)"),
            format!("SELECT \"{name}\" (1)"),
            format!("SELECT pg_catalog.{name}(1)"),
            format!("SELECT pg_catalog.\"{name}\"(1)"),
            format!("SELECT \"pg_catalog\".\"{name}\" /* c */ (1)"),
            format!("SELECT public.{name}()"),
            format!("SELECT * FROM {name}(1)"),
            format!("SELECT * FROM pg_catalog.{name}(1) AS x"),
            format!("SELECT (SELECT {name}(1))"),
            format!("SELECT 1 WHERE {name}(1) IS NULL"),
            format!("WITH x AS (SELECT {name}(1)) SELECT * FROM x"),
            format!("SELECT 1 FROM t ORDER BY {name}(1)"),
            format!("SELECT f({name}(1))"),
            format!("SELECT 1, {name}()"),
            format!("VALUES ({name}(1))"),
            format!("TABLE t ORDER BY {name}(1)"),
        ] {
            match refused(&src) {
                StatementFault::RefusedFunction(r) => assert_eq!(r.name, expected, "{src:?}"),
                f => panic!("{src:?}: {f:?}"),
            }
        }
    }
}

#[test]
fn functions_that_run_sql_from_a_string_are_refused() {
    // Finding H1 of the security review: the text in the string runs on the server with the
    // privileges of the role. These calls hide pg_sleep, pg_terminate_backend and
    // pg_logical_emit_message from the check of the outer text.
    refused_function("SELECT * FROM ts_stat('SELECT pg_sleep(60)::text::tsvector')", "ts_stat");
    refused_function("SELECT * FROM ts_stat('SELECT 1', 'a')", "ts_stat");
    refused_function("SELECT ts_rewrite('a & b'::tsquery, 'SELECT pg_terminate_backend(1)')", "ts_rewrite");
    refused_function("SELECT * FROM crosstab('SELECT 1, 2, 3')", "crosstab");
    refused_function("SELECT * FROM public.connectby('t', 'a', 'b', 'x', 0)", "connectby");
    refused_function("SELECT * FROM xpath_table('k', 'x', 't', '/a', 'true')", "xpath_table");
    refused_function("SELECT pg_logical_emit_message(false, 'p', 'x')", "pg_logical_emit_message");
    refused_function("SELECT pg_terminate_backend(1)", "pg_terminate_backend");
    refused_function("SELECT pg_cancel_backend(1)", "pg_cancel_backend");
    // The functions of the same family that run no text stay possible.
    accepted(
        "SELECT to_tsvector('a b'), ts_rank(to_tsvector('a'), to_tsquery('a')), ts_headline('a', to_tsquery('a'))",
    );
    accepted("SELECT ts_stats FROM t");
}

#[test]
fn refused_function_messages_name_the_function() {
    let f = refused("SELECT pg_sleep(10)");
    assert!(f.message().contains("pg_sleep"), "{}", f.message());
    let f = refused("SELECT dblink_exec('x')");
    assert!(f.message().contains("dblink"), "{}", f.message());
    let f = refused("SELECT 1 INTO t");
    assert!(f.message().contains("into"), "{}", f.message());
}

#[test]
fn refused_function_edges() {
    // A refused name without a call is a name: the check reads calls only.
    accepted("SELECT pg_sleep FROM t");
    accepted("SELECT 'pg_sleep(1)'");
    accepted("SELECT \"pg_sleep\" FROM t");
    accepted("SELECT $$pg_sleep(1)$$");
    accepted("SELECT $f$ set_config(1) $f$");
    accepted("SELECT E'pg_sleep(1)'");
    accepted("SELECT 1 -- pg_sleep(1)\n");
    accepted("SELECT 1 /* pg_sleep(1) */");
    accepted("SELECT /* pg_sleep */ (1)");
    // Names that start like a refused name but are not refused.
    accepted("SELECT pg_sleepy(1)");
    accepted("SELECT my_pg_sleep(1)");
    accepted("SELECT pg_advisory(1)");
    accepted("SELECT pg_try_advisory(1)");
    accepted("SELECT dblin(1)");
    accepted("SELECT pg_read_files(1)");
    accepted("SELECT query_to_xml2(1)");
    accepted("SELECT pg_sleep$(1)");
    accepted("SELECT pg_catalog.pg_sleep");
    accepted("SELECT count(*), now(), lower('A'), abs(-1), pg_backend_pid(), version()");
    accepted("SELECT date_trunc('hour', now()), generate_series(1, 3), extract(epoch FROM now())");
    // A quoted name keeps its case in PostgreSQL. Folding it is stricter and harmless.
    refused_function("SELECT \"PG_SLEEP\"(1)", "pg_sleep");
    // A doubled quote is one quote mark of the name.
    accepted("SELECT \"pg\"\"sleep\"(1)");
    // A cast hides a function: the limit that 7.25 names. The role and the transaction close it.
    accepted("SELECT 'pg_sleep'::regproc");
    accepted("SELECT CAST('pg_sleep' AS regproc)");
}

#[test]
fn dollar_quotes_hide_words() {
    accepted("SELECT $$ INTO t; DELETE FROM x $$");
    accepted("SELECT $a$ FOR UPDATE $a$");
    accepted("SELECT $q$ $$ pg_sleep(1) $$ $q$");
    accepted("SELECT $tag$ $1 U&\"a\" $tag$ AS x");
    // The dollar quote ends at its own tag: the words after it count.
    refused_word("SELECT $a$ x $a$ INTO t", "into");
    refused_word("SELECT $$ x $$ , $$ y $$ INTO t", "into");
    // A different tag does not close the quote, so the quote is open: the lexer refuses it,
    // and the check also refuses a token that does not close.
    assert_eq!(refused("SELECT $a$ x $b$ INTO t"), StatementFault::UnclosedToken);
    assert_eq!(refused("SELECT $$ x"), StatementFault::UnclosedToken);
    // A `$` in a word does not start a dollar quote.
    refused_word("SELECT a$b$ INTO t", "into");
}

#[test]
fn strings_and_e_strings() {
    // A standard string: a backslash is an ordinary byte, so `'a\'` closes at the second quote.
    refused_word("SELECT 'a\\' INTO t -- '", "into");
    // An E string: a backslash escapes the next byte, so the word stays inside.
    accepted("SELECT E'a\\' INTO t'");
    accepted("SELECT e'a\\' DELETE FROM t'");
    accepted("SELECT E'\\\\' ");
    // After the escaped backslash the quote closes: the words count again.
    refused_word("SELECT E'\\\\' INTO t", "into");
    refused_word("SELECT E'x'' y' INTO t", "into");
    accepted("SELECT E'x'' INTO y'");
    // A string that does not close.
    assert_eq!(refused("SELECT 'abc"), StatementFault::UnclosedToken);
    assert_eq!(refused("SELECT E'abc\\'"), StatementFault::UnclosedToken);
    assert_eq!(refused("SELECT \"abc"), StatementFault::UnclosedToken);
    assert_eq!(refused("SELECT 1 /* abc"), StatementFault::UnclosedToken);
    assert_eq!(refused("SELECT 1 /* a /* b */"), StatementFault::UnclosedToken);
    // Prefixed strings are strings.
    accepted("SELECT B'101', X'1F', N'abc'");
    accepted("SELECT x'a;b'");
    // `xE'a'` is a word and a string (7.20).
    accepted("SELECT xE'a'");
    // A Unicode escape string carries the prefix: refused whatever it holds.
    assert_eq!(refused("SELECT U&'\\0049NTO'"), StatementFault::UnicodeEscape);
}

#[test]
fn nested_ctes() {
    accepted("WITH a AS (SELECT 1), b AS (SELECT * FROM a) SELECT * FROM b");
    accepted("WITH a AS (WITH b AS (SELECT 1) SELECT * FROM b) SELECT * FROM a");
    accepted("WITH RECURSIVE t(n) AS (VALUES (1) UNION ALL SELECT n + 1 FROM t WHERE n < 5) SELECT * FROM t");
    accepted("WITH a AS MATERIALIZED (SELECT 1), b AS NOT MATERIALIZED (SELECT 2) SELECT * FROM a, b");
    accepted("SELECT * FROM (WITH a AS (SELECT 1) SELECT * FROM a) s");
    for src in [
        "WITH a AS (SELECT 1), b AS (DELETE FROM t RETURNING *) SELECT * FROM a",
        "WITH a AS (WITH b AS (INSERT INTO t SELECT 1 RETURNING *) SELECT * FROM b) SELECT * FROM a",
        "SELECT * FROM (WITH a AS (UPDATE t SET x = 1 RETURNING *) SELECT * FROM a) s",
        "WITH a AS (SELECT 1) INSERT INTO t SELECT * FROM a",
        "WITH a AS (SELECT 1) UPDATE t SET x = 1",
        "WITH a AS (SELECT 1) DELETE FROM t",
        "WITH a AS (SELECT 1) MERGE INTO t USING a ON true WHEN MATCHED THEN DELETE",
        "WITH a AS (SELECT 1) SELECT * INTO t FROM a",
    ] {
        assert!(matches!(refused(src), StatementFault::RefusedWord(_)), "{src:?}");
    }
}

#[test]
fn bypass_attempts() {
    // Comments between the tokens of a pair.
    refused_word("SELECT 1 /**/ INTO /**/ t", "into");
    assert_eq!(refused("SELECT * FROM t FOR/**/SHARE"), StatementFault::LockingClause);
    refused_function("SELECT pg_sleep/**/(1)", "pg_sleep");
    refused_function("SELECT pg_catalog./**/pg_sleep/* a */(1)", "pg_sleep");
    refused_function("SELECT pg_catalog . pg_sleep (1)", "pg_sleep");
    // A comment mark inside a string does not hide the rest.
    refused_word("SELECT '--' INTO t", "into");
    refused_word("SELECT '/*' INTO t -- */", "into");
    // A quoted name with a comment mark.
    refused_word("SELECT \"--\" INTO t", "into");
    // Operators next to words: `-` and `/` before a comment mark.
    refused_word("SELECT 1/**/INTO t", "into");
    refused_word("SELECT 1+-/**/INTO t", "into");
    // Case and mixed forms.
    refused_function("SELECT Pg_SlEeP(1)", "pg_sleep");
    refused_function("SELECT DBLINK_EXEC('x')", "dblink");
    refused_function("SELECT PG_ADVISORY_LOCK(1)", "pg_advisory_");
    // A function of the list inside a deeper call, a subquery and a cast.
    refused_function("SELECT abs((SELECT pg_sleep(1)))", "pg_sleep");
    refused_function("SELECT CAST(pg_sleep(1) AS text)", "pg_sleep");
    refused_function("SELECT pg_sleep(1)::text", "pg_sleep");
    refused_function("SELECT (pg_sleep(1))", "pg_sleep");
    refused_function("SELECT ARRAY[pg_sleep(1)]", "pg_sleep");
    refused_function("SELECT x FROM generate_series(1, pg_sleep(1))", "pg_sleep");
    // A call by the operator form or by a lateral join.
    refused_function("SELECT * FROM t, LATERAL pg_sleep(1)", "pg_sleep");
    refused_function("SELECT * FROM t CROSS JOIN LATERAL (SELECT pg_sleep(1)) s", "pg_sleep");
    // A parameter and a Unicode escape in the middle of a long text.
    assert_eq!(refused("SELECT a, b, c FROM t WHERE d IN (1, 2, $3)"), StatementFault::Parameter);
    assert_eq!(refused("SELECT a FROM t WHERE \"a\" = 1 AND U&\"b\" = 2"), StatementFault::UnicodeEscape);
    // Operator tokens with a `$`-like byte, `?` and a backtick are not words.
    accepted("SELECT a ? b, a ?| b, a @> b, a -> 'x', a ->> 'x', a #> '{x}', a::text FROM t");
    accepted("SELECT 1 WHERE a ~ 'x' AND b !~* 'y'");
    // Names with Unicode letters are words that match nothing.
    accepted("SELECT \u{e9}t\u{e9} FROM t");
}

#[test]
fn faults_have_positions() {
    let src = "SELECT 1\nFROM t\nFOR SHARE";
    let e = check_statement_postgres(src, &lex(src)).unwrap_err();
    assert_eq!((e.position.line, e.position.column), (3, 1));
    let src = "SELECT 1,\n  pg_sleep(1)";
    let e = check_statement_postgres(src, &lex(src)).unwrap_err();
    assert_eq!((e.position.line, e.position.column), (2, 3));
    let src = "SHOW x";
    let e = check_statement_postgres(src, &lex(src)).unwrap_err();
    assert_eq!(e.position.offset, 0);
    let src = "-- c";
    let e = check_statement_postgres(src, &lex(src)).unwrap_err();
    assert_eq!(e.position.offset, src.len());
    assert!(e.to_string().contains("line 1"));
}

#[test]
fn dialect_choice_and_clickhouse_unchanged() {
    // The dialect picks the check.
    let pg_text = "SELECT 1 INTO t";
    let t = lex(pg_text);
    assert!(check_statement_for(Dialect::Postgres, pg_text, &t).is_err());
    // The ClickHouse check is the same function as before: `INTO` alone is accepted there.
    let ch = |src: &str| check_statement_for(Dialect::ClickHouse, src, &lex_all(src)).map_err(|e| e.fault);
    let ch_direct = |src: &str| check_statement(src, &lex_all(src)).map_err(|e| e.fault);
    for src in [
        "SELECT 1",
        "SELECT 1 INTO x",
        "SELECT 1 FORMAT JSON",
        "SELECT 1 INTO OUTFILE 'x'",
        "SHOW TABLES",
        "VALUES (1)",
        "TABLE t",
        "SELECT (1; 2)",
        "SELECT 1; SELECT 2",
        "WITH 1 AS x INSERT INTO t SELECT x",
        "SELECT pg_sleep(1)",
        "SELECT * FROM t FOR UPDATE",
        "SELECT $1",
        "",
    ] {
        assert_eq!(ch(src), ch_direct(src), "{src:?}");
    }
    assert_eq!(ch("SELECT 1 INTO x"), Ok(()));
    assert_eq!(ch("SELECT pg_sleep(1)"), Ok(()));
    assert_eq!(ch("VALUES (1)"), Err(StatementFault::FirstWord));
    assert_eq!(ch("SELECT (1; 2)"), Ok(()));
    assert_eq!(ch("SELECT 1 FORMAT JSON"), Err(StatementFault::Format), "the ClickHouse messages do not change");
    assert_eq!(StatementFault::FirstWord.message(), "the first word must be SELECT or WITH");
}

#[test]
fn closed_list_has_no_duplicate_and_is_lower_case() {
    let mut seen = std::collections::HashSet::new();
    for r in REFUSED_FUNCTIONS {
        assert_eq!(r.name, r.name.to_ascii_lowercase());
        assert!(seen.insert(r.name), "duplicate {}", r.name);
        assert!(r.message.contains(r.name));
    }
    // The table of 7.25 has 32 names or prefixes.
    assert_eq!(REFUSED_FUNCTIONS.len(), 32);
}

#[test]
fn random_texts_never_panic() {
    // A small deterministic mix of pieces, to run the check on odd token runs.
    let pieces = [
        "SELECT", "WITH", "FOR", "SHARE", "UPDATE", "INTO", "(", ")", "[", "]", "{", "}", ";", "$1", "$$", "$a$", "'",
        "\"", "E'", "U&\"", "U&'", "/*", "*/", "--", "\n", " ", "pg_sleep", "dblink", "x", "1", "::", "||", "\u{e9}",
        "\\", "$",
    ];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..3000 {
        let mut text = String::new();
        for _ in 0..(state % 14 + 1) {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            text.push_str(pieces[(state >> 33) as usize % pieces.len()]);
            text.push(' ');
        }
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let _ = pg(&text);
    }
}
