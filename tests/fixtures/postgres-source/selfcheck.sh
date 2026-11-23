#!/bin/sh
# Self check of the fixture of the job `pgsource` (16.9a): does init.sql give the roles and the data
# that the cases expect? It does not need the source code of Hune.
#
# Use: selfcheck.sh CONTAINER
# The container runs the compose project tests/compose/pgsource.yaml. The script uses `docker exec` and the
# psql of the container, so it sends nothing outside the container.
# Environment (mandatory): PGS_ADMIN_PASSWORD, PGS_RO_PASSWORD, PGS_RW_PASSWORD, PGS_BAD_PASSWORD,
#   PGS_LEGACY_PASSWORD. Environment (optional): DOCKER (default docker).
# Output: one line "ok" or "FAIL" for each check, named FXnn. Exit code: 0 when all checks pass, 1 else.
#
# The statements R1 to R9 are the statements of 8a.12, with the arrays of R7 written out. A unit test of the
# crate (probe_sql.rs, `the_self_check_holds_the_statements_of_the_probe`) makes sure that they are the same text.
set -eu

CONTAINER=${1:?use: selfcheck.sh CONTAINER}
DOCKER=${DOCKER:-docker}
: "${PGS_ADMIN_PASSWORD:?}" "${PGS_RO_PASSWORD:?}" "${PGS_RW_PASSWORD:?}" "${PGS_BAD_PASSWORD:?}" "${PGS_LEGACY_PASSWORD:?}"

failed=0

# q ROLE PASSWORD : run the SQL of the standard input as the role. Print the rows without a header.
q() {
    "$DOCKER" exec -i -e "PGPASSWORD=$2" "$CONTAINER" \
        psql -h 127.0.0.1 -U "$1" -d analytics -qtAX -v ON_ERROR_STOP=1 -f - 2>&1
}

# check NAME EXPECTED ACTUAL : compare two texts.
check() {
    if [ "$2" = "$3" ]; then
        echo "ok      $1"
    else
        echo "FAIL    $1: expected '$2', got '$3'"
        failed=$((failed + 1))
    fi
}

probe() {
    # probe CHECK : print the SQL of one check of 8a.12.
    case $1 in
    R1) cat <<'SQL'
SELECT rolsuper, rolcreaterole, rolcreatedb, rolreplication, rolbypassrls FROM pg_roles WHERE rolname = current_user;
SQL
        ;;
    R2) cat <<'SQL'
SELECT r.rolname FROM pg_roles r WHERE r.rolname <> current_user AND pg_has_role(current_user, r.oid, 'MEMBER') AND (r.rolname LIKE 'pg\_%' OR r.rolsuper OR r.rolcreaterole OR r.rolcreatedb OR r.rolreplication OR r.rolbypassrls) LIMIT 5;
SQL
        ;;
    R3) cat <<'SQL'
SELECT n.nspname FROM pg_namespace n WHERE n.nspname NOT LIKE 'pg\_toast%' AND n.nspname NOT LIKE 'pg\_temp\_%' AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_schema_privilege(m.oid, n.oid, 'CREATE')) LIMIT 5;
SELECT 'database' WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_database_privilege(m.oid, current_database(), 'CREATE'));
SQL
        ;;
    R4) cat <<'SQL'
SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\_toast%' AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND (has_table_privilege(m.oid, c.oid, 'INSERT') OR has_table_privilege(m.oid, c.oid, 'UPDATE') OR has_table_privilege(m.oid, c.oid, 'DELETE') OR has_table_privilege(m.oid, c.oid, 'TRUNCATE') OR has_table_privilege(m.oid, c.oid, 'REFERENCES') OR has_table_privilege(m.oid, c.oid, 'TRIGGER'))) LIMIT 5;
SQL
        ;;
    R5) cat <<'SQL'
SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind = 'S' AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_sequence_privilege(m.oid, c.oid, 'UPDATE')) LIMIT 5;
SQL
        ;;
    R6) cat <<'SQL'
SELECT format('%I.%I.%I', n.nspname, c.relname, a.attname) FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE a.attacl IS NOT NULL AND a.attnum > 0 AND NOT a.attisdropped AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND (has_column_privilege(m.oid, a.attrelid, a.attnum, 'INSERT') OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'UPDATE') OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'REFERENCES'))) LIMIT 5;
SQL
        ;;
    R7) cat <<'SQL'
SELECT p.oid::regprocedure::text FROM pg_proc p WHERE (p.proname LIKE 'dblink%' OR (p.pronamespace = 'pg_catalog'::regnamespace AND p.proname::text = ANY (ARRAY['pg_read_file', 'pg_read_binary_file', 'pg_ls_dir', 'pg_stat_file', 'lo_import', 'lo_export', 'pg_reload_conf', 'pg_rotate_logfile', 'pg_switch_wal', 'pg_promote', 'pg_create_restore_point', 'pg_logical_emit_message', 'pg_cancel_backend', 'pg_terminate_backend', 'ts_stat', 'ts_rewrite']::text[])) OR p.proname::text = ANY (ARRAY['crosstab', 'crosstab2', 'crosstab3', 'crosstab4', 'connectby', 'xpath_table']::text[])) AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_function_privilege(m.oid, p.oid, 'EXECUTE')) LIMIT 5;
SQL
        ;;
    R8) cat <<'SQL'
SELECT w.fdwname FROM pg_foreign_data_wrapper w WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_foreign_data_wrapper_privilege(m.oid, w.oid, 'USAGE')) UNION ALL SELECT s.srvname FROM pg_foreign_server s WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_server_privilege(m.oid, s.oid, 'USAGE'));
SQL
        ;;
    R9) cat <<'SQL'
SELECT format('table %I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind NOT IN ('i', 'I', 't') AND n.nspname NOT LIKE 'pg\_toast%' AND pg_has_role(current_user, c.relowner, 'MEMBER') UNION ALL SELECT format('schema %I', n.nspname) FROM pg_namespace n WHERE pg_has_role(current_user, n.nspowner, 'MEMBER') UNION ALL SELECT format('function %s', p.oid::regprocedure::text) FROM pg_proc p WHERE pg_has_role(current_user, p.proowner, 'MEMBER') UNION ALL SELECT format('type %I.%I', n.nspname, t.typname) FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE t.typrelid = 0 AND t.typcategory <> 'A' AND pg_has_role(current_user, t.typowner, 'MEMBER') UNION ALL SELECT format('database %I', d.datname) FROM pg_database d WHERE pg_has_role(current_user, d.datdba, 'MEMBER') UNION ALL SELECT format('large object %s', l.oid) FROM pg_largeobject_metadata l WHERE pg_has_role(current_user, l.lomowner, 'MEMBER') LIMIT 5;
SQL
        ;;
    esac
}

# role_checks ROLE PASSWORD : print the checks that return a row, as one line "R1 R4".
role_checks() {
    rc_hits=""
    for rc_c in R1 R2 R3 R4 R5 R6 R7 R8 R9; do
        rc_out=$(probe "$rc_c" | q "$1" "$2")
        if [ "$rc_c" = R1 ]; then
            # R1 passes with one row of five `f`.
            [ "$rc_out" = "f|f|f|f|f" ] || rc_hits="$rc_hits $rc_c"
        elif [ -n "$rc_out" ]; then
            rc_hits="$rc_hits $rc_c"
        fi
    done
    printf '%s' "$rc_hits" | sed 's/^ //'
}

echo "FX: fixture self check on $CONTAINER"

# FX01: the role of the source passes R1 to R9.
check FX01-hune_ro-passes-R1-to-R9 "" "$(role_checks hune_ro "$PGS_RO_PASSWORD")"

# FX02 to FX09 and FX23 to FX27: each deviant role fails its own check and no other.
check FX02-hune_bad_createrole-fails-R1 "R1" "$(role_checks hune_bad_createrole "$PGS_BAD_PASSWORD")"
check FX03-hune_bad_pgread-fails-R2 "R2" "$(role_checks hune_bad_pgread "$PGS_BAD_PASSWORD")"
check FX04-hune_bad_create-fails-R3 "R3" "$(role_checks hune_bad_create "$PGS_BAD_PASSWORD")"
check FX05-hune_bad_write-fails-R4 "R4" "$(role_checks hune_bad_write "$PGS_BAD_PASSWORD")"
check FX06-hune_bad_seq-fails-R5 "R5" "$(role_checks hune_bad_seq "$PGS_BAD_PASSWORD")"
check FX07-hune_bad_col-fails-R6 "R6" "$(role_checks hune_bad_col "$PGS_BAD_PASSWORD")"
check FX08-hune_bad_dblink-fails-R7 "R7" "$(role_checks hune_bad_dblink "$PGS_BAD_PASSWORD")"
check FX09-hune_bad_fdw-fails-R8 "R8" "$(role_checks hune_bad_fdw "$PGS_BAD_PASSWORD")"
check FX23-hune_bad_signal-fails-R7 "R7" "$(role_checks hune_bad_signal "$PGS_BAD_PASSWORD")"
check FX24-hune_bad_emit-fails-R7 "R7" "$(role_checks hune_bad_emit "$PGS_BAD_PASSWORD")"
check FX25-hune_bad_ts-fails-R7 "R7" "$(role_checks hune_bad_ts "$PGS_BAD_PASSWORD")"
check FX26-hune_bad_noinh-fails-R4-by-a-membership "R4" "$(role_checks hune_bad_noinh "$PGS_BAD_PASSWORD")"
check FX27-hune_bad_owner-fails-R9 "R9" "$(role_checks hune_bad_owner "$PGS_BAD_PASSWORD")"

# FX10: the settings of 8a.13 on hune_ro.
check FX10-hune_ro-settings \
    "on|1min|15s|1GB|off|18" \
    "$(q hune_ro "$PGS_RO_PASSWORD" <<'SQL'
SELECT current_setting('default_transaction_read_only') || '|' || current_setting('statement_timeout') || '|' ||
       current_setting('idle_in_transaction_session_timeout') || '|' || current_setting('temp_file_limit') || '|' ||
       current_setting('jit') || '|' || (SELECT rolconnlimit FROM pg_roles WHERE rolname = current_user);
SQL
    )"

# FX11: the role of the source reads the schema metrics and nothing else.
check FX11-hune_ro-reads-metrics "2160" "$(echo 'SELECT count(*) FROM metrics.node_load;' | q hune_ro "$PGS_RO_PASSWORD")"
check FX12-hune_ro-denied-restricted "ERROR" \
    "$(echo 'SELECT * FROM restricted.hidden;' | q hune_ro "$PGS_RO_PASSWORD" | sed -n 's/^psql:[^ ]* \(ERROR\):.*/\1/p' | head -n 1)"
check FX13-hune_ro-denied-write "ERROR" \
    "$(echo "INSERT INTO metrics.write_target (note) VALUES ('x');" | q hune_ro "$PGS_RO_PASSWORD" | sed -n 's/^psql:[^ ]* \(ERROR\):.*/\1/p' | head -n 1)"

# FX14: the role with all rights can write when the transaction is read-write.
check FX14-hune_rw-can-write "INSERT 0 1" \
    "$(printf 'BEGIN READ WRITE;\nINSERT INTO metrics.write_target (note) VALUES (%s);\nROLLBACK;\n' "'x'" |
        "$DOCKER" exec -i -e "PGPASSWORD=$PGS_RW_PASSWORD" "$CONTAINER" \
            psql -h 127.0.0.1 -U hune_rw -d analytics -v ON_ERROR_STOP=1 -f - 2>&1 | sed -n '/^INSERT/p')"

# FX15 to FX16: the old methods of login. The hash of hune_md5 is an MD5 hash.
check FX15-hune_md5-hash "md5" \
    "$(echo "SELECT substr(rolpassword, 1, 3) FROM pg_authid WHERE rolname = 'hune_md5';" | q postgres "$PGS_ADMIN_PASSWORD")"
check FX16-hba-methods "md5 password" \
    "$(echo "SELECT string_agg(auth_method, ' ' ORDER BY line_number) FROM pg_hba_file_rules WHERE user_name = '{hune_md5}' OR user_name = '{hune_clear}';" | q postgres "$PGS_ADMIN_PASSWORD")"
check FX17-hune_md5-logs-in "1" "$(echo 'SELECT 1;' | q hune_md5 "$PGS_LEGACY_PASSWORD")"
check FX18-hune_clear-logs-in "1" "$(echo 'SELECT 1;' | q hune_clear "$PGS_LEGACY_PASSWORD")"

# FX19 to FX22: the data.
check FX19-big_rows "1000000" "$(echo 'SELECT count(*) FROM metrics.big_rows;' | q postgres "$PGS_ADMIN_PASSWORD")"
check FX20-big_value "50000000" "$(echo 'SELECT octet_length(v) FROM metrics.big_value;' | q postgres "$PGS_ADMIN_PASSWORD")"
check FX21-types_all "4|38" "$(echo "SELECT count(*) || '|' || (SELECT count(*) FROM information_schema.columns WHERE table_schema = 'metrics' AND table_name = 'types_all') FROM metrics.types_all;" | q postgres "$PGS_ADMIN_PASSWORD")"
check FX22-node_load-times "2026-01-01 00:00:00+00|2026-01-01 11:59:00+00" \
    "$(echo "SET timezone = 'UTC'; SELECT min(created_at) || '|' || max(created_at) FROM metrics.node_load;" | q postgres "$PGS_ADMIN_PASSWORD" | sed -n '/|/p')"

if [ "$failed" -eq 0 ]; then
    echo "FX: all checks passed"
    exit 0
fi
echo "FX: $failed check(s) failed"
exit 1
