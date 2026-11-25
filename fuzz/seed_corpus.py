#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Make the seed corpora of the fuzz targets (16.16) from the fixtures of the repository.

Use: python3 -I fuzz/seed_corpus.py
The script writes small files to fuzz/corpus/<target>/. It does not delete a file that the
fuzzer made. A seed file has the name "seed-<name>".
"""
import json
import pathlib
import re
import struct

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "fuzz" / "corpus"
MAX_SEED_BYTES = 64 * 1024


def put(target, name, data, limit=MAX_SEED_BYTES):
    if isinstance(data, str):
        data = data.encode("utf-8")
    if len(data) > limit:
        return
    folder = OUT / target
    folder.mkdir(parents=True, exist_ok=True)
    safe = re.sub(r"[^A-Za-z0-9._-]+", "_", name)[:80]
    (folder / ("seed-" + safe)).write_bytes(data)


def golden_cases():
    for name in ("macros", "contexts", "variables", "spec"):
        path = ROOT / "tests" / "golden" / "sql" / (name + ".json")
        for index, case in enumerate(json.loads(path.read_text())):
            yield "%s-%02d" % (name, index), case


def seed_lexer():
    for name, case in golden_cases():
        put("lexer", "golden-" + name, "\n".join(case["sql"]))
    vectors = json.loads((ROOT / "tests" / "fixtures" / "sql-lex-vectors.json").read_text())
    for index, vector in enumerate(vectors):
        put("lexer", "vector-%02d" % index, vector["sql"])
    hand = [
        "select $$a'b$$, $tag$ x $tag$ from t",
        "/* a /* nested */ b */ select 1 -- end",
        "select 'it''s', 'a\\'b', \"q\"\"n\", `b\\`c` # no comment",
        "select 1.5e-3, .5, 0x1F, 1e, 1_000, -5 --5\nfrom t",
        "select 'é中\U0001F600',     1",
        "select '$e', ${e:csv}, $__timeFilter(ts), $",
        "select 'unclosed",
        "/* unclosed",
        "$tag$ unclosed",
    ]
    for index, text in enumerate(hand):
        put("lexer", "hand-%02d" % index, text)


def seed_lexer_pg():
    """Seeds of the target lexer_pg: the vectors of 7.20, the templates of the PostgreSQL golden files, hand texts."""
    vectors = json.loads((ROOT / "tests" / "fixtures" / "sql-lex-vectors-pg.json").read_text())
    for index, vector in enumerate(vectors):
        put("lexer_pg", "vector-%02d" % index, vector["sql"])
    for path in sorted((ROOT / "tests" / "golden" / "expansion-pg").glob("*.json")):
        case = json.loads(path.read_text())
        put("lexer_pg", "golden-" + path.stem, case["template"])
        if case.get("expected"):
            put("lexer_pg", "expected-" + path.stem, case["expected"])
    hand = [
        "select E'a\\'b', B'101', X'1F', N'x', U&'d\\0061t' UESCAPE '!'",
        "select $1, $2::text, $a$ x $a$, $$ $$, a$b, $",
        "select 1 /* a /* nested */ b */ -- end\n# x",
        "select x @-1, y !-2, z <->-3, a ||/ b, c#>>d",
        "select 'a'\n'b' 'c'\n  E'd\\''\n'e'",
        "select U&\"d\\0061t\", \"a\"\"b\", 1.5e+3, 1e, .5, 1_000, 0x1F, 1..2",
        "select 'unclosed",
        "select E'unclosed\\",
        "/* unclosed",
        "$tag$ unclosed",
        "select U&",
        "select 'é中\U0001F600', $é$ x $é$",
    ]
    for index, text in enumerate(hand):
        put("lexer_pg", "hand-%02d" % index, text)


HEADER_KEYS = ("s", "n", "l", "q", "c", "a", "t")


def expand_header(template, flags=0, points=1200, length=21599):
    start = 1794211200
    return struct.pack(">BBHIHHH", template, flags, points, start, length, 0, 0)


def expand_seed(template, sql, sections, flags=0):
    body = b""
    if template == 0:
        body += sql.encode("utf-8") + b"\0"
    body += b"\0".join(b"\x01".join(v.encode("utf-8") for v in values) for values in sections)
    return expand_header(template, flags) + body


def seed_expand():
    mapping = {"query": "q", "custom": "c", "text": "s"}
    for name, case in golden_cases():
        sql = "\n".join(case["sql"])
        used = {}
        sections = {}
        for var in case["variables"]:
            kind = var["kind"]
            letter = "l" if kind == "query" and var.get("multi") else mapping[kind]
            if letter in used.values():
                continue
            used[var["name"]] = letter
        for old, letter in used.items():
            sql = re.sub(r"\$\{%s(?=[:}])" % re.escape(old), "${" + letter, sql)
            sql = re.sub(r"\$%s\b" % re.escape(old), "$" + letter, sql)
            member = case["request"].get(old)
            if member is not None:
                sections[letter] = member["values"]
        ordered = [sections.get(key, []) for key in HEADER_KEYS]
        put("expand", "golden-" + name, expand_seed(0, sql, ordered))
    hostile = [
        ["x"],
        ["1"],
        ["-5"],
        ["a' OR 1=1 --"],
        ["\\"],
        ["\\' ; DROP TABLE t; --"],
        ["$__timeFilter(x) ${s:raw} $s"],
        ["/*"],
        ["*/ 1 /*"],
        ["--"],
        [""],
        ["1", "-2", "3.5"],
        ["a", "b'c", "d\\e"],
        [" ", " "],
        ["$$"],
    ]
    for template in range(1, 12):
        for index, values in enumerate(hostile):
            sections = [values, values, values, values, [], [], []]
            put("expand", "template-%02d-%02d" % (template, index), expand_seed(template, "", sections))


def seed_expand_open():
    """Seeds of the target expand with the flag bits 4 and 5 (F41): the open `custom` and `query` variables."""
    open_custom, open_query = 16, 32
    texts = [
        "SELECT '$c', ${a:singlequote}, ${c:sqlstring} FROM t",
        "SELECT 1 FROM `$a` WHERE x = $c",
        "SELECT 1 WHERE x IN (${c:csv}) AND y = ${a:raw}",
        "SELECT 1 WHERE x = '$q' AND y IN ($l) AND z = ${t:singlequote}",
    ]
    hostile = [["x"], ["1"], ["a' OR 1=1 --"], ["\\"], ["$secret"], ["$__all"], [""], ["a", "b'c", "d\\e"], ["/*"], ["--"]]
    for t_index, text in enumerate(texts):
        for h_index, values in enumerate(hostile):
            sections = [[], [], values, values, values, values[:1], values[:1]]
            for flags, tag in ((open_custom, "custom"), (open_query, "query"), (open_custom | open_query, "both")):
                put("expand", "open-%s-%02d-%02d" % (tag, t_index, h_index), expand_seed(0, text, sections, flags=flags))


def seed_expand_pg():
    """Seeds of the target expand with the flag bit 3 (the dialect postgres): the golden files and the templates."""
    mapping = {"query": "q", "custom": "c", "text": "s"}
    pg = 8
    for path in sorted((ROOT / "tests" / "golden" / "expansion-pg").glob("*.json")):
        case = json.loads(path.read_text())
        sql = case["template"]
        used = {}
        sections = {}
        for var in case["variables"]:
            kind = var["kind"]
            letter = "l" if kind == "query" and var.get("multi") else mapping.get(kind, "q")
            if letter in used.values():
                continue
            used[var["name"]] = letter
        for old, letter in used.items():
            sql = re.sub(r"\$\{%s(?=[:}])" % re.escape(old), "${" + letter, sql)
            sql = re.sub(r"\$%s\b" % re.escape(old), "$" + letter, sql)
            member = case["values"].get(old)
            if member is not None:
                sections[letter] = member["values"]
        ordered = [sections.get(key, []) for key in HEADER_KEYS]
        put("expand", "pg-golden-" + path.stem, expand_seed(0, sql, ordered, flags=pg))
    hostile = [
        ["x"],
        ["1"],
        ["-5"],
        ["\\"],
        ["a' OR 1=1 --"],
        ["\\' ; DROP TABLE t; --"],
        ["$$ x $$"],
        ["$__timeFilter(x) ${s:raw} $s"],
        ["*/ 1 /*"],
        ["--"],
        [""],
        ["1", "-2", "3.5"],
        ["a", "b'c", "d\\e"],
        ["U&", "E", "B"],
    ]
    for template in range(1, 15):
        for index, values in enumerate(hostile):
            sections = [values, values, values, values, [], [], []]
            put("expand", "pg-template-%02d-%02d" % (template, index), expand_seed(template, "", sections, flags=pg))


def seed_document():
    folder = ROOT / "tests" / "fixtures" / "dashboards"
    for path in sorted(folder.rglob("*.json")):
        name = str(path.relative_to(folder).with_suffix("")).replace("/", "-")
        put("document_parse", "fixture-" + name, path.read_bytes())
    put("document_parse", "folder-ok", '{"format_version":1,"title":"Weights"}')
    put("document_parse", "folder-no-version", '{"title":"Weights"}')
    put("document_parse", "folder-extra", '{"format_version":1,"title":"x","extra":1}')
    put("document_parse", "empty-object", "{}")
    put("document_parse", "not-object", "[1,2]")
    # Each form of `repeat` (4.13a): the fixture `repeats.json` holds them too. These are the small forms.
    base = json.loads((folder / "repeats.json").read_text())
    panel_h = base["sections"][0]["panels"][0]
    for name, member in (
        ("panel-h-max", panel_h["repeat"]),
        ("panel-h", {"variable": "host", "direction": "h"}),
        ("panel-v", {"variable": "region", "direction": "v"}),
    ):
        doc = json.loads(json.dumps(base))
        doc["sections"][0]["panels"][0]["repeat"] = member
        put("document_parse", "repeat-" + name, json.dumps(doc))
    doc = json.loads(json.dumps(base))
    doc["sections"][1]["repeat"] = {"variable": "region"}
    put("document_parse", "repeat-section", json.dumps(doc))
    for name, member in (
        ("max-wrong-type", {"variable": "host", "direction": "h", "max_per_row": "3"}),
        ("direction-v-max", {"variable": "host", "direction": "v", "max_per_row": 2}),
        ("empty", {}),
    ):
        doc = json.loads(json.dumps(base))
        doc["sections"][0]["panels"][0]["repeat"] = member
        put("document_parse", "repeat-bad-" + name, json.dumps(doc))
    seed_text_panels(folder)
    seed_timeline_panels(folder)
    seed_bucket_panels(folder)
    seed_queries(folder)
    seed_variable_kinds(folder)
    seed_more_transforms(folder)


def seed_more_transforms(folder):
    """Seeds of the eleven transforms of milestone F (F28, 4.25a): the bounds, the order of the members, hostile text."""
    base = json.loads((folder / "transforms_more.json").read_text())
    typical = base["sections"][0]["panels"][0]["transforms"]

    def put_with(name, edit):
        doc = json.loads(json.dumps(base))
        edit(doc["sections"][0]["panels"][0]["transforms"])
        put("document_parse", "more-transforms-" + name, json.dumps(doc))

    # One kind alone, and the members in the reverse order.
    for index in range(len(typical)):
        put_with("kind-%02d" % index, lambda t, i=index: t.__setitem__(slice(None), [t[i]]))
        put_with("reversed-%02d" % index, lambda t, i=index: t.__setitem__(i, dict(reversed(list(t[i].items())))))
    hostile = ["<img src=x onerror=alert(1)>", "$1$$${a}", "a'b\"c\\d", "\u0000", "é" * 256, "x" * 1025, ""]
    for index, text in enumerate(hostile):
        put_with("hostile-name-%02d" % index, lambda t, x=text: t[0].update({"hide": [x], "rename": [{"field": "a", "name": x}]}))
        put_with("hostile-alias-%02d" % index, lambda t, x=text: t[1].update({"alias": x}))
        put_with("hostile-pattern-%02d" % index, lambda t, x=text: t[9].update({"regex": x, "rename_pattern": x}))
    patterns = ["(a+)+$", "(?=a)", "\\1", "[a", "a{99999}", "^(" + "a|" * 300 + "b)$", "/x/gimsx", "/(/"]
    for index, pattern in enumerate(patterns):
        put_with("pattern-%02d" % index, lambda t, x=pattern: t[8].update({"include": {"names": [], "pattern": x}}))
    for index, count in enumerate([-1, 0, 1000000, 1000001, 2.5, 1e6, 18446744073709551616, "5", None]):
        put_with("limit-%02d" % index, lambda t, c=count: t[3].update({"count": c}))
    put_with("hide-65", lambda t: t[0].update({"hide": ["h%d" % i for i in range(65)]}))
    put_with("hide-dup", lambda t: t[0].update({"hide": ["a", "a"]}))
    put_with("sort-9", lambda t: t[2].update({"sort": [{"field": "f%d" % i, "desc": False} for i in range(9)]}))
    put_with("reducers-19", lambda t: t[5].update({"reducers": ["max"] * 19}))
    put_with("partition-5", lambda t: t[6].update({"fields": list("abcde")}))
    put_with("convert-17", lambda t: t[10].update({"conversions": [{"field": "c%d" % i, "to": "text"} for i in range(17)]}))
    put_with("mode-two-objects", lambda t: t[1].update({"unary": {"operator": "abs", "field": "a"}, "index": {"as_percentile": True}}))
    put_with("mode-wrong-object", lambda t: (t[1].pop("binary"), t[1].update({"unary": {"operator": "abs", "field": "a"}})))
    put_with("operand-both", lambda t: t[1]["binary"].update({"left": {"kind": "field", "field": "a", "value": 1}}))
    put_with("operand-numbers", lambda t: t[1]["binary"].update({"left": {"kind": "number", "value": 1}}))
    put_with("operand-huge", lambda t: t[1]["binary"].update({"right": {"kind": "number", "value": 1e308}}))
    put_with("operand-overflow", lambda t: t[1]["binary"].update({"right": {"kind": "number", "value": 1e400}}))
    put_with("merge-member", lambda t: t[4].update({"options": {}}))
    put_with("unknown-kind", lambda t: t[4].update({"kind": "prepare_time_series"}))
    put_with("sixteen", lambda t: t.__setitem__(slice(None), [t[4]] * 16))
    put_with("seventeen", lambda t: t.__setitem__(slice(None), [t[4]] * 17))


def seed_variable_kinds(folder):
    """Seeds of the variable kinds `interval` and `constant` (F27, 4.10a, 4.10b): the bounds and each refusal."""
    base = json.loads((folder / "variable_kinds.json").read_text())

    def put_with(name, edit):
        doc = json.loads(json.dumps(base))
        edit(doc["variables"])
        put("document_parse", "variable-kinds-" + name, json.dumps(doc))

    put_with("one-item", lambda v: v[0].update({"intervals": ["1ms"], "default": None}))
    put_with("item-365-days", lambda v: v[0].update({"intervals": ["365d"], "default": None}))
    put_with("item-9-digits", lambda v: v[0].update({"intervals": ["999999999ms"], "default": None}))
    put_with("item-overflow", lambda v: v[0].update({"intervals": ["999999999y"], "default": None}))
    put_with("item-10-digits", lambda v: v[0].update({"intervals": ["1000000000ms"], "default": None}))
    put_with("item-unicode-digit", lambda v: v[0].update({"intervals": ["1\u0661m"], "default": None}))
    put_with("item-hostile", lambda v: v[0].update({"intervals": ["1m'; DROP TABLE t; --"], "default": None}))
    put_with("auto-count-max", lambda v: v[0].update({"auto_count": 1000}))
    put_with("auto-count-huge", lambda v: v[0].update({"auto_count": 18446744073709551616}))
    put_with("auto-min-not-table", lambda v: v[0].update({"auto_min": "90s"}))
    put_with("default-auto-no-auto", lambda v: v[0].update({"auto": False}))
    put_with("constant-empty", lambda v: v[3].update({"value": ""}))
    put_with("constant-hostile", lambda v: v[2].update({"value": "a'b\"c$d\\e;--x ${x} $__from"}))
    put_with("constant-control", lambda v: v[2].update({"value": "a\u0000b"}))
    put_with("constant-hide-none", lambda v: v[2].update({"hide": "none"}))


def text_panel_member(content, mode="markdown"):
    """A text panel of format 3 (4.33a) with the given lines."""
    return {
        "id": 7, "type": "text", "title": "Notes", "description": "", "transparent": False,
        "grid": {"x": 0, "y": 0, "w": 12, "h": 8},
        "options": {"mode": mode, "content": content}, "links": [],
    }


def seed_text_panels(folder):
    """Seeds of the text panel (F29, 4.33a): each boundary of the limits, each refusal and a hostile content."""
    base = json.loads((folder / "minimal.json").read_text())

    def with_panel(panel):
        doc = json.loads(json.dumps(base))
        doc["sections"][0]["panels"] = [panel]
        return json.dumps(doc)

    big = 80 * 1024
    cases = [
        ("empty", [], "markdown"),
        ("one-line", ["# Title"], "markdown"),
        ("tab-and-unicode", ["a\tb", "x\u2028y", "\U0001F600"], "markdown"),
        ("lines-2000", ["line %d" % i for i in range(2000)], "markdown"),
        ("lines-2001", ["x"] * 2001, "markdown"),
        ("line-8192", ["a" * 8190 + "\u00e9"], "markdown"),
        ("line-8193", ["a" * 8193], "markdown"),
        ("bytes-65536", ["b" * 8191] * 7 + ["c" * 8192], "markdown"),
        ("bytes-65537", ["b" * 8192] + ["b" * 8191] * 6 + ["c" * 8192], "markdown"),
        ("control-lf", ["a\nb"], "markdown"),
        ("control-cr", ["a\rb"], "markdown"),
        ("control-nul", ["a\u0000b"], "markdown"),
        ("control-del", ["a\u007fb"], "markdown"),
        ("mode-html", ["<b>x</b>"], "html"),
        ("mode-code", ["x"], "code"),
        ("not-text-item", ["a", 3], "markdown"),
        (
            "hostile",
            [
                "<script>alert(1)</script>", "<img src=x onerror=alert(1)>", "[x](javascript:alert(1))",
                "![a](http://evil.example/i.png)", "> > > > > > > > x", "*a " * 40, "${__from} [[x]] $__to",
                "\u202ertl \u200bzero width", "`" * 9, "\\" * 9,
            ],
            "markdown",
        ),
    ]
    for name, content, mode in cases:
        put("document_parse", "text-" + name, with_panel(text_panel_member(content, mode)), limit=big)
    members = (("data", {"reuse": {"panel": 1, "with_transforms": False}}), ("transforms", []),
               ("fields", {"defaults": {}, "overrides": []}), ("repeat", {"variable": "host", "direction": "v"}))
    for name, value in members:
        panel = text_panel_member(["x"])
        panel[name] = value
        put("document_parse", "text-has-" + name, with_panel(panel))
    panel = text_panel_member(["x"])
    panel["options"]["extra"] = True
    put("document_parse", "text-options-extra", with_panel(panel))
    put("document_parse", "text-no-mode", with_panel({**text_panel_member(["x"]), "options": {"content": ["x"]}}))
    put("document_parse", "text-type-texts", with_panel({**text_panel_member(["x"]), "type": "texts"}))


def seed_timeline_panels(folder):
    """Seeds of the state timeline and the status history (F30, 4.33b): each boundary of each option."""
    timeline = json.loads((folder / "state_timeline.json").read_text())
    history = json.loads((folder / "status_history.json").read_text())
    for kind, base in (("timeline", timeline), ("history", history)):
        members = [("row_height", 10, 100), ("line_width", 0, 10), ("fill_opacity", 0, 100)]
        if kind == "history":
            members.append(("col_width", 10, 100))
        for member, low, high in members:
            # The two boundaries, and the two values just outside them.
            for tag, value in (("low", low), ("high", high), ("below", low - 1), ("above", high + 1)):
                doc = json.loads(json.dumps(base))
                doc["sections"][0]["panels"][0]["options"][member] = value
                put("document_parse", "%s-%s-%s" % (kind, member.replace("_", "-"), tag), json.dumps(doc))
        for name, edit in (
            ("legend-table", lambda o: o["legend"].update(mode="table")),
            ("legend-values", lambda o: o["legend"].update(values=["last"])),
            ("legend-sort", lambda o: o["legend"].update(sort_by="name", sort_desc=True)),
            ("tooltip-sort", lambda o: o["tooltip"].update(sort="asc", hide_zeros=True)),
            ("other-type", lambda o: o.update(merge_values=True, col_width=90)),
        ):
            doc = json.loads(json.dumps(base))
            edit(doc["sections"][0]["panels"][0]["options"])
            put("document_parse", "%s-%s" % (kind, name), json.dumps(doc))
    doc = json.loads(json.dumps(timeline))
    doc["sections"][0]["panels"][0]["type"] = "state_timelines"
    put("document_parse", "timeline-type-timelines", json.dumps(doc))


def seed_bucket_panels(folder):
    """Seeds of the heatmap and the histogram (F31, 4.33c, 4.33d): each boundary of each option."""
    heatmap = json.loads((folder / "heatmap.json").read_text())
    histogram = json.loads((folder / "histogram.json").read_text())

    def with_options(base, edit):
        doc = json.loads(json.dumps(base))
        edit(doc["sections"][0]["panels"][1 if base is heatmap else 0]["options"])
        return json.dumps(doc)

    for kind, base, members in (
        ("heatmap", heatmap, [("cell_gap", 0, 25), ("x_count", 1, 1000), ("y_count", 1, 1000), ("steps", 2, 128)]),
        ("histogram", histogram, [("bucket_count", 1, 1000), ("line_width", 0, 10), ("fill_opacity", 0, 100)]),
    ):
        for member, low, high in members:
            for tag, value in (("low", low), ("high", high), ("below", low - 1), ("above", high + 1)):
                def edit(o, member=member, value=value):
                    if member == "x_count":
                        o["x_buckets"] = {"mode": "count", "value": value}
                    elif member == "y_count":
                        o["y_buckets"] = {"mode": "count", "value": value, "scale": "linear"}
                    elif member == "steps":
                        o["color"]["steps"] = value
                    else:
                        o[member] = value
                put("document_parse", "%s-%s-%s" % (kind, member.replace("_", "-"), tag), with_options(base, edit))
    for name, edit in (
        ("exponent-low", lambda o: o["color"].update(exponent=0.1)),
        ("exponent-high", lambda o: o["color"].update(exponent=2)),
        ("exponent-two-decimals", lambda o: o["color"].update(exponent=0.55)),
        ("x-size-low", lambda o: o.update(x_buckets={"mode": "size", "value": 1})),
        ("x-size-high", lambda o: o.update(x_buckets={"mode": "size", "value": 31536000000})),
        ("x-size-above", lambda o: o.update(x_buckets={"mode": "size", "value": 31536000001})),
        ("log-count", lambda o: o.update(y_buckets={"mode": "count", "value": None, "scale": "log2"})),
        ("log-split-17", lambda o: o.update(y_buckets={"mode": "size", "value": 17, "scale": "log10"})),
        ("log-axis-zero", lambda o: o.update(y_buckets={"mode": "size", "value": 1, "scale": "log2"}, y_min=0)),
        ("color-range", lambda o: o["color"].update(min=5, max=5)),
        ("tooltip-sort", lambda o: o["tooltip"].update(sort="asc", hide_zeros=True)),
        ("legend-object", lambda o: o.update(legend={"show": True})),
    ):
        put("document_parse", "heatmap-" + name, with_options(heatmap, edit))
    for name, edit in (
        ("size-low", lambda o: o.update(bucket_size=1e-9)),
        ("size-zero", lambda o: o.update(bucket_size=0)),
        ("offset-negative", lambda o: o.update(bucket_offset=-12.5)),
        ("offset-huge", lambda o: o.update(bucket_offset=1e21)),
        ("stacking-group", lambda o: o.update(stacking="group")),
        ("other-type", lambda o: o.update(calculate=True)),
    ):
        put("document_parse", "histogram-" + name, with_options(histogram, edit))
    for kind, base, wrong in (("heatmap", heatmap, "heatmaps"), ("histogram", histogram, "histograms")):
        doc = json.loads(json.dumps(base))
        doc["sections"][0]["panels"][0]["type"] = wrong
        put("document_parse", kind + "-type-" + wrong, json.dumps(doc))


def seed_config():
    folders = [ROOT / "tests" / "fixtures" / "config", ROOT / "deploy"]
    for folder in folders:
        for path in sorted(folder.rglob("*.hcl")):
            rel = str(path.relative_to(ROOT)).replace("/", "_")
            put("hcl_config", rel, path.read_bytes())
    hand = [
        'server {\n  listen = "0.0.0.0:8080"\n  listen = "0.0.0.0:8081"\n}\n',
        'unknown_block {\n}\n',
        'storage {\n  volume "main" {\n    primary = true\n  }\n}\nstorage.volume {\n  path = "/data"\n}\n',
        'clickhouse "prod" {\n  url = "http://ch:8123"\n  password = "plain"\n}\n',
        'clickhouse "prod" {\n  url = "http://ch:8123"\n  password_env = "HUNE_TOKEN"\n}\n',
        'clickhouse "prod" {\n  url = "https://ch:8443"\n  password_file = "relative"\n}\n',
        'log {\n  level = "debug" # c\n}\n/* b */ // l\n',
        'server {\n  max_connections = 1e3\n  idle_timeout = "30s"\n}\n',
        '﻿server {}\n',
    ]
    for index, text in enumerate(hand):
        put("hcl_config", "hand-%02d" % index, text)


def seed_queries(folder):
    """Seeds of several queries in one panel (4.15a): each form of `extra_queries`, `ref` and `hide`, and the bad forms."""
    base = json.loads((folder / "queries.json").read_text())

    def with_data(edit):
        doc = json.loads(json.dumps(base))
        edit(doc["sections"][0]["panels"][2]["data"])
        return json.dumps(doc)

    item = {"ref": "B", "source": "prod", "format": "table", "sql": ["SELECT 1"]}
    for name, edit in (
        ("one-item", lambda d: d.update(extra_queries=[dict(item)])),
        ("item-hide", lambda d: d.update(extra_queries=[{**item, "hide": True}])),
        ("item-hide-false", lambda d: d.update(extra_queries=[{**item, "hide": False}])),
        ("seven-items", lambda d: d.update(extra_queries=[{**item, "ref": "R%d" % i} for i in range(7)])),
        ("query-ref-and-hide", lambda d: d["query"].update(ref="C", hide=True)),
        ("query-ref-a", lambda d: d["query"].update(ref="A", hide=False)),
        ("ref-16-chars", lambda d: d.update(extra_queries=[{**item, "ref": "a" * 16}])),
        ("two-sources", lambda d: d.update(extra_queries=[{**item, "source": "conf"}])),
        ("empty-array", lambda d: d.update(extra_queries=[])),
        ("eight-items", lambda d: d.update(extra_queries=[{**item, "ref": "R%d" % i} for i in range(8)])),
        ("ref-17-chars", lambda d: d.update(extra_queries=[{**item, "ref": "a" * 17}])),
        ("ref-digit", lambda d: d.update(extra_queries=[{**item, "ref": "1"}])),
        ("ref-twice", lambda d: d.update(extra_queries=[{**item, "ref": "A"}])),
        ("item-panel-member", lambda d: d.update(extra_queries=[{**item, "min_interval": "1m"}])),
        ("item-no-ref", lambda d: d.update(extra_queries=[{k: v for k, v in item.items() if k != "ref"}])),
        ("with-reuse", lambda d: d.update(reuse={"panel": 1, "with_transforms": False}, extra_queries=[dict(item)])),
    ):
        put("document_parse", "queries-" + name, with_data(edit))
    doc = json.loads(json.dumps(base))
    doc["sections"][0]["panels"][0]["fields"]["overrides"][0]["match"] = {"by": "query", "value": "B"}
    put("document_parse", "queries-matcher", json.dumps(doc))


def seed_grafana_variable_kinds():
    """Seeds of the variables `interval` and `constant` of Grafana (15.6): lists, bounds and hostile values."""
    intervals = [
        {"type": "interval", "name": "iv", "query": "1m,10m,1h", "auto": True, "auto_count": 30, "auto_min": "10s",
         "current": {"text": "Auto", "value": "$__auto_interval_iv"}},
        {"type": "interval", "name": "iv", "query": "1m,1.5h,1M;-1m 10m,abc,5x,0m", "auto": False},
        {"type": "interval", "name": "iv", "query": "", "auto": True, "auto_count": 0, "auto_min": "zz"},
        {"type": "interval", "name": "iv", "query": ",".join("%dm" % i for i in range(1, 80)), "auto_min": "90s"},
        {"type": "interval", "name": "iv", "query": "1M", "auto_count": "x", "current": {"value": ["1m", "2m"]}},
        {"type": "interval", "name": "iv", "query": 5, "auto": "yes", "current": 7},
    ]
    constants = [
        {"type": "constant", "name": "k", "query": "eu-1", "hide": 2},
        {"type": "constant", "name": "k", "query": "a'b\"c$d\\e;--x", "hide": 0, "current": {"value": "x"}},
        {"type": "constant", "name": "k", "query": "x" * 1025, "hide": 1},
        {"type": "constant", "name": "k", "query": "a\nb"},
        {"type": "constant", "name": "k", "query": None, "options": [{"value": "x"}]},
    ]
    for index, var in enumerate(intervals + constants):
        put(
            "grafana_import",
            "variable-kind-v1-%02d" % index,
            json.dumps({"uid": "a", "title": "T", "schemaVersion": 41, "templating": {"list": [var]}, "panels": []}),
        )
    for form in ("alpha1", "beta1"):
        spec_vars = [
            {"kind": "IntervalVariable", "spec": {"name": "iv", "query": "1m,1M", "auto": True, "auto_min": "90s"}},
            {"kind": "ConstantVariable", "spec": {"name": "k", "query": "eu-1", "hide": "dontHide"}},
        ]
        put(
            "grafana_import",
            "variable-kind-v2-" + form,
            json.dumps(
                {
                    "apiVersion": "dashboard.grafana.app/v2" + form, "kind": "Dashboard", "metadata": {"name": "a"},
                    "spec": {"title": "T", "elements": {}, "variables": spec_vars,
                             "layout": {"kind": "RowsLayout", "spec": {"rows": []}}},
                }
            ),
        )


def seed_grafana_more_transforms():
    """Seeds of the eleven transformations of milestone F in the import (F28, 15.6): each id, each option that the
    measure found, and the values that the table refuses."""
    ds = {"type": "grafana-clickhouse-datasource", "uid": "ch-main"}
    target = {"datasource": ds, "editorType": "sql", "format": 1, "rawSql": "SELECT 1", "refId": "A"}

    def dashboard(transformations):
        return {
            "uid": "t", "title": "T", "schemaVersion": 41, "templating": {"list": []},
            "panels": [{"id": 1, "type": "table", "title": "p", "datasource": ds, "targets": [target],
                        "gridPos": {"h": 4, "w": 12, "x": 0, "y": 0}, "transformations": transformations}],
        }

    cases = {
        "organize": {"excludeByName": {"a": True, "b": False}, "indexByName": {"c": 0, "a": 0},
                     "renameByName": {"a": "A", "b": ""}, "includeByName": {"a": True}},
        "calculateField": {"mode": "binary", "binary": {"left": {"matcher": {"id": "byName", "options": "x"}}, "operator": "+",
                                                         "right": {"fixed": "2"}}, "alias": "S", "replaceFields": True},
        "calculateField-reduce": {"mode": "reduceRow",
                                  "reduce": {"reducer": "sum", "include": ["x", "x"], "nullValueMode": "null as zero"}},
        "calculateField-unary": {"mode": "unary", "unary": {"operator": "percent", "fieldName": "x"}, "ignoreNulls": True},
        "calculateField-index": {"mode": "index", "index": {"asPercentile": True}, "timeSeries": False},
        "calculateField-window": {"mode": "windowFunctions", "window": {"field": "x", "windowSize": 3}},
        "sortBy": {"sort": [{"field": "a", "desc": True}] * 10, "reverse": True},
        "limit-neg": {"limitField": "-2.5"},
        "limit-big": {"limitField": 1e30},
        "limit-var": {"limitField": "$n"},
        "merge": {},
        "reduce": {"mode": "reduceFields", "reducers": ["sum", "sum", "p50", "diffperc"], "fields": "/x/",
                   "includeTimeField": True},
        "partitionByValues": {"fields": ["a", "b", "c", "d", "e"], "keepFields": "yes",
                              "naming": {"asLabels": False, "append": True}},
        "joinByField": {"byField": "a" * 2000, "mode": "outerTabular"},
        "seriesToColumns": {"byField": "", "mode": "inner"},
        "filterFieldsByName": {"include": {"names": ["a", "a"], "pattern": "(?=x)"}, "exclude": {"pattern": "/b/i"},
                               "byVariable": True},
        "renameByRegex": {"regex": "(a+)+$", "renamePattern": "$1$$${a}"},
        "convertFieldType": {"conversions": [{"targetField": "a", "destinationType": "string", "dateFormat": "YYYY"},
                                              {"targetField": "a", "destinationType": "other"},
                                              {"destinationType": "number"}]},
    }
    for name, options in cases.items():
        item = {"id": name.split("-")[0], "options": options}
        put("grafana_import", "more-transforms-" + name, json.dumps(dashboard([item])))
    every = [{"id": n.split("-")[0], "options": o} for n, o in cases.items()]
    put("grafana_import", "more-transforms-all", json.dumps(dashboard(every)))
    bare = [{"id": n} for n in ("organize", "limit", "merge", "reduce")]
    put("grafana_import", "more-transforms-no-options", json.dumps(dashboard(bare)))
    put("grafana_import", "more-transforms-17", json.dumps(dashboard([{"id": "merge", "options": {}}] * 17)))


def seed_grafana():
    folder = ROOT / "tests" / "fixtures" / "grafana" / "export" / "dashboards"
    for path in sorted(folder.glob("*.json")):
        put("grafana_import", "export-" + path.stem, path.read_bytes())
    put("grafana_import", "empty-object", "{}")
    seed_grafana_variable_kinds()
    seed_grafana_more_transforms()
    # The 6 dashboards of several queries in a panel (15.6, 15.14c).
    for path in sorted((ROOT / "tests" / "fixtures" / "grafana").glob("multi_query_*.json")):
        put("grafana_import", path.stem.replace("_", "-"), path.read_bytes())
    # The 8 dashboards of the measure of the state timeline and the status history (15.14c).
    for path in sorted((ROOT / "tests" / "fixtures" / "grafana").glob("timelines_*.json")):
        put("grafana_import", path.stem.replace("_", "-"), path.read_bytes())
    # The 8 dashboards of the measure of the heatmap and the histogram (15.14c).
    for pattern in ("heatmap_*.json", "histogram_*.json"):
        for path in sorted((ROOT / "tests" / "fixtures" / "grafana").glob(pattern)):
            put("grafana_import", path.stem.replace("_", "-"), path.read_bytes())
    # Each form of the repeat (15.6): a v1 panel with and without a direction, a v1 row, a copy of a repeated
    # panel, and the same members with a wrong type.
    repeat_panels = [
        {"id": 1, "type": "stat", "title": "p", "repeat": "host", "repeatDirection": "h", "maxPerRow": 3,
         "gridPos": {"h": 4, "w": 24, "x": 0, "y": 0}, "targets": [{"rawSql": "SELECT 1", "datasource": {"uid": "ch-main"}}]},
        {"id": 2, "type": "stat", "title": "q", "repeat": "host", "gridPos": {"h": 4, "w": 8, "x": 0, "y": 4}},
        {"id": 3, "type": "stat", "title": "copy", "repeatPanelId": 1, "repeatIteration": 1, "scopedVars": {}},
        {"id": 4, "type": "row", "title": "Host", "repeat": "host", "collapsed": False, "panels": [],
         "gridPos": {"h": 1, "w": 24, "x": 0, "y": 8}},
        {"id": 5, "type": "stat", "title": "bad", "repeat": 5, "repeatDirection": "x", "maxPerRow": "a"},
    ]
    variables = [{"type": "custom", "name": "host", "query": "a,b", "multi": True, "includeAll": True}]
    put(
        "grafana_import",
        "repeat-v1",
        json.dumps({"uid": "a", "title": "T", "schemaVersion": 41, "templating": {"list": variables}, "panels": repeat_panels}),
    )
    put(
        "grafana_import",
        "repeat-v2",
        json.dumps(
            {
                "apiVersion": "dashboard.grafana.app/v2beta1", "kind": "Dashboard", "metadata": {"name": "a"},
                "spec": {
                    "title": "T", "elements": {}, "variables": [],
                    "layout": {"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {
                        "title": "Host", "repeat": {"mode": "variable", "value": "host"},
                        "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {
                            "x": 0, "y": 0, "width": 24, "height": 4,
                            "element": {"kind": "ElementReference", "name": "panel-1"},
                            "repeat": {"mode": "variable", "value": "host", "direction": "h", "maxPerRow": 3}}}]}}}}]}},
                },
            }
        ),
    )
    # The text panel (15.6): each mode, an absent mode and content, a content with CR LF, a control character and
    # each construct, a panel with a repeat, and the v2 form.
    text_options = [
        {"mode": "markdown", "content": "# Title\n\nA *text* with a [link](https://wiki.example).",
         "code": {"language": "plaintext", "showLineNumbers": False, "showMiniMap": False}},
        {"mode": "html", "content": "<b>x</b>\n<script>alert(1)</script>"},
        {"mode": "code", "content": "fn main() {}\n```\n````", "code": {"language": "rust", "showLineNumbers": True}},
        {"content": "no mode"},
        {"mode": "markdown"},
        {"mode": "other", "content": "a\r\nb\rc\u0000d"},
        {"mode": "markdown", "content": "<b>x</b> ![i](http://a) &amp; ~~s~~ - [ ] t\nTitle\n===\n\n    code\n[n]: u\n[x](//h) ${v:json} [[v]] $__from"},
        {"mode": "markdown", "content": "> > > > > > > x\n\n" + "| a " * 21 + "|\n" + "|---" * 21 + "|"},
        {"mode": 5, "content": 7, "code": "x", "extra": 1},
        {"mode": "markdown", "content": "x" * 70000},
    ]
    text_panels = [
        {"id": 100 + i, "type": "text", "title": "t", "options": options, "repeat": "host" if i == 2 else None,
         "gridPos": {"h": 3, "w": 8, "x": 0, "y": 3 * i}, "targets": [{"refId": "A"}], "fieldConfig": {"defaults": {}, "overrides": []}}
        for i, options in enumerate(text_options)
    ]
    put(
        "grafana_import",
        "text-v1",
        json.dumps({"uid": "a", "title": "T", "schemaVersion": 41, "templating": {"list": variables}, "panels": text_panels}),
        limit=96 * 1024,
    )
    put(
        "grafana_import",
        "text-v2",
        json.dumps(
            {
                "apiVersion": "dashboard.grafana.app/v2beta1", "kind": "Dashboard", "metadata": {"name": "a"},
                "spec": {
                    "title": "T", "variables": [],
                    "elements": {"panel-1": {"kind": "Panel", "spec": {
                        "id": 1, "title": "t", "links": [],
                        "data": {"kind": "QueryGroup", "spec": {"queries": [], "transformations": [], "queryOptions": {}}},
                        "vizConfig": {"kind": "VizConfig", "group": "text", "version": "12.4.2", "spec": {
                            "options": {"mode": "html", "content": "<i>x</i>"}, "fieldConfig": {"defaults": {}, "overrides": []}}}}}},
                    "layout": {"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {
                        "title": "", "hideHeader": True,
                        "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {
                            "x": 0, "y": 0, "width": 12, "height": 4,
                            "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}}}]}},
                },
            }
        ),
    )
    put("grafana_import", "minimal-v1", '{"uid":"a","title":"T","panels":[],"schemaVersion":39}')
    put(
        "grafana_import",
        "wrapped",
        '{"dashboard":{"uid":"a","title":"T","panels":[{"id":1,"type":"stat","title":"p",'
        '"targets":[{"rawSql":"SELECT 1","datasource":{"uid":"ch-main"}}]}]},"meta":{"folderUid":"f"}}',
    )


FRAMES = [
    '["t","v"]\n["DateTime","Float64"]\n[1794211200,1.5]\n[1794211260,2]\n',
    '["a","b","c"]\n["String","Nullable(Int64)","UInt8"]\n["x",null,1]\n["y\\u00e9",-5,0]\n',
    '["a"]\n["Array(String)"]\n[["x","y"]]\n[[]]\n',
    '["d","e"]\n["Date","DateTime64(3)"]\n["2026-11-10","2026-11-10 12:00:00.123"]\n',
    '["u","i"]\n["UInt64","Int128"]\n[18446744073709551615,-170141183460469231731687303715884105728]\n',
    '["f"]\n["Float32"]\n["nan"]\n["inf"]\n["-inf"]\n[1e308]\n',
    '["b"]\n["Bool"]\n[true]\n[false]\n[null]\n',
    '["m"]\n["LowCardinality(Nullable(String))"]\n["a"]\n[null]\n',
    '["x","y"]\n["String"]\n',
    '["x"]\n["String"]\n["a","b"]\n',
    '["x"]\n["String"]\nnot json\n',
    '[]\n[]\n',
    '["dec"]\n["Decimal(18, 4)"]\n["12.3456"]\n[12.3456]\n',
]


def seed_frame():
    for index, text in enumerate(FRAMES):
        put("frame_decode", "hand-%02d" % index, text)


def seed_annotation():
    """The vectors of `tests/golden/annotations/frames.json` as texts of 8.4 for the target `annotation_frame`."""
    path = ROOT / "tests" / "golden" / "annotations" / "frames.json"
    for index, case in enumerate(json.loads(path.read_text())["cases"]):
        lines = [
            json.dumps([c["name"] for c in case["columns"]]),
            json.dumps([c["type"] for c in case["columns"]]),
        ] + [json.dumps(row) for row in case["rows"]]
        put("annotation_frame", "vector-%02d-%s" % (index, case["name"]), "\n".join(lines) + "\n")


if __name__ == "__main__":
    seed_lexer()
    seed_lexer_pg()
    seed_expand()
    seed_expand_pg()
    seed_expand_open()
    seed_document()
    seed_config()
    seed_grafana()
    seed_frame()
    seed_annotation()
