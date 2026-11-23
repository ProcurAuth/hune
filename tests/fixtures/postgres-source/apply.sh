#!/bin/sh
# Apply init.sql in a disposable PostgreSQL container, at its first start (16.9a).
#
# The compose project tests/compose/pgsource.yaml mounts this file as
# /docker-entrypoint-initdb.d/10-hune-apply.sh. The entrypoint of the official image runs it one time,
# with the temporary server that listens on the Unix socket only.
#
# The script does two things.
#   1. It puts three lines at the top of pg_hba.conf, for the cases of PS12: the roles hune_md5 and
#      hune_clear must log in with the methods `md5` and `password`.
#   2. It runs init.sql with psql. The passwords come from the environment of the container and go to
#      psql as variables. The file init.sql holds no password.
#
# Environment (all mandatory): PGS_RO_PASSWORD, PGS_RW_PASSWORD, PGS_BAD_PASSWORD, PGS_LEGACY_PASSWORD.
# Environment (optional): PGS_INIT_SQL (default /fixtures/init.sql), POSTGRES_USER (default postgres).
# The demo database does not use this script: see demo.sh.
set -eu

: "${PGS_RO_PASSWORD:?PGS_RO_PASSWORD is not set}"
: "${PGS_RW_PASSWORD:?PGS_RW_PASSWORD is not set}"
: "${PGS_BAD_PASSWORD:?PGS_BAD_PASSWORD is not set}"
: "${PGS_LEGACY_PASSWORD:?PGS_LEGACY_PASSWORD is not set}"
init_sql=${PGS_INIT_SQL:-/fixtures/init.sql}
admin=${POSTGRES_USER:-postgres}
[ -r "$init_sql" ] || { echo "apply.sh: $init_sql is not readable" >&2; exit 1; }

hba="${PGDATA:?PGDATA is not set}/pg_hba.conf"
[ -w "$hba" ] || { echo "apply.sh: $hba is not writable" >&2; exit 1; }
tmp="$hba.hune"
{
    echo "# Added by apply.sh for the cases PS12: an old method of login for two roles."
    echo "host all hune_md5 all md5"
    echo "host all hune_clear all password"
    cat "$hba"
} >"$tmp"
cat "$tmp" >"$hba"
rm -f "$tmp"

psql -v ON_ERROR_STOP=1 --username "$admin" --dbname postgres \
    -v ro_password="$PGS_RO_PASSWORD" -v rw_password="$PGS_RW_PASSWORD" \
    -v bad_password="$PGS_BAD_PASSWORD" -v legacy_password="$PGS_LEGACY_PASSWORD" \
    -f "$init_sql"
