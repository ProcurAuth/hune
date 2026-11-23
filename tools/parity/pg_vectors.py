#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Capture tool of the PostgreSQL expansion vectors (16.13, case I22; 15.14a).

What the tool does:
  1. It reads the cases of `pg_vector_cases.py`.
  2. It sends each case to the pinned Grafana, which has a PostgreSQL data source.
     A case of the mode `api` goes to `POST /api/ds/query`. A case of the mode `explore` goes through
     the Explore page, with `pg-explore.mjs`, because only the front end computes `$__interval`.
  3. The text `executedQueryString` of the answer is the expected SQL.
  4. It writes one vector file for each case into `tests/golden/expansion-pg/`.

The differences between Hune and Grafana are in `tools/parity/vector-differences.json`. Each key of the
PostgreSQL cases starts with `pg-`. The ClickHouse tool `vectors.mjs` skips these keys.

With `--check`, the tool writes no file. It compares the capture with the committed files. It exits with 1
at the first difference.

The queries read nothing from a table: the SQL of each case only builds a text. Grafana never runs
the data source on a table that holds data.

Use:  python3 -I tools/parity/pg_vectors.py [--url URL] [--uid UID] [--out DIR]
                                            [--differences FILE] [--check] [--only REGEX]
"""
import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))


def load_cases():
    spec = importlib.util.spec_from_file_location("pg_vector_cases", os.path.join(HERE, "pg_vector_cases.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fail(message):
    sys.stderr.write("pg_vectors: %s\n" % message)
    sys.exit(2)


def sent_sql(case):
    """The SQL that the front end sends: the template, with each variable replaced by its SQL text."""
    text = case["template"]
    for name, value in (case["values_sql"] or {}).items():
        text = re.sub(r"\$\{%s\}|\$%s\b" % (name, name), lambda m: value, text)
    return text


def capture_api(case, url, uid):
    sql = sent_sql(case)
    query = {
        "refId": "A",
        "datasource": {"type": "grafana-postgresql-datasource", "uid": uid},
        "rawSql": sql,
        "format": "table",
        "rawQuery": True,
        "editorMode": "code",
        "maxDataPoints": case["mdp"],
        "intervalMs": case["interval_ms"],
    }
    body = {"queries": [query], "from": str(case["from_ms"]), "to": str(case["to_ms"])}
    request = urllib.request.Request(
        url + "/api/ds/query", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}
    )
    try:
        answer = json.load(urllib.request.urlopen(request, timeout=60))
    except urllib.error.HTTPError as e:
        answer = json.loads(e.read())
    result = answer.get("results", {}).get("A", {})
    frame = (result.get("frames") or [{}])[0]
    executed = frame.get("schema", {}).get("meta", {}).get("executedQueryString")
    if executed is None and not result.get("error"):
        fail("no answer for %s: %s (does the data source %s exist?)" % (case["id"], answer.get("message", "no message"), uid))
    return {
        "sent": sql,
        "executed": executed,
        "error": result.get("error"),
        "interval_ms": case["interval_ms"],
        "mdp": case["mdp"],
    }


def capture_explore(cases, url, uid):
    if not cases:
        return {}
    helper = os.path.join(HERE, "pg-explore.mjs")
    process = subprocess.run(
        ["node", helper, "--url", url, "--uid", uid, "--sqls", json.dumps([c["template"] for c in cases])],
        capture_output=True,
        text=True,
        check=False,
    )
    lines = [json.loads(x) for x in process.stdout.splitlines() if x.startswith("{")]
    if process.returncode != 0 or len(lines) != len(cases):
        fail("the Explore helper failed: %s" % process.stderr.strip()[:500])
    for x in lines:
        if x.get("intervalMs") is None:
            fail("Explore did not send the query %r (does the data source %s exist?)" % (x["sql"], uid))
    return {
        c["id"]: {
            "sent": x["sent_sql"],
            "executed": x["executed"],
            "error": x["error"],
            "interval_ms": x["intervalMs"],
            "mdp": x["mdp"],
        }
        for c, x in zip(cases, lines)
    }


def vector(module, case, raw, differences):
    explore = case["mode"] == "explore"
    v = {
        "id": case["id"],
        "tags": case["tags"],
        "grafana": module.GRAFANA_VERSION,
        "plugin": module.PLUGIN,
        "capture": "explore" if explore else "ds-query",
        "template": case["template"],
        "variables": case["variables"],
        "values": case["values"],
    }
    if case["values_sql"]:
        v["values_sql"] = case["values_sql"]
    v.update(
        {
            "from_ms": case["from_ms"],
            "to_ms": case["to_ms"],
            "max_data_points": 1000 if explore else case["mdp"],
            "min_interval": "1m" if explore else None,
            "min_interval_ms": 60000 if explore else None,
            "interval_ms": raw["interval_ms"],
            "expected": raw["executed"],
        }
    )
    if raw["error"]:
        v["grafana_error"] = raw["error"]
    v["sent_sql"] = raw["sent"]
    if case["id"] in differences:
        v["difference"] = differences[case["id"]]
    return json.dumps(v, indent=2, ensure_ascii=False) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--url", default="http://127.0.0.1:3010")
    parser.add_argument("--uid", default="pgcheck", help="the uid of the PostgreSQL data source in Grafana")
    parser.add_argument("--out", default=os.path.join(ROOT, "tests/golden/expansion-pg"))
    parser.add_argument("--differences", default=os.path.join(HERE, "vector-differences.json"))
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--only", default=None)
    args = parser.parse_args()
    module = load_cases()
    cases = module.cases
    ids = [c["id"] for c in cases]
    if len(set(ids)) != len(ids):
        fail("two cases have the same id")
    with open(args.differences, encoding="utf-8") as f:
        every = json.load(f)
    differences = {k: v for k, v in every.items() if k.startswith("pg-")}
    for key in differences:
        if key not in ids:
            fail("%s: unknown case %s" % (args.differences, key))
    if args.only:
        pattern = re.compile(args.only)
        cases = [c for c in cases if pattern.search(c["id"])]
    raw = {c["id"]: capture_api(c, args.url, args.uid) for c in cases if c["mode"] == "api"}
    raw.update(capture_explore([c for c in cases if c["mode"] == "explore"], args.url, args.uid))
    if args.check:
        bad = 0
        for c in cases:
            path = os.path.join(args.out, c["id"] + ".json")
            want = vector(module, c, raw[c["id"]], differences)
            have = open(path, encoding="utf-8").read() if os.path.exists(path) else None
            if have != want:
                bad += 1
                sys.stderr.write("pg_vectors: the vector %s differs from the capture\n" % c["id"])
        if bad:
            sys.exit(1)
        print("%d vectors agree" % len(cases))
        return
    os.makedirs(args.out, exist_ok=True)
    if not args.only:
        for name in os.listdir(args.out):
            if name.endswith(".json"):
                os.remove(os.path.join(args.out, name))
    for c in cases:
        with open(os.path.join(args.out, c["id"] + ".json"), "w", encoding="utf-8") as f:
            f.write(vector(module, c, raw[c["id"]], differences))
    print("%d vectors written" % len(cases))


if __name__ == "__main__":
    main()
