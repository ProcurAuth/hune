#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Job `policy` (16.17, 16.18): the policy checks. The workflow ci.yaml and tests/ci/all.sh both call this script.
# Use: tests/ci/policy.sh [--local]
#      tests/ci/policy.sh --print-deps        print the dependency list of 17.9 (to update deps.lock.txt)
#   (default) runs the checks in the builder image of 17.2, so the runner needs only Docker.
#   --local   runs the checks on the host with no Docker. cargo-deny is optional here: when it is absent the
#             check prints "skipped: cargo-deny not installed".
# Each check prints "ok", "FAIL" with its findings, or "skipped". The script exits 1 when a check fails.
#
# Patterns that would match this script are split with '' (16.18): the shell joins the parts.
#
# Exemptions:
#   no file of the repository holds a word of the forbidden word list (check "No forbidden word",
#          over every file outside target/, .git/ and private/). The list stays outside the
#          repository: see words_checks below.
#   "Addresses in tests" accepts the wildcard 0.0.0.0 (tools/check-addresses.sh).
#   "No remote load" and "No inline script" skip web/js/vendor/ and web/css/vendor/ only for the lines that
#          the copied files trigger (they hold data: URLs with http://www.w3.org/...).
set -eu
. "$(dirname "$0")/ci-lib.sh"

# The list has each package with a source (the registry and the git dependencies) and each local copy under
# vendor/ (the patched crate mongodb has no source). A local copy has the tag " (vendor)", so the
# list shows that the crate is not the one of the registry. A workspace crate is not in the list.
deps_list() {
    for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
        if command -v jq >/dev/null 2>&1; then
            cargo metadata --locked --format-version 1 --filter-platform "$t" |
                jq -r '(.resolve.nodes | map(.id)) as $ids | (.workspace_root + "/vendor/") as $v
                       | .packages[] | select(.id as $i | $ids | index($i))
                       | select(.source != null or (.manifest_path | startswith($v)))
                       | "\(.name) \(.version)\(if .source == null then " (vendor)" else "" end)"'
        else
            # The same filter as the jq program above.
            cargo metadata --locked --format-version 1 --filter-platform "$t" |
                python3 -I -c '
import json, sys
d = json.load(sys.stdin)
ids = {n["id"] for n in d["resolve"]["nodes"]}
vendor = d["workspace_root"] + "/vendor/"
for p in d["packages"]:
    if p["id"] in ids and (p.get("source") is not None or p["manifest_path"].startswith(vendor)):
        print("%s %s%s" % (p["name"], p["version"], " (vendor)" if p.get("source") is None else ""))'
        fi
    # The order is the byte order (LC_ALL=C), so the list is the same on the host and in the builder container.
    done | LC_ALL=C sort -u
}

if [ "${1:-}" = "--print-deps" ]; then
    ci_init
    deps_list
    exit 0
fi

# Internal option of the Docker mode (below): the host runs the checks of the word list, so the list never
# goes into the builder container.
WORDS_ON_HOST=0
if [ "$#" -eq 2 ] && [ "$1" = "--local" ] && [ "$2" = "--words-on-host" ]; then
    WORDS_ON_HOST=1
    set -- --local
fi

ci_init "$@"

failed=0

# --- The forbidden word list: check 16 and the extra check ---
# The word list stays outside the repository. The script reads it from the environment
# variable HUNE_POLICY_WORDS (a secret of the CI; the words are separated by line ends or commas) or, out of
# the CI, from the untracked file private/policy-words.txt (one word on each line; .gitignore holds /private/).
# With no list, the two checks fail (fail-closed). A finding gives the file and the line number, never the word.
POLICY_WORDS_FILE=private/policy-words.txt

policy_words() {
    if [ -n "${HUNE_POLICY_WORDS:-}" ]; then
        printf '%s\n' "$HUNE_POLICY_WORDS" | tr ',' '\n'
    elif [ "${GITHUB_ACTIONS:-}" != "true" ] && [ -f "$POLICY_WORDS_FILE" ]; then
        cat "$POLICY_WORDS_FILE"
    fi | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -v '^$' || true
}

# words_report NAME FINDINGS : the same output as run_check.
words_report() {
    if [ -z "$2" ]; then
        echo "ok      $1"
    else
        echo "FAIL    $1"
        printf '%s\n' "$2" | head -n 40 | sed 's/^/          /'
        failed=$((failed + 1))
    fi
}

# words_checks : the two checks of the word list. Each failed check adds 1 to $failed.
words_checks() {
    wc_words=$(policy_words)
    if [ -z "$wc_words" ]; then
        if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
            wc_why="no word list: set the secret HUNE_POLICY_WORDS of the repository (the job policy gives it to this script)"
        else
            wc_why="no word list: write the words in $POLICY_WORDS_FILE (one word on each line) or set HUNE_POLICY_WORDS"
        fi
        words_report "Synthetic fixtures" "$wc_why"
        words_report "No forbidden word in the repository" "$wc_why"
        return 0
    fi
    # 16 Synthetic fixtures: a whole word, in any case, under tests/ and deploy/.
    wc_out=$(printf '%s\n' "$wc_words" | grep -rniwF -f - tests deploy 2>&1 | cut -d: -f1,2 || true)
    words_report "Synthetic fixtures" "$wc_out"
    # Extra: any text, in any case, in each file outside target/,.git/ and private/.
    wc_out=$(printf '%s\n' "$wc_words" |
        grep -rniIF -f - --exclude-dir=target --exclude-dir=.git --exclude-dir=private . 2>&1 | cut -d: -f1,2 || true)
    words_report "No forbidden word in the repository" "$wc_out"
}

if [ "$LOCAL" -eq 0 ]; then
    # Run this script again inside the builder image, in --local mode. The host runs the checks of the word list.
    words_checks
    rc=0
    in_builder tests/ci/policy.sh --local --words-on-host || rc=$?
    if [ "$failed" -ne 0 ]; then
        echo "policy: $failed check(s) of the word list failed"
        rc=1
    fi
    exit "$rc"
fi

mkdir -p target

# run_check NAME COMMAND : COMMAND prints the findings. No output means the check passes.
run_check() {
    rc_name=$1
    rc_cmd=$2
    rc_out=$(sh -c "$rc_cmd" 2>&1 || true)
    if [ -z "$rc_out" ]; then
        echo "ok      $rc_name"
    else
        echo "FAIL    $rc_name"
        printf '%s\n' "$rc_out" | head -n 40 | sed 's/^/          /'
        failed=$((failed + 1))
    fi
}

# --- 1 No image push, and no registry login in the scripts (16.18) ---
# The patterns are split with '' so that they do not match this script. The check scans the workflows and
# the other files that could push an image.
run_check "No image push" \
    "grep -rnE --exclude=policy.sh 'ghcr''\\.io|docker/login''-action|packages: *wr''ite|push: *tr''ue' .github/workflows tests/ci tests/compose tests/conformance build tools deploy"
# Extra check: no script pushes an image or logs in either.
run_check "No docker push or login in scripts (extra)" \
    "grep -rnE --exclude=policy.sh 'docker[[:space:]]+(pus''h|logi''n)|buildx[^#]*--pus''h|imagetools[[:space:]]+crea''te' .github tests/ci tests/conformance build tools deploy"

# --- 2 No environment read outside the engine (3.4, R2) ---
# The pattern holds the short form `env::var` too (after `use std::env;`).
run_check "No environment read outside the engine (R2)" \
    "grep -rnE --include='*.rs' '(^|[^[:alnum:]_])env::(var|var_os|vars|vars_os|set_var|remove_var)\\b' crates | grep -vE '^crates/hclconf/src/env\\.rs:|/build\\.rs:|/tests/'"

# --- 2a No environment read in the MongoDB driver copy (13.2 rule 5, 8b.15) ---
# The copy vendor/mongodb reads no environment variable and no file that tells the platform (/.dockerenv).
# The list tests/ci/env-read-allow.txt of the reads that this check accepts is EMPTY: a line that is not a
# comment is a fixed text, and a finding that holds it is accepted. The test code of the crate is not compiled
# in Hune (the files test.rs, and the directories test and tests), so the check skips it. The compile-time
# `env!` (the version of the package) is not a read at run time. The check also scans the code of Hune in
# crates/ (tests and build.rs excepted, as R2 says) for the names of the platform variables.
grep -vE '^[[:space:]]*(#|$)' tests/ci/env-read-allow.txt >target/ci-policy-env-allow.txt 2>/dev/null || : >target/ci-policy-env-allow.txt
run_check "No environment read in the MongoDB driver copy" '
{
  find vendor/mongodb -name "*.rs" -not -name test.rs -not -name tests.rs -not -path "*/test/*" -not -path "*/tests/*" |
    sort | xargs grep -nE "(^|[^[:alnum:]_])env::(var|var_os|vars|vars_os|set_var|remove_var|args|args_os)\\b|option_env!|getenv|/\\.dockerenv|resolv\\.conf" || true
  grep -rnE --include="*.rs" "AWS_EXECUTION_ENV|AWS_REGION|AWS_LAMBDA|K_SERVICE|FUNCTION_NAME|VERCEL|FUNCTIONS_WORKER_RUNTIME|KUBERNETES_SERVICE_HOST|/\\.dockerenv" crates | grep -vE "/tests/|/tests\\.rs:|/build\\.rs:" || true
} | grep -vE "^[^:]+:[0-9]+:[[:space:]]*//" | grep -vFf target/ci-policy-env-allow.txt || true
'

# --- 3 No read of the configuration outside the module config (13.2, rule 4) ---
run_check "No hclconf use outside the module config" \
    "grep -rnE --include='*.rs' 'hclconf::' crates | grep -vE '^crates/hclconf/|^crates/hune/src/config/'"

# --- 4 No process spawn ---
run_check "No process spawn (R3)" \
    "grep -rnE --include='*.rs' 'process::Command' crates | grep -v '/tests/'"

# --- 5 No file write outside the volume backend and the CLI output module ---
run_check "No file write outside the volume backend (R4)" \
    "grep -rnE --include='*.rs' 'fs::(write|create_dir|create_dir_all|remove_file|remove_dir|remove_dir_all|rename|hard_link|set_permissions|copy)|File::create|OpenOptions|openat|renameat|unlinkat|mkdirat' crates | grep -vE '^crates/hune-store/src/volume/|^crates/hune/src/cli/output\\.rs:|/build\\.rs:|/tests/'"

# --- 5a No simple protocol for the SQL of a caller (8a.5, 16.18) ---
# The names of the simple protocol (and of COPY) appear only in the store and in fixed.rs of the PostgreSQL source.
run_check "No simple protocol for the SQL of a caller (8a.5)" \
    "grep -rnE --include='*.rs' 'simple_query|batch_execute|copy_in|copy_out' crates | grep -vE '^crates/hune-store/|^crates/hune-source/src/postgres/fixed\\.rs:|/tests/'"

# --- 5b Simple protocol in fixed.rs only with a constant (8a.5, 16.18) ---
# Each call of simple_query or batch_execute in fixed.rs takes a constant as its one argument: an upper-case name
# (also a path such as Self::BEGIN), with or without `&`. A variable, a call, a format! or a text built at run
# time does not match. A line that holds a call but does not end it (a split call) does not match either.
# The names copy_in and copy_out are forbidden in the whole directory, fixed.rs too. A comment line is skipped.
run_check "Simple protocol only with a constant in fixed.rs" '
f=crates/hune-source/src/postgres/fixed.rs
[ -f "$f" ] || exit 0
grep -nE "(simple_query|batch_execute)[[:space:]]*(\(|::<)" "$f" |
    grep -vE "^[0-9]+:[[:space:]]*//" |
    grep -vE "\.(simple_query|batch_execute)\(&?([A-Za-z_]+::)*[A-Z][A-Z0-9_]*\)"
grep -rnE --include="*.rs" "copy_in|copy_out" crates/hune-source/src/postgres |
    grep -vE "^[^:]*:[0-9]+:[[:space:]]*//"
'

# --- 5c The four checks of the MongoDB source (8b.2, 8b.4, 8b.5, 16.18) ---
# Differences from the text of 16.18, and why:
#   - `run_command` is the name of a function of the CLI (crates/hune/src/cli/mod.rs), so the pattern for it is the
#     method call `.run_command(`. The names run_cursor_command and run_raw_command stay as the text has them.
#   - The exemption `/tests/` also holds the files named tests.rs (the unit tests of a module).
#   - The crate name `mongodb::` or `bson::` counts only at the start of a path: `config::mongodb::block` is the
#     module of Hune with this name, and it is not the crate.
# No command document from a caller (8b.5): only the module driver.rs can send a document of its own.
run_check "No command document from a caller (8b.5, MongoDB)" \
    "grep -rnE --include='*.rs' '\\.run_command\\(|run_cursor_command|run_raw_command' crates | grep -vE '^crates/hune-source/src/mongo/driver\\.rs:|/tests/|/tests\\.rs:'"
# No MongoDB crate outside the driver module (8b.2): in the code, and in the manifests.
run_check "No MongoDB crate outside the driver module (8b.2)" \
    "grep -rnE --include='*.rs' '(^|[^[:alnum:]_:])(mongodb|bson)::' crates | grep -vE '^crates/hune-source/src/mongo/driver\\.rs:|/tests/|/tests\\.rs:'; grep -lE '^(mongodb|bson) *=' crates/*/Cargo.toml | grep -v '^crates/hune-source/Cargo\\.toml\$'"
# No write call of the driver (8b.5).
run_check "No write call of the MongoDB driver (8b.5)" \
    "grep -rnE --include='*.rs' '\\.(insert_one|insert_many|update_one|update_many|replace_one|delete_one|delete_many|drop|create_collection|create_index|create_indexes|drop_index|find_one_and_[a-z_]+)\\(' crates/hune-source/src/mongo | grep -vE '/tests/|/tests\\.rs:'"
# No SRV lookup (8b.4).
run_check "No SRV lookup (8b.4)" \
    "grep -rnE 'mongodb\\+srv|dns-resolver|srv_service_name|hickory' Cargo.toml crates/*/Cargo.toml deps.lock.txt"
# The closed command list (8b.5): COMMAND_NAMES holds the names of the table, in its order, and no other name.
run_check "Closed command list (8b.5)" '
f=crates/hune-core/src/mongo/command.rs
[ -f "$f" ] || exit 0
got=$(awk "/COMMAND_NAMES: \\[&str; [0-9]+\\] = \\[/{on=1;next} on&&/^\\];/{exit} on{gsub(/[ \",]/,\"\");print}" "$f" | tr "\\n" " ")
want="hello saslStart saslContinue buildInfo connectionStatus listCollections aggregate find getMore killCursors endSessions "
[ "$got" = "$want" ] || echo "$f: COMMAND_NAMES is [$got], the table of 8b.5 gives [$want]"
'
# The refused names (7.32) appear in the code of the three files that hold the refused lists (the grammar, the
# wire guard and the refused command names), and nowhere else. The check skips a comment line and the unit tests of
# a file (the lines after its first #[cfg(test)]). `$code` is not in the list: the reader of server data names it.
cat >target/ci-policy-refused.awk <<'AWK'
FNR == 1 { intest = 0 }
/#\[cfg\(test\)\]/ { intest = 1 }
intest { next }
/^[ \t]*\/\// { next }
/\$(out|merge|function|accumulator|where)([^A-Za-z0-9_]|$)|mapReduce/ { print FILENAME ":" FNR ": " $0 }
AWK
run_check "Refused MongoDB names only in the refused lists (7.32)" \
    "find crates -name '*.rs' -path '*/src/*' -not -path '*/tests/*' -not -name 'tests.rs' | sort | xargs awk -f target/ci-policy-refused.awk | grep -vE '^crates/hune-core/src/mongo/(grammar|command)\\.rs:|^crates/hune-source/src/mongo/guard\\.rs:'"
# No connection string in the code (8b.4): the source has the keys of 13.12b, never a URI. Tests are exempt.
run_check "No MongoDB connection string in the code (8b.4)" \
    "grep -rnE --include='*.rs' 'mongodb(\\+srv)?://' crates | grep -vE '/tests/|/tests\\.rs:'; grep -rnE 'mongodb(\\+srv)?://' deploy"

# --- 6 No unsafe code ---
run_check "No unsafe code (R5)" \
    "grep -L 'forbid(unsafe_code)' crates/*/src/lib.rs crates/hune/src/main.rs"

# --- 7 No remote load in the front end ---
# The pattern is in a file so that the quotes stay simple. Files under vendor/ are copies: the check skips
# only the lines that match inside them (data: URLs of SVG with http://www.w3.org/...).
cat >target/ci-policy-remote.pat <<'PAT'
(src|href)=["']?https?:|import[^;]*["']https?:|url\(["']?https?:
PAT
run_check "No remote load in the front end" \
    "grep -rnEf target/ci-policy-remote.pat web | grep -vE '^web/(js|css)/vendor/[^:]*:[0-9]+:.*data:'"

# --- 8 No inline script ---
run_check "No inline script" \
    "grep -rnP '<script(?![^>]*\\bsrc=)(?![^>]*type=\"application/json\")' web | grep -vE '^web/(js|css)/vendor/'"

# --- 9 npm only in test images ---
# The directory vendor/ holds the local copy of the crate mongodb: the copy has no package.json.
run_check "npm only in test images" \
    "find . -name package.json -not -path './tests/browser/*' -not -path './tools/parity/*' -not -path './target/*' -not -path './vendor/*' -not -path './.git/*'"

# --- 10 Copied chart engine, 11 Lexer vectors ---
run_check "Copied chart engine" "tools/check-vendor-lock.sh --chdash"
run_check "Lexer vectors" "tools/check-vendor-lock.sh --lexer"
# --- 10a Local copy of the MongoDB driver ---
run_check "Local copy of the MongoDB driver (lock and patch)" "tools/check-vendor-lock.sh --mongodb"

# --- 12 Engine hclconf ---
run_check "Engine hclconf (PROVENANCE)" '
if [ ! -f crates/hclconf/PROVENANCE ]; then echo "crates/hclconf/PROVENANCE does not exist"; exit 0; fi
grep -q "not a copy" crates/hclconf/PROVENANCE || echo "PROVENANCE does not say that the engine is not a copy"
# The engine is written in Hune, not
# copied. Then PROVENANCE says so ("not a copy") in place of a source commit.
grep -qE "\b[0-9a-f]{7,40}\b|not a copy" crates/hclconf/PROVENANCE || echo "PROVENANCE does not name a commit"
grep -q "2021" crates/hclconf/PROVENANCE || echo "PROVENANCE does not name the edition 2021"
grep -qE "^edition *= *\"2021\"" crates/hclconf/Cargo.toml || echo "crates/hclconf/Cargo.toml does not hold edition = \"2021\""
'

# --- 13 Configuration reference (K13) ---
run_check "Configuration reference" '
if [ ! -f docs/configuration-reference.md ]; then echo "docs/configuration-reference.md does not exist"; exit 0; fi
if ! cargo build --locked -q -p hune 2>target/ci-policy-build.log; then
  echo "cargo build of hune failed:"; tail -n 5 target/ci-policy-build.log; exit 0
fi
target/debug/hune print-reference --format markdown | diff -u docs/configuration-reference.md -
'

# --- 14 cargo deny ---
if command -v cargo-deny >/dev/null 2>&1; then
    run_check "Licences, advisories, bans, sources (cargo deny)" \
        "cargo deny --locked check advisories bans licenses sources >target/ci-deny.log 2>&1 || tail -n 40 target/ci-deny.log"
else
    echo "skipped Licences, advisories, bans, sources: cargo-deny not installed"
fi

# --- 15 Dependency list ---
if command -v cargo >/dev/null 2>&1; then
    if deps_list >target/ci-deps.now 2>target/ci-deps.err; then
        run_check "Dependency list (deps.lock.txt)" "diff -u deps.lock.txt target/ci-deps.now"
    else
        echo "FAIL    Dependency list (deps.lock.txt)"
        sed 's/^/          /' target/ci-deps.err | head -n 10
        failed=$((failed + 1))
    fi
else
    echo "skipped Dependency list: cargo not installed"
fi

# --- 16 Synthetic fixtures, and Extra: the word list (words_checks) ---
# In the Docker mode, the host ran these two checks before the builder container.
if [ "$WORDS_ON_HOST" -eq 0 ]; then
    words_checks
fi

# --- 17 Addresses in tests ---
run_check "Addresses in tests" "tools/check-addresses.sh"

# --- 18 Documentation language ---
run_check "Documentation language" \
    "python3 -I tools/ste_lint.py docs/*.md README.md CHANGELOG.md >target/ci-ste.log 2>&1 || tail -n 40 target/ci-ste.log"

# --- Extra: LICENSE-APACHE is the standard Apache-2.0 text, and LICENSE-MIT is the MIT text ---
# LICENSE-APACHE differs from the standard text on line 190 only (the copyright notice of the project).
run_check "LICENSE-APACHE is the standard Apache-2.0 text, LICENSE-MIT is the MIT text (extra)" '
got=$(sed "190s/.*/   Copyright [yyyy] [name of copyright owner]/" LICENSE-APACHE | sha256sum | cut -d" " -f1)
[ "$got" = cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30 ] || echo "LICENSE-APACHE has sha256 $got after the normal notice line"
head -n 1 LICENSE-MIT | grep -qx "MIT License" || echo "LICENSE-MIT does not start with MIT License"
'

echo
if [ "$failed" -eq 0 ]; then
    echo "policy: all checks passed"
    exit 0
fi
echo "policy: $failed check(s) failed"
exit 1
