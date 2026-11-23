#!/bin/sh
# Apply init.js on a disposable MongoDB server with no login (16.9b).
#
# Use: apply.sh [--host HOST] [--port PORT] [--file PATH]
#   The script runs init.js with mongosh against a server with authorization off. It does not use TLS: the
#   server of the first start listens on 127.0.0.1 only. The default host is 127.0.0.1, the default port is
#   27017, and the default file is init.js in the directory of this script.
# Environment (mandatory): MGS_ADMIN_PASSWORD, MGS_RO_PASSWORD, MGS_RW_PASSWORD, MGS_BAD_PASSWORD,
#   MGS_LEGACY_PASSWORD. mongosh reads them as process variables, so init.js and the command line hold no
#   password.
# Environment (optional): MGS_NODE_LOAD_DOCS, MGS_BIG_BYTES (see init.js).
# The compose project tests/compose/mongosource.yaml calls this script through start.sh at the first start.
# The demo database does not use this script: see demo.sh.
set -eu

DIR=$(cd "$(dirname "$0")" && pwd)
host=127.0.0.1
port=27017
file="$DIR/init.js"
while [ $# -gt 0 ]; do
    case $1 in
    --host) host=${2:?--host needs a value}; shift 2 ;;
    --port) port=${2:?--port needs a value}; shift 2 ;;
    --file) file=${2:?--file needs a path}; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) echo "apply.sh: unknown option: $1" >&2; exit 1 ;;
    esac
done
: "${MGS_ADMIN_PASSWORD:?MGS_ADMIN_PASSWORD is not set}"
: "${MGS_RO_PASSWORD:?MGS_RO_PASSWORD is not set}"
: "${MGS_RW_PASSWORD:?MGS_RW_PASSWORD is not set}"
: "${MGS_BAD_PASSWORD:?MGS_BAD_PASSWORD is not set}"
: "${MGS_LEGACY_PASSWORD:?MGS_LEGACY_PASSWORD is not set}"
[ -r "$file" ] || { echo "apply.sh: $file is not readable" >&2; exit 1; }

exec mongosh --quiet --norc --host "$host" --port "$port" --file "$file"
