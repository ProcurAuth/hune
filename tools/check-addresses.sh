#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Check the IPv4 literals under tests/ and deploy/ (16.18, "Addresses in tests").
# Use: tools/check-addresses.sh [DIR ...]      (default: tests deploy)
# A literal passes when it is in 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 or one of the
# documentation ranges 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24.
# The wildcard address 0.0.0.0 passes too (a listen address is not a host address).
# The file tests/images.lock is skipped: it holds image tags, and a tag such as 26.8.34.13 has four numbers.
# A line that names a tag, a version or an image (the words tag, version, image, clickhouse-server) is read as
# a version text: its four-number literals are not addresses. (Choice of the delivery worker.)
# Exit code: 0 when every literal passes, 1 when one does not, 2 for a bad call.
set -eu
cd "$(dirname "$0")/.."
[ "$#" -gt 0 ] || set -- tests deploy
for d in "$@"; do
    [ -e "$d" ] || { echo "error: $d does not exist" >&2; exit 2; }
done

grep -rIn --exclude=images.lock -E '([0-9]{1,3}\.){3}[0-9]{1,3}' "$@" 2>/dev/null |
    awk '
    {
        i1 = index($0, ":"); rest = substr($0, i1 + 1); i2 = index(rest, ":")
        file = substr($0, 1, i1 - 1); line = substr(rest, 1, i2 - 1); text = substr(rest, i2 + 1)
        version_line = (tolower(text) ~ /(tag|version|image|clickhouse-server)/)
        while (match(text, /([0-9]{1,3}\.){3}[0-9]{1,3}/)) {
            ip = substr(text, RSTART, RLENGTH)
            before = (RSTART > 1) ? substr(text, RSTART - 1, 1) : " "
            after = substr(text, RSTART + RLENGTH, 1)
            text = substr(text, RSTART + RLENGTH)
            # Part of a longer dotted token (a version of five numbers, a word before): not an address.
            if (before ~ /[0-9A-Za-z._-]/ || after ~ /[0-9A-Za-z_]/) continue
            if (after == "." && substr(text, 2, 1) ~ /[0-9]/) continue
            split(ip, o, ".")
            ok = 1
            for (i = 1; i <= 4; i++) if (o[i] + 0 > 255) ok = 0
            if (!ok) continue                        # not an address
            if (version_line) continue               # a version text
            a = o[1] + 0; b = o[2] + 0; c = o[3] + 0
            pass = 0
            if (a == 127) pass = 1
            else if (a == 10) pass = 1
            else if (a == 172 && b >= 16 && b <= 31) pass = 1
            else if (a == 192 && b == 168) pass = 1
            else if (a == 192 && b == 0 && c == 2) pass = 1
            else if (a == 198 && b == 51 && c == 100) pass = 1
            else if (a == 203 && b == 0 && c == 113) pass = 1
            else if (a == 0 && b == 0 && c == 0 && o[4] + 0 == 0) pass = 1
            if (!pass) { printf "%s:%s: address %s is not in a permitted range\n", file, line, ip; bad = 1 }
        }
    }
    END { exit bad }'
