//! The SQL texts of the probe (8a.12, 8a.13, ID-PG2 to ID-PG4).
//!
//! This module holds the constant statements of the setup check R0, of the account checks R1 to
//! R9 and of the advice checks A1 to A4. They read the catalog only. Each text is a constant. No
//! value of a caller goes in them. The only parameters are the constant arrays [`R7_FUNCTIONS`] and
//! [`R7_ANYWHERE`], the `$1` and the `$2` of [`R7`]. The probe runs each statement in the extended
//! protocol, inside the read-only transaction of 8a.12.
//!
//! # The roles of the account
//!
//! The checks R3 to R8 look at the rights of the login role and at the rights of each role that it is a
//! member of, with or without `INHERIT` (finding M3). A member of a role can run `SET ROLE` to it, and
//! that role can have a right that the login role does not have. The text `pg_has_role(current_user,
//! m.oid, 'MEMBER')` selects these roles: it holds for the login role itself and for each direct or
//! indirect membership. The check R9 refuses an object that the login role, or a role that it is a member
//! of, owns, because an owner can grant itself each right again.
//!
//! The texts are the texts of the tables of 8a.12, with one change. R4, R5 and R6 give the names
//! with `format('%I.%I', schema, name)` and not with `regclass`. A `regclass` leaves out the
//! schema when it is in the `search_path`, and the key `search_path` of the source names the
//! schemas of the dashboards, so the name would be ambiguous in a detail. Each statement gives one column. The checks
//! that list names have `LIMIT 5`, so a hostile catalog cannot make a large answer. The decision
//! about a result is in `probe`, as pure functions.

/// The number of names that a check reads and that a log line holds (8a.12).
pub(crate) const MAX_NAMES: usize = 5;

/// R0: the session parameters, read back at each new connection (8a.4).
pub(crate) const R0: &str = "SELECT current_setting('server_version_num')::int, \
current_setting('default_transaction_read_only'), current_setting('standard_conforming_strings'), \
current_setting('client_encoding'), current_setting('TimeZone'), current_setting('search_path')";

/// R1: the attributes of the role. One row with five `false` passes.
pub(crate) const R1: &str = "SELECT rolsuper, rolcreaterole, rolcreatedb, rolreplication, rolbypassrls \
FROM pg_roles WHERE rolname = current_user";

/// R2: the memberships in a `pg_*` role and in a role with a write attribute (ID-PG3).
pub(crate) const R2: &str = "SELECT r.rolname FROM pg_roles r WHERE r.rolname <> current_user \
AND pg_has_role(current_user, r.oid, 'MEMBER') \
AND (r.rolname LIKE 'pg\\_%' OR r.rolsuper OR r.rolcreaterole OR r.rolcreatedb OR r.rolreplication OR r.rolbypassrls) \
LIMIT 5";

/// R3, first statement: the schemas where the role, or a role that it is a member of, can create.
pub(crate) const R3_SCHEMAS: &str = "SELECT n.nspname FROM pg_namespace n \
WHERE n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp\\_%' \
AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND has_schema_privilege(m.oid, n.oid, 'CREATE')) LIMIT 5";

/// R3, second statement: the right to create in the database.
pub(crate) const R3_DATABASE: &str = "SELECT 'database' WHERE EXISTS (SELECT 1 FROM pg_roles m \
WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_database_privilege(m.oid, current_database(), 'CREATE'))";

/// R4: the tables, views, materialized views and foreign tables that the role can change.
pub(crate) const R4: &str = "SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
AND n.nspname NOT LIKE 'pg\\_toast%' \
AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND (has_table_privilege(m.oid, c.oid, 'INSERT') OR has_table_privilege(m.oid, c.oid, 'UPDATE') \
OR has_table_privilege(m.oid, c.oid, 'DELETE') OR has_table_privilege(m.oid, c.oid, 'TRUNCATE') \
OR has_table_privilege(m.oid, c.oid, 'REFERENCES') OR has_table_privilege(m.oid, c.oid, 'TRIGGER'))) \
LIMIT 5";

/// R5: the sequences that the role can change.
pub(crate) const R5: &str = "SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE c.relkind = 'S' AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND has_sequence_privilege(m.oid, c.oid, 'UPDATE')) LIMIT 5";

/// R6: the columns that the role can write.
pub(crate) const R6: &str = "SELECT format('%I.%I.%I', n.nspname, c.relname, a.attname) FROM pg_attribute a \
JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE a.attacl IS NOT NULL AND a.attnum > 0 AND NOT a.attisdropped \
AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND (has_column_privilege(m.oid, a.attrelid, a.attnum, 'INSERT') \
OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'UPDATE') \
OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'REFERENCES'))) LIMIT 5";

/// R7: the functions of `dblink`, the privileged functions of [`R7_FUNCTIONS`] and the functions of
/// [`R7_ANYWHERE`] in each schema. `$1` and `$2` are text arrays. The role, or a role that it is a member of,
/// MUST NOT be able to run them: `PUBLIC` counts.
pub(crate) const R7: &str = "SELECT p.oid::regprocedure::text FROM pg_proc p \
WHERE (p.proname LIKE 'dblink%' OR (p.pronamespace = 'pg_catalog'::regnamespace AND p.proname::text = ANY ($1::text[])) \
OR p.proname::text = ANY ($2::text[])) \
AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND has_function_privilege(m.oid, p.oid, 'EXECUTE')) LIMIT 5";

/// The value of `$1` of [`R7`]: the file functions, the server functions and the functions that run
/// SQL text from a string or that write to the log of the server, which the role MUST NOT run. The
/// functions `pg_cancel_backend` and `pg_terminate_backend` signal another session of the same role.
/// `pg_logical_emit_message` writes to the write-ahead log, also in a read-only transaction. `ts_stat` and
/// `ts_rewrite` run the SQL text of their argument (finding H1).
pub(crate) const R7_FUNCTIONS: [&str; 16] = [
    "pg_read_file",
    "pg_read_binary_file",
    "pg_ls_dir",
    "pg_stat_file",
    "lo_import",
    "lo_export",
    "pg_reload_conf",
    "pg_rotate_logfile",
    "pg_switch_wal",
    "pg_promote",
    "pg_create_restore_point",
    "pg_logical_emit_message",
    "pg_cancel_backend",
    "pg_terminate_backend",
    "ts_stat",
    "ts_rewrite",
];

/// The value of `$2` of [`R7`]: the functions of the contrib modules `tablefunc` and `xml2` that run SQL
/// text from a string. They are in the schema of the extension, so [`R7`] looks in each schema.
pub(crate) const R7_ANYWHERE: [&str; 6] =
    ["crosstab", "crosstab2", "crosstab3", "crosstab4", "connectby", "xpath_table"];

/// R8: the foreign data wrappers and the foreign servers that the role can use.
pub(crate) const R8: &str = "SELECT w.fdwname FROM pg_foreign_data_wrapper w \
WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND has_foreign_data_wrapper_privilege(m.oid, w.oid, 'USAGE')) \
UNION ALL SELECT s.srvname FROM pg_foreign_server s \
WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') \
AND has_server_privilege(m.oid, s.oid, 'USAGE'))";

/// R9: the objects that the role, or a role that it is a member of, owns. An owner can grant itself each
/// right again (finding M3). The statement reads the six catalogs of the objects that a dashboard role
/// has no reason to own. A name has a kind in front: `table metrics.t`.
pub(crate) const R9: &str = "SELECT format('table %I.%I', n.nspname, c.relname) FROM pg_class c \
JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE c.relkind NOT IN ('i', 'I', 't') AND n.nspname NOT LIKE 'pg\\_toast%' \
AND pg_has_role(current_user, c.relowner, 'MEMBER') \
UNION ALL SELECT format('schema %I', n.nspname) FROM pg_namespace n \
WHERE pg_has_role(current_user, n.nspowner, 'MEMBER') \
UNION ALL SELECT format('function %s', p.oid::regprocedure::text) FROM pg_proc p \
WHERE pg_has_role(current_user, p.proowner, 'MEMBER') \
UNION ALL SELECT format('type %I.%I', n.nspname, t.typname) FROM pg_type t \
JOIN pg_namespace n ON n.oid = t.typnamespace \
WHERE t.typrelid = 0 AND t.typcategory <> 'A' AND pg_has_role(current_user, t.typowner, 'MEMBER') \
UNION ALL SELECT format('database %I', d.datname) FROM pg_database d \
WHERE pg_has_role(current_user, d.datdba, 'MEMBER') \
UNION ALL SELECT format('large object %s', l.oid) FROM pg_largeobject_metadata l \
WHERE pg_has_role(current_user, l.lomowner, 'MEMBER') LIMIT 5";

/// A1: a language that runs `DO` blocks and that the role can use.
pub(crate) const A1: &str = "SELECT l.lanname FROM pg_language l \
WHERE l.laninline <> 0 AND has_language_privilege(current_user, l.oid, 'USAGE')";

/// A2: the right to make temporary tables.
pub(crate) const A2: &str = "SELECT 'temp' WHERE has_database_privilege(current_user, current_database(), 'TEMPORARY')";

/// A3: the functions that the grammar refuses and that the role can still run.
pub(crate) const A3: &str = "SELECT p.oid::regprocedure::text FROM pg_proc p \
WHERE p.pronamespace = 'pg_catalog'::regnamespace \
AND (p.proname IN ('pg_sleep', 'pg_sleep_for', 'pg_sleep_until', 'pg_notify') \
OR p.proname LIKE 'pg\\_advisory\\_%' OR p.proname LIKE 'pg\\_try\\_advisory\\_%' \
OR p.proname LIKE 'query\\_to\\_xml%' OR p.proname LIKE 'cursor\\_to\\_xml%') \
AND has_function_privilege(current_user, p.oid, 'EXECUTE') LIMIT 5";

/// A4: the limit of temporary files.
pub(crate) const A4: &str = "SELECT current_setting('temp_file_limit')";

/// Every statement of the probe, in the order of the run, with its name.
#[allow(dead_code, reason = "The tests walk the list.")]
pub(crate) const ALL: [(&str, &str); 15] = [
    ("R0", R0),
    ("R1", R1),
    ("R2", R2),
    ("R3_SCHEMAS", R3_SCHEMAS),
    ("R3_DATABASE", R3_DATABASE),
    ("R4", R4),
    ("R5", R5),
    ("R6", R6),
    ("R7", R7),
    ("R8", R8),
    ("R9", R9),
    ("A1", A1),
    ("A2", A2),
    ("A3", A3),
    ("A4", A4),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_statement_is_one_read_only_select() {
        for (name, sql) in ALL {
            assert!(sql.starts_with("SELECT "), "{name}");
            assert!(!sql.contains(';'), "{name}: a second statement is possible");
            assert!(!sql.contains("--") && !sql.contains("/*"), "{name}: a comment");
            for word in ["INSERT INTO", "UPDATE ", "DELETE FROM", "DROP ", "ALTER ", "GRANT ", "COPY ", "CALL "] {
                assert!(!sql.contains(word), "{name} holds {word}");
            }
        }
    }

    /// Writes an array of names as the text of a PostgreSQL array literal.
    fn array_text(names: &[&str]) -> String {
        let items: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
        format!("ARRAY[{}]::text[]", items.join(", "))
    }

    #[test]
    fn the_self_check_holds_the_statements_of_the_probe() {
        // The script of the fixture runs the statements of R1 to R9 with psql. Each statement is the text of
        // this module, with the two arrays of R7 written out. The test fails when one of them changes alone.
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/postgres-source/selfcheck.sh");
        let script = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let r7 = R7.replace("$1::text[]", &array_text(&R7_FUNCTIONS)).replace("$2::text[]", &array_text(&R7_ANYWHERE));
        for (name, text) in [
            ("R1", R1.to_owned()),
            ("R2", R2.to_owned()),
            ("R3_SCHEMAS", R3_SCHEMAS.to_owned()),
            ("R3_DATABASE", R3_DATABASE.to_owned()),
            ("R4", R4.to_owned()),
            ("R5", R5.to_owned()),
            ("R6", R6.to_owned()),
            ("R7", r7),
            ("R8", R8.to_owned()),
            ("R9", R9.to_owned()),
        ] {
            assert!(script.contains(&format!("{text};\n")), "{name} differs in selfcheck.sh");
        }
    }

    #[test]
    fn only_r7_has_parameters() {
        for (name, sql) in ALL {
            assert_eq!(sql.contains("$1"), name == "R7", "{name}");
            assert_eq!(sql.contains("$2"), name == "R7", "{name}");
            assert!(!sql.contains("$3"), "{name}");
        }
    }

    #[test]
    fn the_checks_of_the_rights_look_at_each_role_that_the_account_is_a_member_of() {
        // Finding M3: a membership without INHERIT still allows `SET ROLE`. R3 to R8 ask the rights of each
        // member role with `has_*_privilege(m.oid, ...)`, and none of them asks `current_user` directly.
        for (name, sql) in ALL {
            if ["R3_SCHEMAS", "R3_DATABASE", "R4", "R5", "R6", "R7", "R8"].contains(&name) {
                assert!(sql.contains("pg_has_role(current_user, m.oid, 'MEMBER')"), "{name}");
                assert!(!sql.contains("privilege(current_user"), "{name}");
            }
        }
    }

    #[test]
    fn r9_reads_the_owner_of_the_six_kinds_of_object() {
        for column in ["relowner", "nspowner", "proowner", "typowner", "datdba", "lomowner"] {
            assert!(R9.contains("pg_has_role(current_user, ") && R9.contains(column), "{column}");
        }
        for catalog in ["pg_class", "pg_namespace", "pg_proc", "pg_type", "pg_database", "pg_largeobject_metadata"] {
            assert!(R9.contains(catalog), "{catalog}");
        }
    }

    #[test]
    fn the_functions_that_run_text_or_signal_are_in_r7() {
        // Finding H1: SQL text inside a string, signals to the sessions of the same role, and the log of the
        // server. `PUBLIC` runs these functions by default.
        for name in ["pg_logical_emit_message", "pg_terminate_backend", "pg_cancel_backend", "ts_stat", "ts_rewrite"] {
            assert!(R7_FUNCTIONS.contains(&name), "{name}");
        }
        for name in ["crosstab", "connectby", "xpath_table"] {
            assert!(R7_ANYWHERE.contains(&name), "{name}");
        }
        // The three names that R7 holds do not stay in the advice A3.
        assert!(!A3.contains("pg_cancel_backend") && !A3.contains("pg_terminate_backend"));
    }

    #[test]
    fn the_lists_of_names_have_a_limit_of_five() {
        for (name, sql) in ALL {
            let lists = ["R2", "R3_SCHEMAS", "R4", "R5", "R6", "R7", "R9", "A3"];
            if lists.contains(&name) {
                assert!(sql.ends_with("LIMIT 5"), "{name}");
            }
        }
        assert_eq!(MAX_NAMES, 5);
    }

    #[test]
    fn like_patterns_escape_the_underscore() {
        assert!(R2.contains(r"'pg\_%'"));
        assert!(R3_SCHEMAS.contains(r"'pg\_toast%'") && R3_SCHEMAS.contains(r"'pg\_temp\_%'"));
        assert!(A3.contains(r"'pg\_advisory\_%'") && A3.contains(r"'pg\_try\_advisory\_%'"));
        assert!(A3.contains(r"'query\_to\_xml%'") && A3.contains(r"'cursor\_to\_xml%'"));
    }

    #[test]
    fn r7_lists_the_functions_of_the_spec() {
        let expected = [
            "pg_read_file",
            "pg_read_binary_file",
            "pg_ls_dir",
            "pg_stat_file",
            "lo_import",
            "lo_export",
            "pg_reload_conf",
            "pg_rotate_logfile",
            "pg_switch_wal",
            "pg_promote",
            "pg_create_restore_point",
            "pg_logical_emit_message",
            "pg_cancel_backend",
            "pg_terminate_backend",
            "ts_stat",
            "ts_rewrite",
        ];
        assert_eq!(R7_FUNCTIONS, expected);
        assert!(R7.contains("'dblink%'"));
    }
}
