#!/bin/sh
# Start a disposable demo MongoDB server with synthetic data, and print the block of the Hune configuration (16.9b).
#
# Use: demo.sh FOLDER [--days N] [--image IMAGE] [--container NAME] [--port PORT]
#   FOLDER             a folder for the passwords (it is made when absent, with the mode 700). The script writes
#                      two files there with the mode 600: root.password (the administrator of the disposable server)
#                      and hune_ro.password (the user of the block). It never prints a password.
#   --days N           the length of the data in days (1 to 30, default 2).
#   --image IMAGE      the image of the server (default mongo:8.0).
#   --container NAME   the name of the container (default hune-demo-mongo). The script stops when a container of
#                      this name exists: remove it first with `docker rm -f -v NAME`.
#   --port PORT        the port on 127.0.0.1 (default 27018).
# The script starts the container on 127.0.0.1 only, with authorization on and no TLS (the loopback interface), and
# runs demo.js with the login of the administrator. demo.js makes the database `analytics` with the collections
# `node_load` (a time series of metrics), `node_events` and `orders`, and the user `hune_ro` of 8b.13 (the role
# `read` on `analytics` and no other role). The block that the script prints uses `tls_mode = "disable"`, which is
# for a server on the loopback interface only.
# Old form: demo.sh --container NAME --password-file P [--admin-user U --admin-password-file P] runs demo.js in a
# server that already runs, and starts nothing.
# Variable: DOCKER (the docker command, default `docker`).
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
DOCKER=${DOCKER:-docker}

# --- The mode that starts a server: the first argument is a folder, not an option. ---
if [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; then
    folder=$1
    shift
    days=2
    image=mongo:8.0
    name=hune-demo-mongo
    hostport=27018
    while [ $# -gt 0 ]; do
        case $1 in
        --days) days=${2:?--days needs a number}; shift 2 ;;
        --image) image=${2:?--image needs a name}; shift 2 ;;
        --container) name=${2:?--container needs a name}; shift 2 ;;
        --port) hostport=${2:?--port needs a number}; shift 2 ;;
        *) echo "error: unknown option: $1" >&2; exit 1 ;;
        esac
    done
    case $days in
    '' | *[!0-9]*) echo "error: --days must be a whole number: $days" >&2; exit 1 ;;
    esac
    if [ "$days" -lt 1 ] || [ "$days" -gt 30 ]; then
        echo "error: --days must be from 1 to 30: $days" >&2
        exit 1
    fi
    if [ -n "$("$DOCKER" ps -aq --filter "name=^${name}\$")" ]; then
        echo "error: the container $name exists; remove it with: $DOCKER rm -f -v $name" >&2
        exit 1
    fi
    (umask 077 && mkdir -p "$folder")
    chmod 700 "$folder"
    folder=$(cd "$folder" && pwd)
    for f in root.password hune_ro.password; do
        [ -s "$folder/$f" ] || (umask 077 && openssl rand -hex 16 >"$folder/$f")
        chmod 600 "$folder/$f"
    done
    root_password=$(cat "$folder/root.password")
    "$DOCKER" run -d --name "$name" -p "127.0.0.1:${hostport}:27017" \
        -e MONGO_INITDB_ROOT_USERNAME=root -e "MONGO_INITDB_ROOT_PASSWORD=$root_password" \
        "$image" >/dev/null
    # The entrypoint starts a temporary server first. Wait until the final one accepts the login.
    i=0
    until "$DOCKER" exec -e "P=$root_password" "$name" sh -c \
        'mongosh --quiet --norc --host 127.0.0.1 --port 27017 -u root -p "$P" --authenticationDatabase admin --eval "db.runCommand({ping:1}).ok" 2>/dev/null' |
        grep -qx 1; do
        i=$((i + 1))
        if [ "$i" -gt 90 ]; then
            echo "error: the server did not start" >&2
            "$DOCKER" rm -f -v "$name" >/dev/null
            exit 1
        fi
        sleep 1
    done
    "$0" --container "$name" --password-file "$folder/hune_ro.password" --days "$days" \
        --admin-user root --admin-password-file "$folder/root.password"
    exit 0
fi
container=""
pwfile="./hune-demo-mongo-ro.password"
days=2
admin_user=""
admin_pwfile=""
while [ $# -gt 0 ]; do
    case $1 in
    --container) container=${2:?--container needs a name}; shift 2 ;;
    --password-file) pwfile=${2:?--password-file needs a path}; shift 2 ;;
    --days) days=${2:?--days needs a number}; shift 2 ;;
    --admin-user) admin_user=${2:?--admin-user needs a name}; shift 2 ;;
    --admin-password-file) admin_pwfile=${2:?--admin-password-file needs a path}; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) echo "error: unknown option: $1" >&2; exit 1 ;;
    esac
done
[ -n "$container" ] || { echo "error: give --container" >&2; exit 1; }
case $days in
'' | *[!0-9]*) echo "error: --days must be a whole number: $days" >&2; exit 1 ;;
esac
if [ "$days" -lt 1 ] || [ "$days" -gt 30 ]; then
    echo "error: --days must be from 1 to 30: $days" >&2
    exit 1
fi
if [ -n "$admin_user" ] && [ -z "$admin_pwfile" ]; then
    echo "error: --admin-user needs --admin-password-file" >&2
    exit 1
fi
if [ -z "$admin_user" ] && [ -n "$admin_pwfile" ]; then
    echo "error: --admin-password-file needs --admin-user" >&2
    exit 1
fi

case $pwfile in
/*) ;;
*) pwfile="$PWD/${pwfile#./}" ;;
esac
if [ ! -e "$pwfile" ]; then
    (umask 077 && openssl rand -hex 16 >"$pwfile")
fi
[ -s "$pwfile" ] || { echo "error: $pwfile is empty" >&2; exit 1; }
password=$(cat "$pwfile")

# The script text goes to mongosh on the standard input, so it is in no file of the container.
if [ -n "$admin_user" ]; then
    [ -s "$admin_pwfile" ] || { echo "error: $admin_pwfile is empty or absent" >&2; exit 1; }
    admin_password=$(cat "$admin_pwfile")
    "$DOCKER" exec -i -e "MGS_DEMO_RO_PASSWORD=$password" -e "MGS_DEMO_DAYS=$days" \
        -e "MGS_DEMO_ADMIN_USER=$admin_user" -e "MGS_DEMO_ADMIN_PASSWORD=$admin_password" \
        "$container" mongosh --quiet --norc --host 127.0.0.1 --port 27017 \
        --eval 'db.getSiblingDB("admin").auth(process.env.MGS_DEMO_ADMIN_USER, process.env.MGS_DEMO_ADMIN_PASSWORD)' \
        --file /dev/stdin <"$DIR/demo.js"
else
    "$DOCKER" exec -i -e "MGS_DEMO_RO_PASSWORD=$password" -e "MGS_DEMO_DAYS=$days" \
        "$container" mongosh --quiet --norc --host 127.0.0.1 --port 27017 --file /dev/stdin <"$DIR/demo.js"
fi
port=$("$DOCKER" port "$container" 27017/tcp 2>/dev/null | head -n 1 | sed 's/.*://' || true)

echo "demo database ready: database analytics, user hune_ro, $days day(s) of data"
echo "password file of hune_ro: $pwfile"
[ -z "$port" ] || echo "host 127.0.0.1, port $port"
echo "block of the Hune configuration:"
cat <<HCL
  mongodb "mgdemo" {
    label                = "MongoDB demo"
    topology             = "direct"
    host                 = "127.0.0.1"
    port                 = ${port:-27017}
    user                 = "hune_ro"
    password_file        = "$pwfile"
    auth_database        = "admin"
    databases            = ["analytics"]
    collections          = ["node_load", "node_events", "orders"]
    read_preference      = "primary"
    tls_mode             = "disable"
    connect_timeout      = "5s"
    max_execution_time   = "1m"
    max_result_rows      = 100000
    max_result_bytes     = 33554432
    max_query_size       = 1048576
    max_concurrent       = 16
    queue_wait           = "5s"
  }
HCL
