#!/bin/sh
# Make the synthetic demo database of the PostgreSQL source in a disposable PostgreSQL server.
#
# Use: demo.sh --container NAME [--password-file PATH] [--days N]
#      demo.sh --psql "COMMAND" [--password-file PATH] [--days N]
#   --container NAME   run psql in the container NAME (`docker exec`), as the user `postgres` of the image.
#                      The container must be a PostgreSQL 15 to 18 server that has no database `analytics`.
#   --psql "COMMAND"   run this command as the psql client with superuser rights, for example
#                      "psql -h 127.0.0.1 -p 55432 -U postgres". The script adds the options of its own.
#                      The command must reach a disposable server on 127.0.0.1. The script never starts one.
#   --password-file P  the file of the password of the role `hune_ro`. When it does not exist, the script
#                      makes a random password and writes it there with the mode 600. The password is never
#                      printed. The default is ./hune-demo-ro.password
#   --days N           the length of the data in days (1 to 30, default 2).
# The file demo.sql holds the SQL. It makes the database `analytics` with the schema `metrics` and the
# role `hune_ro` of 8a.13. Run the script one time for each server.
# Variable: DOCKER (the docker command, default `docker`).
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
DOCKER=${DOCKER:-docker}
container=""
psql_cmd=""
pwfile="./hune-demo-ro.password"
days=2
while [ $# -gt 0 ]; do
    case $1 in
    --container) container=${2:?--container needs a name}; shift 2 ;;
    --psql) psql_cmd=${2:?--psql needs a command}; shift 2 ;;
    --password-file) pwfile=${2:?--password-file needs a path}; shift 2 ;;
    --days) days=${2:?--days needs a number}; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) echo "error: unknown option: $1" >&2; exit 1 ;;
    esac
done
if [ -n "$container" ] && [ -n "$psql_cmd" ]; then
    echo "error: give --container or --psql, not both" >&2
    exit 1
fi
if [ -z "$container" ] && [ -z "$psql_cmd" ]; then
    echo "error: give --container or --psql" >&2
    exit 1
fi
case $days in
'' | *[!0-9]*) echo "error: --days must be a whole number: $days" >&2; exit 1 ;;
esac
if [ "$days" -lt 1 ] || [ "$days" -gt 30 ]; then
    echo "error: --days must be from 1 to 30: $days" >&2
    exit 1
fi

case $pwfile in
/*) ;;
*) pwfile="$PWD/$pwfile" ;;
esac
if [ ! -e "$pwfile" ]; then
    (umask 077 && openssl rand -hex 16 >"$pwfile")
fi
[ -s "$pwfile" ] || { echo "error: $pwfile is empty" >&2; exit 1; }
password=$(cat "$pwfile")

if [ -n "$container" ]; then
    "$DOCKER" exec -i "$container" psql -U postgres -d postgres -v ON_ERROR_STOP=1 -q \
        -v "ro_password=$password" -v "days=$days" -f - <"$DIR/demo.sql"
    port=$("$DOCKER" port "$container" 5432/tcp 2>/dev/null | head -n 1 | sed 's/.*://' || true)
else
    # The command is a text with words: the shell splits it here on purpose.
    # shellcheck disable=SC2086
    $psql_cmd -d postgres -v ON_ERROR_STOP=1 -q -v "ro_password=$password" -v "days=$days" -f "$DIR/demo.sql"
    port=""
fi

echo "demo database ready: database analytics, schema metrics, role hune_ro, $days day(s) of data"
echo "password file of hune_ro: $pwfile"
[ -z "$port" ] || echo "host 127.0.0.1, port $port"
echo "block of the Hune configuration:"
cat <<HCL
  postgres "demo" {
    label              = "Demo"
    host               = "127.0.0.1"
    port               = ${port:-5432}
    database           = "analytics"
    user               = "hune_ro"
    password_file      = "$pwfile"
    search_path        = "metrics"
    sslmode            = "disable"
    connect_timeout    = "5s"
    max_execution_time = "1m"
    max_result_rows    = 100000
    max_result_bytes   = 33554432
    max_query_size     = 1048576
    max_concurrent     = 16
    queue_wait         = "5s"
  }
HCL
