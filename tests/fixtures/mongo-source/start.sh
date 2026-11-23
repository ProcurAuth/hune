#!/bin/sh
# Entrypoint of the containers of tests/compose/mongosource.yaml (16.9b). It runs as root in the container.
#
# Steps:
#   1. Start a temporary mongod on 127.0.0.1 with authorization off (a replica set when MGS_REPLSET is set:
#      two members when MGS_PORT2 is set too, in this container, on the ports MGS_PORT and MGS_PORT2), and wait
#      until it accepts writes and, for the second member, until it holds the data.
#   2. Run apply.sh (init.js).
#   3. Stop the temporary server.
#   4. Replace this process with the final mongod: authorization on, TLS only (requireTLS, TLS 1.2 and later),
#      and --noscripting when MGS_NOSCRIPTING is 1. The second member starts first, in the background.
# The server listens on the same port inside the container and on the host (MGS_PORT), so a replica set can
# announce `localhost:MGS_PORT` and a client on the host can reach it.
# Environment (mandatory): MGS_PORT, the five passwords of apply.sh.
# Environment (optional): MGS_NOSCRIPTING (1 or 0, default 1), MGS_REPLSET (a set name, default none), MGS_PORT2
#   (the port of a second member of the set, default none),
#   MGS_MECHANISMS (the value of authenticationMechanisms, default SCRAM-SHA-256,SCRAM-SHA-1),
#   MGS_CACHE_GB (the WiredTiger cache, default 0.5), MGS_NODE_LOAD_DOCS, MGS_BIG_BYTES.
# Files: /tls/server.pem (certificate and key), /tls/ca.pem and, for a replica set, /tls/keyfile (read only mounts).
set -eu

: "${MGS_PORT:?MGS_PORT is not set}"
noscripting=${MGS_NOSCRIPTING:-1}
replset=${MGS_REPLSET:-}
port2=${MGS_PORT2:-}
dbpath2=/var/lib/member2
mechanisms=${MGS_MECHANISMS:-SCRAM-SHA-256,SCRAM-SHA-1}
cache=${MGS_CACHE_GB:-0.5}
dbpath=/data/db
mkdir -p "$dbpath" /var/tls
cp /tls/server.pem /tls/ca.pem /var/tls/
chmod 600 /var/tls/server.pem
if [ -n "$replset" ]; then
    cp /tls/keyfile /var/tls/keyfile
    chmod 600 /var/tls/keyfile
fi

tmp_args="--dbpath $dbpath --bind_ip 127.0.0.1 --port $MGS_PORT --wiredTigerCacheSizeGB $cache"
[ -z "$replset" ] || tmp_args="$tmp_args --replSet $replset"
# shellcheck disable=SC2086
mongod $tmp_args --fork --logpath /tmp/mongod-init.log >/dev/null ||
    { cat /tmp/mongod-init.log >&2; exit 1; }
if [ -n "$replset" ] && [ -n "$port2" ]; then
    mkdir -p "$dbpath2"
    mongod --dbpath "$dbpath2" --bind_ip 127.0.0.1 --port "$port2" --wiredTigerCacheSizeGB "$cache" \
        --replSet "$replset" --fork --logpath /tmp/mongod-init2.log >/dev/null ||
        { cat /tmp/mongod-init2.log >&2; exit 1; }
fi

if [ -n "$replset" ]; then
    members="{ _id: 0, host: 'localhost:$MGS_PORT', priority: 2 }"
    [ -z "$port2" ] || members="$members, { _id: 1, host: 'localhost:$port2' }"
    mongosh --quiet --norc --host 127.0.0.1 --port "$MGS_PORT" --eval \
        "rs.initiate({ _id: '$replset', members: [$members] })" >/dev/null
fi
i=0
until mongosh --quiet --norc --host 127.0.0.1 --port "$MGS_PORT" --eval \
    'if (!db.hello().isWritablePrimary) { quit(1) }' >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -le 60 ] || { echo "start.sh: the temporary server is not writable" >&2; exit 1; }
    sleep 1
done

/fixtures/apply.sh --host 127.0.0.1 --port "$MGS_PORT" --file /fixtures/init.js
if [ -n "$port2" ]; then
    # The second member must hold the data before the temporary servers stop.
    i=0
    until mongosh --quiet --norc --host 127.0.0.1 --port "$port2" --eval \
        'db.getMongo().setReadPref("secondaryPreferred"); if (db.getSiblingDB("analytics").fixture_marker.countDocuments({}) !== 1) { quit(1) }' \
        >/dev/null 2>&1; do
        i=$((i + 1))
        [ "$i" -le 120 ] || { echo "start.sh: the second member did not get the data" >&2; exit 1; }
        sleep 1
    done
    mongod --dbpath "$dbpath2" --shutdown >/dev/null
fi
mongod --dbpath "$dbpath" --shutdown >/dev/null

final="--dbpath $dbpath --bind_ip_all --port $MGS_PORT --wiredTigerCacheSizeGB $cache --auth"
final="$final --tlsMode requireTLS --tlsCertificateKeyFile /var/tls/server.pem --tlsCAFile /var/tls/ca.pem"
final="$final --tlsAllowConnectionsWithoutCertificates --tlsDisabledProtocols TLS1_0,TLS1_1"
final="$final --setParameter authenticationMechanisms=$mechanisms"
[ "$noscripting" != 1 ] || final="$final --noscripting"
[ -z "$replset" ] || final="$final --replSet $replset --keyFile /var/tls/keyfile"
if [ -n "$port2" ]; then
    final2="--dbpath $dbpath2 --bind_ip_all --port $port2 --wiredTigerCacheSizeGB $cache --auth"
    final2="$final2 --tlsMode requireTLS --tlsCertificateKeyFile /var/tls/server.pem --tlsCAFile /var/tls/ca.pem"
    final2="$final2 --tlsAllowConnectionsWithoutCertificates --tlsDisabledProtocols TLS1_0,TLS1_1"
    final2="$final2 --setParameter authenticationMechanisms=$mechanisms --replSet $replset --keyFile /var/tls/keyfile"
    [ "$noscripting" != 1 ] || final2="$final2 --noscripting"
    # shellcheck disable=SC2086
    mongod $final2 --fork --logpath /tmp/mongod-member2.log >/dev/null ||
        { cat /tmp/mongod-member2.log >&2; exit 1; }
fi
# shellcheck disable=SC2086
exec mongod $final
