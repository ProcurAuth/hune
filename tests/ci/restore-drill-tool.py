# SPDX-License-Identifier: MIT OR Apache-2.0
"""Helper of the restore drill (16.14). The script tests/ci/restore-drill.sh calls it.

Run it with `python3 -I`. It uses the standard library only. It has four commands:

  config   Write a complete configuration file for a store (volume or PostgreSQL).
  seed     Start `hune serve` on a volume. Build the synthetic history through the API. Stop it.
  list     Start `hune serve` on a store. Print the list of dashboards in a stable form. Stop it.
  check    Check the files of an export: the `external` version without bytes, or the head kind.

The seed makes 21 uids: 20 current dashboards in 3 folders and one deleted dashboard. The history
holds saves, one move, one delete, one restore and one version of kind `external` without bytes.
Only the invalid file of the hook (5.14) is written by hand. All other writes use the API.
"""

import argparse
import hashlib
import http.client
import json
import os
import re
import signal
import socket
import subprocess
import sys
import time
import uuid

FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "fixtures")
CONFIG_TEMPLATE = os.path.join(FIXTURES, "config", "local.hcl")
CH_CA = os.path.join(FIXTURES, "config", "ca-a.pem")
DASHBOARDS = os.path.join(FIXTURES, "dashboards")

# The bytes that the hook writes over the current file of the dashboard (5.14, step 4).
# They are not valid JSON, so the etag of the file is the SHA-256 of these raw bytes.
HOOK_BYTES = b'{"hook": "an invalid file written outside Hune"\n'

FOLDERS = [("transit", "Transit desk"), ("ops", "Operations"), ("research", "Research and études")]
COUNT = 20
HOOK_UID = "d-02"
MOVED_UID = "d-05"
RESTORED_UID = "d-19"
DELETED_UID = "d-21"


def free_port():
    """Give a TCP port on 127.0.0.1 that is free now."""
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def die(message):
    print("error: " + message, file=sys.stderr)
    sys.exit(1)


# ---- Configuration files ----


def command_config(args):
    """Write a configuration file from tests/fixtures/config/local.hcl with another block `storage`."""
    directory = os.path.dirname(os.path.abspath(args.out))
    with open(CONFIG_TEMPLATE, encoding="utf-8") as f:
        text = f.read()
    head, rest = text.split("storage {", 1)
    _, tail = rest.split("\naccess {", 1)
    if args.backend == "volume":
        storage = (
            'storage {\n  volume "main" {\n    path            = "%s"\n'
            "    primary         = true\n    rescan_interval = \"1s\"\n    history_keep    = 100\n  }\n}\n\n"
            % args.path
        )
    else:
        storage = (
            'storage {\n  postgres {\n    host              = "%s"\n'
            "    port              = %d\n    database          = \"%s\"\n    user              = \"%s\"\n"
            '    password_file     = "%s"\n    sslmode           = "disable"\n    max_connections   = 8\n'
            '    connect_timeout   = "5s"\n    statement_timeout = "30s"\n    poll_interval     = "1s"\n'
            "    primary           = true\n    history_keep      = 100\n  }\n}\n\n"
            % (args.host, args.port, args.database, args.user, args.password_file)
        )
    text = head + storage + "access {" + tail
    # The ClickHouse block of the template needs two files. The drill never queries ClickHouse.
    ch_password = os.path.join(directory, "clickhouse.pw")
    ch_ca = os.path.join(directory, "clickhouse-ca.pem")
    with open(ch_password, "w", encoding="utf-8") as f:
        f.write("unused-by-the-drill\n")
    with open(CH_CA, "rb") as src, open(ch_ca, "wb") as dst:
        dst.write(src.read())
    text = text.replace("/run/secrets/clickhouse/password", ch_password)
    text = text.replace("/run/secrets/clickhouse/ca.pem", ch_ca)
    text = text.replace("0.0.0.0:8080", "127.0.0.1:%d" % free_port())
    text = text.replace("0.0.0.0:9090", "127.0.0.1:%d" % free_port())
    text = text.replace('"30s"', '"8s"')
    with open(args.out, "w", encoding="utf-8") as f:
        f.write(text)


# ---- The server ----


def listen_addresses(config):
    """Read the API address and the ops address from a configuration file written by `config`."""
    with open(config, encoding="utf-8") as f:
        text = f.read()
    found = {}
    for key in ("listen", "ops_listen"):
        m = re.search(r'^\s*%s\s*=\s*"127\.0\.0\.1:(\d+)"' % key, text, re.M)
        if not m:
            die("the key server.%s is not in %s" % (key, config))
        found[key] = int(m.group(1))
    return found["listen"], found["ops_listen"]


def request(port, method, path, body=None, headers=None):
    """Send one HTTP request to 127.0.0.1. Return the status, the headers and the body bytes."""
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        data = None
        head = dict(headers or {})
        if body is not None:
            data = json.dumps(body, ensure_ascii=False).encode("utf-8")
            head["Content-Type"] = "application/json"
        connection.request(method, path, body=data, headers=head)
        response = connection.getresponse()
        return response.status, dict(response.getheaders()), response.read()
    finally:
        connection.close()


class Server:
    """A running `hune serve`. Use it in a `with` block: the exit stops the process."""

    def __init__(self, binary, config):
        self.binary = binary
        self.config = config
        self.api, self.ops = listen_addresses(config)
        self.process = None
        self.log = None

    def __enter__(self):
        self.log = open(self.config + ".serve.log", "wb")
        self.process = subprocess.Popen(
            [self.binary, "serve", "-c", self.config], stdin=subprocess.DEVNULL, stdout=self.log, stderr=self.log
        )
        deadline = time.time() + 30
        while time.time() < deadline:
            if self.process.poll() is not None:
                die("serve stopped at start; see %s.serve.log" % self.config)
            try:
                status, _, _ = request(self.ops, "GET", "/readyz")
                if status == 200:
                    return self
            except OSError:
                pass
            time.sleep(0.1)
        die("serve is not ready after 30 s; see %s.serve.log" % self.config)

    def __exit__(self, *exc):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.log.close()
        return False

    def call(self, method, path, body=None, headers=None, expect=None):
        status, head, raw = request(self.api, method, path, body, headers)
        if expect is not None and status not in expect:
            die("%s %s gave %d, expected %s: %s" % (method, path, status, expect, raw[:300]))
        return status, head, raw

    def write(self, method, path, body, etag=None, expect=(200, 201)):
        """A write with a new `save_id` (6.6). `etag` is the current etag for `If-Match`."""
        headers = {"Idempotency-Key": str(uuid.uuid4())}
        if etag is not None:
            headers["If-Match"] = '"%s"' % etag
        status, _, raw = self.call(method, path, body, headers, expect)
        return json.loads(raw) if raw else {}


# ---- The documents ----


def load(name):
    with open(os.path.join(DASHBOARDS, name), encoding="utf-8") as f:
        return json.load(f)


def make_document(index, revision):
    """Make the document of dashboard number `index` at revision `revision` (a valid format 2 document)."""
    base = load("nodes.json" if index % 2 == 0 else "minimal.json")
    base["uid"] = "d-%02d" % index
    base["title"] = "Dashboard %02d" % index if revision == 1 else "Dashboard %02d, revision %d" % (index, revision)
    base["description"] = "Synthetic dashboard %d for the restore drill. Café – 日本." % index
    base["tags"] = ["drill", "r%d" % revision]
    return base


# ---- Command seed ----


def command_seed(args):
    """Build the synthetic volume through the API, then add the hook case."""
    root = args.root
    folder_of = {}
    etag_of = {}
    summary = {
        "hook_uid": HOOK_UID,
        "moved_uid": MOVED_UID,
        "restored_uid": RESTORED_UID,
        "deleted_uid": DELETED_UID,
        "hook_etag": hashlib.sha256(HOOK_BYTES).hexdigest(),
    }
    with Server(args.binary, args.config) as server:
        for folder, title in FOLDERS:
            server.write("POST", "/api/folders", {"id": folder, "title": title}, expect=(201,))
        # 20 current dashboards and one more that the seed deletes. The folders get 7 each.
        for index in range(1, COUNT + 2):
            folder = FOLDERS[(index - 1) % len(FOLDERS)][0]
            document = make_document(index, 1)
            answer = server.write(
                "POST", "/api/dashboards", {"folder": folder, "message": "First version", "document": document}
            )
            folder_of[document["uid"]] = folder
            etag_of[document["uid"]] = answer["etag"]
        # Saves: dashboard i gets (i mod 4) more revisions.
        for index in range(1, COUNT + 2):
            uid = "d-%02d" % index
            for revision in range(2, 2 + index % 4):
                document = make_document(index, revision)
                message = "Revision %d of %s – déjà vu" % (revision, uid)
                answer = server.write(
                    "PUT", "/api/dashboards/" + uid, {"message": message, "document": document}, etag_of[uid]
                )
                etag_of[uid] = answer["etag"]
        # One move.
        answer = server.write(
            "POST",
            "/api/dashboards/%s/move" % MOVED_UID,
            {"folder": "research", "message": "Move to research"},
            etag_of[MOVED_UID],
        )
        folder_of[MOVED_UID] = "research"
        etag_of[MOVED_UID] = answer["etag"]
        # One restore: the dashboard goes back to its version 1.
        answer = server.write(
            "POST",
            "/api/dashboards/%s/restore" % RESTORED_UID,
            {"version": 1, "message": "Back to the first version"},
            etag_of[RESTORED_UID],
        )
        etag_of[RESTORED_UID] = answer["etag"]
        # One delete. The uid stays deleted: step 7 of 16.14 restores it with `hune restore`.
        server.write("DELETE", "/api/dashboards/" + DELETED_UID, None, etag_of[DELETED_UID], expect=(200,))
        # The hook of 5.14: a writer outside Hune puts an invalid file. Hune sees it at its next scan.
        path = os.path.join(root, folder_of[HOOK_UID], HOOK_UID + ".json")
        if not os.path.isfile(path):
            die("the hook target %s does not exist" % path)
        with open(path, "wb") as f:
            f.write(HOOK_BYTES)
        deadline = time.time() + 30
        while True:
            _, _, raw = server.call("GET", "/api/dashboards?limit=100", expect=(200,))
            item = next(i for i in json.loads(raw)["items"] if i["uid"] == HOOK_UID)
            if item["state"] == "invalid":
                break
            if time.time() > deadline:
                die("Hune did not see the invalid file of %s" % HOOK_UID)
            time.sleep(0.2)
        if item["etag"] != summary["hook_etag"]:
            die("the etag of the invalid file is %s, not the SHA-256 of its bytes" % item["etag"])
        # A save with the etag of the raw bytes replaces the invalid file (5.14, step 8).
        answer = server.write(
            "PUT",
            "/api/dashboards/" + HOOK_UID,
            {"message": "Repair after the hook", "document": make_document(2, 9)},
            item["etag"],
            expect=(200,),
        )
        etag_of[HOOK_UID] = answer["etag"]
        summary["hook_save_version"] = answer["version"]
        _, _, raw = server.call("GET", "/api/dashboards/%s/history?limit=100" % HOOK_UID, expect=(200,))
        kinds = [(i["version"], i["kind"], i["size"], i["etag"]) for i in json.loads(raw)["items"]]
        external = [k for k in kinds if k[1] == "external"]
        if len(external) != 1 or external[0][2] is not None or external[0][3] != summary["hook_etag"]:
            die("the history of %s has no external version without bytes: %s" % (HOOK_UID, kinds))
        summary["hook_external_version"] = external[0][0]
        # The final list must hold 20 valid dashboards.
        _, _, raw = server.call("GET", "/api/dashboards?limit=100", expect=(200,))
        items = json.loads(raw)["items"]
        if len(items) != COUNT or any(i["state"] != "valid" for i in items):
            die("the seed volume holds %d dashboards, expected %d valid" % (len(items), COUNT))
    with open(args.summary, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, sort_keys=True)
        f.write("\n")
    print(
        "seed: %d dashboards in %d folders; external version %d of %s"
        % (COUNT, len(FOLDERS), summary["hook_external_version"], HOOK_UID)
    )


# ---- Command list ----


def command_list(args):
    """Print the list of dashboards of a served store, without the time members."""
    with Server(args.binary, args.config) as server:
        _, _, raw = server.call("GET", "/api/dashboards?limit=100", expect=(200,))
        items = json.loads(raw)["items"]
        for item in sorted(items, key=lambda i: i["uid"]):
            print(
                "\t".join(
                    [
                        item["uid"],
                        item["folder"],
                        item["title"],
                        ",".join(item["tags"]),
                        item["etag"],
                        str(item["version"]),
                        item["state"],
                    ]
                )
            )


# ---- Command check ----


def read_metas(directory, uid):
    """Give the metadata of the versions of a uid in an export, as {number: (meta, has_bytes)}."""
    base = os.path.join(directory, ".history", uid)
    found = {}
    for name in sorted(os.listdir(base)):
        m = re.fullmatch(r"([1-9][0-9]*)\.meta\.json", name)
        if m:
            with open(os.path.join(base, name), encoding="utf-8") as f:
                found[int(m.group(1))] = (json.load(f), os.path.exists(os.path.join(base, m.group(1) + ".json")))
    return found


def command_check(args):
    """Check one export. `--what external` or `--what head-kind`."""
    if args.what == "external":
        with open(args.summary, encoding="utf-8") as f:
            summary = json.load(f)
        uid = summary["hook_uid"]
        metas = read_metas(args.dir, uid)
        n = summary["hook_external_version"]
        if n not in metas:
            die("%s: version %d of %s is absent" % (args.dir, n, uid))
        meta, has_bytes = metas[n]
        problems = []
        if meta["kind"] != "external":
            problems.append("kind is %r" % meta["kind"])
        if meta["saved_by"] != "external":
            problems.append("saved_by is %r" % meta["saved_by"])
        if meta["etag"] != summary["hook_etag"]:
            problems.append("etag is %r, not the SHA-256 of the raw bytes" % meta["etag"])
        if has_bytes:
            problems.append("the file %d.json exists, but the version has no bytes" % n)
        following = metas.get(n + 1)
        if following is None or following[0]["kind"] != "save" or not following[1]:
            problems.append("version %d is not a save with bytes" % (n + 1))
        if problems:
            die("%s: external version of %s: %s" % (args.dir, uid, "; ".join(problems)))
        print("external version %d of %s without bytes: ok (%s)" % (n, uid, args.dir))
    else:
        metas = read_metas(args.dir, args.uid)
        head = max(metas)
        print(metas[head][0]["kind"])


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("config")
    p.add_argument("--backend", choices=["volume", "postgres"], required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--path")
    p.add_argument("--host")
    p.add_argument("--port", type=int)
    p.add_argument("--database")
    p.add_argument("--user")
    p.add_argument("--password-file")
    p.set_defaults(run=command_config)
    p = sub.add_parser("seed")
    p.add_argument("--binary", required=True)
    p.add_argument("--config", required=True)
    p.add_argument("--root", required=True)
    p.add_argument("--summary", required=True)
    p.set_defaults(run=command_seed)
    p = sub.add_parser("list")
    p.add_argument("--binary", required=True)
    p.add_argument("--config", required=True)
    p.set_defaults(run=command_list)
    p = sub.add_parser("check")
    p.add_argument("--what", choices=["external", "head-kind"], required=True)
    p.add_argument("--dir", required=True)
    p.add_argument("--summary")
    p.add_argument("--uid")
    p.set_defaults(run=command_check)
    args = parser.parse_args()
    if args.command == "config":
        if args.backend == "volume" and not args.path:
            die("config: --path is necessary for a volume")
        if args.backend == "postgres" and not all([args.host, args.port, args.database, args.user, args.password_file]):
            die("config: --host --port --database --user --password-file are necessary for postgres")
    args.run(args)


if __name__ == "__main__":
    main()
