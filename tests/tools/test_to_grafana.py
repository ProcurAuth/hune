# SPDX-License-Identifier: MIT OR Apache-2.0
"""Tests of tools/parity/to_grafana.py (Hune document, format 2, to Grafana classic JSON).

Use: python3 -I tests/tools/test_to_grafana.py -v

The unit tests need only Python. The round trip test needs the debug binary `target/debug/hune`
and skips when it is absent. It converts the fixture `tests/fixtures/dashboards/all_types.json`
(every member of format 2), imports the result with `hune import-grafana`, and compares the
documents. It runs the same test on `f_members.json` (each member that milestone F added).
The only differences that it accepts are the known losses of the converter
(`to_grafana.KNOWN_LOSSES`).
"""
import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "tools", "parity"))
import to_grafana  # noqa: E402

FIXTURE = os.path.join(ROOT, "tests", "fixtures", "dashboards", "all_types.json")
F_MEMBERS = os.path.join(ROOT, "tests", "fixtures", "dashboards", "f_members.json")
PG_ROUND_TRIP = os.path.join(ROOT, "tests", "fixtures", "grafana", "round-trip-pg.json")
REPEATS = os.path.join(ROOT, "tests", "fixtures", "dashboards", "repeats.json")
PARITY_DIR = os.path.join(ROOT, "tests", "fixtures", "parity")
PARITY_REPEAT = [os.path.join(PARITY_DIR, n) for n in ("repeat-h.json", "repeat-v.json", "repeat-section.json")]
PARITY_TEXT = [os.path.join(PARITY_DIR, n) for n in ("text-constructs.json", "text-variables.json", "text-links.json")]
OPEN_VARIABLES = os.path.join(ROOT, "tests", "fixtures", "dashboards", "open_variables.json")
OPEN_VARIABLES_GRAFANA = os.path.join(ROOT, "tests", "fixtures", "grafana", "open_variables.json")
VARIABLE_KINDS = os.path.join(ROOT, "tests", "fixtures", "dashboards", "variable_kinds.json")
VARIABLE_KINDS_GRAFANA = os.path.join(ROOT, "tests", "fixtures", "grafana", "variable_kinds.json")
TEXT_PANEL = os.path.join(ROOT, "tests", "fixtures", "dashboards", "text_panel.json")
STATE_TIMELINE = os.path.join(ROOT, "tests", "fixtures", "dashboards", "state_timeline.json")
STATUS_HISTORY = os.path.join(ROOT, "tests", "fixtures", "dashboards", "status_history.json")
TRANSFORMS_MORE = os.path.join(ROOT, "tests", "fixtures", "dashboards", "transforms_more.json")
HEATMAP = os.path.join(ROOT, "tests", "fixtures", "dashboards", "heatmap.json")
HISTOGRAM = os.path.join(ROOT, "tests", "fixtures", "dashboards", "histogram.json")
ANNOTATIONS = os.path.join(ROOT, "tests", "fixtures", "dashboards", "annotations.json")
QUERIES = os.path.join(ROOT, "tests", "fixtures", "dashboards", "queries.json")
PARITY_QUERIES = [
    os.path.join(PARITY_DIR, "multi-query-%s.json" % n)
    for n in ("timeseries", "table", "stat-hidden", "mixed", "error", "all-hidden")
]
GRAFANA_QUERIES = [
    os.path.join(ROOT, "tests", "fixtures", "grafana", "multi_query_%s.json" % n)
    for n in ("timeseries", "table", "stat_hidden", "mixed", "error", "all_hidden")
]
PARITY_ANNOTATIONS = [
    os.path.join(PARITY_DIR, "annotations-%s.json" % n) for n in ("colours", "filter", "texts", "many")
]
GRAFANA_ANNOTATIONS = [
    os.path.join(ROOT, "tests", "fixtures", "grafana", "annotations_%s.json" % n)
    for n in ("colours", "filter", "texts", "many")
]
# The environment variable HUNE_BINARY names another binary, for example one in a private target folder.
BINARY = os.environ.get("HUNE_BINARY") or os.path.join(ROOT, "target", "debug", "hune")


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def panels(grafana):
    """Every panel of a converted dashboard, the rows excluded, the collapsed rows opened."""
    out = []
    for p in grafana["panels"]:
        if p["type"] == "row":
            out.extend(p["panels"])
        else:
            out.append(p)
    return out


class Convert(unittest.TestCase):
    def setUp(self):
        self.doc = load(FIXTURE)
        self.g = to_grafana.convert(self.doc)

    def test_header(self):
        g = self.g
        self.assertEqual(g["schemaVersion"], 42)
        self.assertEqual(g["uid"], "all-types")
        self.assertEqual(g["title"], self.doc["title"])
        self.assertEqual(g["tags"], self.doc["tags"])
        self.assertEqual(g["time"], {"from": self.doc["time"]["from"], "to": self.doc["time"]["to"]})
        self.assertEqual(g["timezone"], "utc")
        self.assertEqual(g["graphTooltip"], 0)
        self.assertEqual(g["refresh"], "")

    def test_cursor_sync_and_refresh(self):
        doc = copy.deepcopy(self.doc)
        doc["cursor_sync"] = "crosshair"
        doc["time"]["refresh"] = "1m"
        g = to_grafana.convert(doc)
        self.assertEqual(g["graphTooltip"], 1)
        self.assertEqual(g["refresh"], "1m")

    def test_all_seven_types(self):
        types = [p["type"] for p in panels(self.g)]
        self.assertEqual(
            sorted(types), sorted(["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge"])
        )

    def test_query_is_not_changed(self):
        for p, vp in zip(panels(self.g), [p for s in self.doc["sections"] for p in s["panels"]]):
            if "query" not in vp["data"]:
                continue
            target = p["targets"][0]
            self.assertEqual(target["rawSql"], "\n".join(vp["data"]["query"]["sql"]))
            self.assertEqual(target["datasource"], {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"})
            self.assertEqual(target["format"], 1 if vp["data"]["query"]["format"] == "table" else 0)

    def test_postgres_source_reference(self):
        doc = load(PG_ROUND_TRIP)
        g = to_grafana.convert(doc, {"analytics": "pg"}, {"analytics": "postgres"})
        by_id = {p["id"]: p for p in panels(g)}
        ref = {"type": "grafana-postgresql-datasource", "uid": "pg"}
        table, series, other = by_id[10], by_id[20], by_id[40]
        self.assertEqual(table["datasource"], ref)
        self.assertEqual(table["targets"][0]["datasource"], ref)
        self.assertEqual(
            {k: v for k, v in table["targets"][0].items() if k != "rawSql"},
            {"datasource": ref, "editorMode": "code", "format": "table", "rawQuery": True, "refId": "A"},
        )
        self.assertEqual(series["targets"][0]["format"], "time_series")
        self.assertNotIn("queryType", series["targets"][0])
        # A source that has no kind stays a ClickHouse source.
        self.assertEqual(other["datasource"], {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"})
        var = {v["name"]: v for v in g["templating"]["list"]}["q"]
        self.assertEqual(var["datasource"], ref)
        self.assertEqual(var["query"], var["definition"])

    def test_source_map(self):
        g = to_grafana.convert(self.doc, {"prod": "other"})
        self.assertEqual(panels(g)[0]["datasource"]["uid"], "other")

    def test_sections_are_rows(self):
        doc = copy.deepcopy(self.doc)
        titles = [s["title"] for s in doc["sections"]]
        rows = [p for p in to_grafana.convert(doc)["panels"] if p["type"] == "row"]
        self.assertEqual([r["title"] for r in rows], [t for t in titles if t is not None])

    def test_collapsed_row_holds_its_panels(self):
        doc = copy.deepcopy(self.doc)
        doc["sections"][-1]["collapsed"] = True
        g = to_grafana.convert(doc)
        row = [p for p in g["panels"] if p["type"] == "row"][-1]
        self.assertTrue(row["collapsed"])
        self.assertEqual(len(row["panels"]), len(doc["sections"][-1]["panels"]))
        # The panels that follow a collapsed row start under it.
        flat = [p for p in g["panels"] if p["type"] != "row"]
        self.assertTrue(all(p["gridPos"]["y"] < row["gridPos"]["y"] or p in row["panels"] for p in flat))

    def test_grid_y_is_absolute_under_a_row(self):
        doc = copy.deepcopy(self.doc)
        doc["sections"][-1]["collapsed"] = False
        g = to_grafana.convert(doc)
        row = [p for p in g["panels"] if p["type"] == "row"][0]
        after = [p for p in g["panels"] if p["type"] != "row" and p["gridPos"]["y"] > row["gridPos"]["y"]]
        self.assertTrue(after)
        self.assertEqual(min(p["gridPos"]["y"] for p in after), row["gridPos"]["y"] + 1)

    def test_variables(self):
        by_name = {v["name"]: v for v in self.g["templating"]["list"]}
        kinds = {v["kind"] for v in self.doc["variables"]}
        self.assertEqual(kinds, {"query", "custom", "text"})
        for v in self.doc["variables"]:
            out = by_name[v["name"]]
            self.assertEqual(out["type"], {"query": "query", "custom": "custom", "text": "textbox"}[v["kind"]])
            self.assertEqual(out["hide"], {"none": 0, "label": 1, "variable": 2}[v["hide"]])

    def test_current_all(self):
        var = {
            "kind": "custom",
            "name": "x",
            "label": None,
            "hide": "none",
            "options": [{"text": "A", "value": "a"}],
            "multi": True,
            "include_all": True,
            "all_value": None,
            "default": ["$__all"],
        }
        out = to_grafana.variable_out(var, lambda s: s)
        self.assertEqual(out["current"]["value"], ["$__all"])
        self.assertEqual(out["current"]["text"], ["All"])
        self.assertTrue(out["includeAll"])

    def test_allow_custom_value_is_always_written_for_query_and_custom_variables(self):
        # Grafana reads an absent member as true (19.20h): the converter writes true and false, never nothing.
        for kind, extra in (("custom", {"options": [{"text": "A", "value": "a"}]}), ("query", {"source": "prod", "sql": ["SELECT 1"], "refresh": "load", "sort": "none", "regex": None})):
            var = {"kind": kind, "name": "x", "label": None, "hide": "none", "multi": False, "include_all": False,
                   "all_value": None, "default": []}
            var.update(extra)
            self.assertIs(to_grafana.variable_out(var, lambda s: s)["allowCustomValue"], False, kind)
            var["allow_custom_value"] = True
            self.assertIs(to_grafana.variable_out(var, lambda s: s)["allowCustomValue"], True, kind)
        text = {"kind": "text", "name": "t", "label": None, "hide": "none", "value_type": "text", "default": "x"}
        self.assertNotIn("allowCustomValue", to_grafana.variable_out(text, lambda s: s))

    def test_the_open_variables_fixture_converts_to_the_grafana_fixture(self):
        doc = load(OPEN_VARIABLES)
        out = to_grafana.dump(to_grafana.convert(doc))
        with open(OPEN_VARIABLES_GRAFANA, encoding="utf-8") as f:
            self.assertEqual(f.read(), out)
        by_name = {v["name"]: v for v in json.loads(out)["templating"]["list"]}
        self.assertEqual(
            {n: v.get("allowCustomValue") for n, v in by_name.items()},
            {"region_open": True, "region_closed": False, "env_open": True, "env_closed": False, "note": None},
        )
        # The typed default of the open `custom` variable reaches `current`.
        self.assertEqual(by_name["env_open"]["current"]["value"], ["prod", "a typed value"])

    def test_custom_query_escapes_commas(self):
        var = {
            "kind": "custom",
            "name": "x",
            "label": None,
            "hide": "none",
            "options": [{"text": "One", "value": "a,b"}, {"text": "c", "value": "c"}],
            "multi": False,
            "include_all": False,
            "all_value": None,
            "default": ["c"],
        }
        out = to_grafana.variable_out(var, lambda s: s)
        self.assertEqual(out["query"], "One : a\\,b, c")
        self.assertEqual(out["current"], {"selected": True, "text": "c", "value": "c"})

    def test_mappings_keep_order(self):
        items = [
            {"kind": "value", "value": "b", "text": "B", "color": None},
            {"kind": "range", "from": 1, "to": None, "text": None, "color": "red"},
            {"kind": "special", "match": "null_nan", "text": "none", "color": None},
            {"kind": "regex", "pattern": "/^a/", "text": "A", "color": "blue"},
        ]
        out = to_grafana.mappings_out(items)
        self.assertEqual([m["type"] for m in out], ["value", "range", "special", "regex"])
        self.assertEqual(out[0]["options"]["b"]["index"], 0)
        self.assertEqual(out[2]["options"]["match"], "null+nan")
        self.assertNotIn("text", out[1]["options"]["result"])

    def test_thresholds_first_step_is_null(self):
        t = {"mode": "absolute", "steps": [{"value": None, "color": "green"}, {"value": 80, "color": "red"}]}
        self.assertEqual(to_grafana.thresholds_out(t)["steps"][0]["value"], None)

    def test_overrides(self):
        o = [{"match": {"by": "regex", "value": "/^a/"}, "set": {"unit": "ms", "width": 90}}]
        out = to_grafana.overrides_out(o)
        self.assertEqual(out[0]["matcher"], {"id": "byRegexp", "options": "/^a/"})
        self.assertEqual({p["id"]: p["value"] for p in out[0]["properties"]}, {"unit": "ms", "custom.width": 90})

    def test_reduce_and_legend_names(self):
        self.assertEqual(to_grafana.reduce_out({"calc": "total", "fields": None, "values": False})["calcs"], ["sum"])
        lg = {"show": True, "placement": "right", "mode": "table", "values": ["total", "last"]}
        self.assertEqual(to_grafana.legend_out(lg)["calcs"], ["sum", "last"])
        self.assertEqual(to_grafana.tooltip_out({"mode": "hidden"})["mode"], "none")

    def test_barchart_widths(self):
        bar = [p for p in panels(self.g) if p["type"] == "barchart"][0]
        vp = [p for s in self.doc["sections"] for p in s["panels"] if p["type"] == "barchart"][0]
        self.assertAlmostEqual(bar["options"]["barWidth"] * 100, vp["options"]["bar_width"])
        self.assertEqual(bar["options"]["orientation"], vp["options"]["orientation"])

    def test_reuse(self):
        doc = copy.deepcopy(self.doc)
        src = doc["sections"][0]["panels"][0]
        other = copy.deepcopy(src)
        other["id"] = 999
        other["data"] = {"reuse": {"panel": src["id"], "with_transforms": True}}
        doc["sections"][0]["panels"].append(other)
        # The grid of the copy must not overlap: not checked here, the converter does not validate.
        g = to_grafana.convert(doc)
        p = [x for x in panels(g) if x["id"] == 999][0]
        self.assertEqual(p["datasource"]["uid"], "-- Dashboard --")
        self.assertEqual(p["targets"][0]["panelId"], src["id"])
        self.assertTrue(p["targets"][0]["withTransforms"])

    def test_transforms(self):
        out = to_grafana.transforms_out(
            [
                {
                    "kind": "filter_by_value",
                    "mode": "exclude",
                    "match": "all",
                    "filters": [
                        {"field": "a", "matcher": {"kind": "range", "from": 1, "to": 2}},
                        {"field": "b", "matcher": {"kind": "is_null"}},
                        {"field": "c", "matcher": {"kind": "regex", "pattern": "x"}},
                    ],
                },
                {
                    "kind": "grouping_to_matrix",
                    "column_field": "c",
                    "row_field": "r",
                    "value_field": "v",
                    "empty_value": "null",
                },
            ]
        )
        self.assertEqual(out[0]["options"]["filters"][0]["config"], {"id": "between", "options": {"from": 1, "to": 2}})
        self.assertEqual(out[0]["options"]["filters"][2]["config"]["options"], {"value": "x"})
        self.assertEqual(out[1]["id"], "groupingToMatrix")

    def test_more_transforms(self):
        """The eleven kinds of 4.25a give the Grafana ids and options of 15.6."""
        fixture = os.path.join(ROOT, "tests", "fixtures", "dashboards", "transforms_more.json")
        doc = load(fixture)
        panels_in = doc["sections"][0]["panels"]
        out = to_grafana.transforms_out(panels_in[0]["transforms"])
        self.assertEqual(
            [t["id"] for t in out],
            [
                "organize",
                "calculateField",
                "sortBy",
                "limit",
                "merge",
                "reduce",
                "partitionByValues",
                "joinByField",
                "filterFieldsByName",
                "renameByRegex",
                "convertFieldType",
            ],
        )
        self.assertEqual(
            out[0]["options"],
            {"excludeByName": {"b": True}, "indexByName": {"a": 0, "b": 1, "c": 2}, "renameByName": {"a": "A"}},
        )
        self.assertEqual(
            out[1]["options"],
            {
                "mode": "binary",
                "alias": "sum_ab",
                "replaceFields": False,
                "binary": {"left": "a", "operator": "+", "right": "2.5"},
            },
        )
        self.assertEqual(out[2]["options"], {"sort": [{"field": "a", "desc": True}]})
        self.assertEqual(out[3]["options"], {"limitField": 0})
        self.assertEqual(out[5]["options"], {"mode": "seriesToRows", "reducers": ["max", "mean"]})
        self.assertEqual(out[7]["options"], {"mode": "outer"})
        self.assertEqual(
            out[8]["options"],
            {"include": {"names": ["a", "b"]}, "exclude": {"pattern": "^tmp_"}},
        )
        self.assertEqual(out[9]["options"], {"regex": "(.*)_total", "renamePattern": "$1"})
        self.assertEqual(out[10]["options"], {"conversions": [{"targetField": "a", "destinationType": "number"}]})
        # The other modes, the last rows and the integer operands.
        modes = to_grafana.transforms_out(panels_in[1]["transforms"])
        self.assertEqual(
            modes[0]["options"],
            {"mode": "reduceRow", "replaceFields": True, "reduce": {"reducer": "sum", "include": ["a", "b"]}},
        )
        self.assertEqual(modes[1]["options"]["unary"], {"operator": "abs", "fieldName": "a"})
        self.assertEqual(modes[2]["options"]["index"], {"asPercentile": True})
        self.assertEqual(modes[4]["options"]["binary"]["left"], "-1")
        bounds = to_grafana.transforms_out(panels_in[2]["transforms"])
        self.assertEqual(bounds[3]["options"], {"limitField": -1000000})
        self.assertEqual(len(bounds[5]["options"]["reducers"]), 18)
        # The whole document converts.
        g = to_grafana.convert(doc)
        self.assertEqual(len(panels(g)), 3)

    def test_json_is_stable(self):
        a = to_grafana.dump(to_grafana.convert(self.doc))
        b = to_grafana.dump(to_grafana.convert(load(FIXTURE)))
        self.assertEqual(a, b)

    def test_volume_layout(self):
        with tempfile.TemporaryDirectory() as tmp:
            vol = os.path.join(tmp, "vol", "f1")
            os.makedirs(vol)
            with open(os.path.join(vol, "_folder.json"), "w") as f:
                json.dump({"format_version": 1, "title": "Folder One"}, f)
            with open(os.path.join(vol, "all-types.json"), "w") as f:
                json.dump(self.doc, f)
            out = os.path.join(tmp, "out")
            written = to_grafana.convert_volume(os.path.join(tmp, "vol"), out, set(), {})
            self.assertEqual(written, [os.path.join(out, "Folder One", "all-types.json")])
            self.assertEqual(load(written[0])["uid"], "all-types")
            self.assertEqual(to_grafana.convert_volume(os.path.join(tmp, "vol"), out, {"all-types"}, {}), [])


class VariableKinds(unittest.TestCase):
    """The variable kinds `interval` and `constant` (F27, 4.10a, 4.10b, 15.6)."""

    def interval(self, **extra):
        var = {"kind": "interval", "name": "iv", "label": None, "hide": "none", "intervals": ["1m", "10m", "1h"],
               "auto": True, "auto_count": 30, "auto_min": "10s", "default": None}
        var.update(extra)
        return var

    def test_an_interval_variable_has_the_members_of_grafana(self):
        out = to_grafana.variable_out(self.interval(default="$__auto"), lambda s: s)
        self.assertEqual(out["type"], "interval")
        self.assertEqual(out["query"], "1m,10m,1h")
        self.assertEqual((out["auto"], out["auto_count"], out["auto_min"]), (True, 30, "10s"))
        self.assertEqual(out["current"]["value"], "$__auto")
        self.assertEqual([o["value"] for o in out["options"]], ["$__auto", "1m", "10m", "1h"])
        self.assertEqual([o["selected"] for o in out["options"]], [True, False, False, False])
        self.assertEqual(out["hide"], 0)
        self.assertNotIn("multi", out)

    def test_the_option_auto_is_written_only_with_auto_and_a_null_default_has_no_current(self):
        out = to_grafana.variable_out(self.interval(auto=False, default="10m"), lambda s: s)
        self.assertEqual([o["value"] for o in out["options"]], ["1m", "10m", "1h"])
        self.assertEqual(out["current"], {"selected": True, "text": "10m", "value": "10m"})
        self.assertFalse(out["auto"])
        none = to_grafana.variable_out(self.interval(), lambda s: s)
        self.assertEqual(none["current"], {})

    def test_a_constant_is_hidden_and_holds_its_value_in_query(self):
        var = {"kind": "constant", "name": "k", "label": "A note", "hide": "variable", "value": "a'b\"c$d"}
        out = to_grafana.variable_out(var, lambda s: s)
        self.assertEqual((out["type"], out["query"], out["hide"], out["label"]), ("constant", "a'b\"c$d", 2, "A note"))
        self.assertNotIn("current", out)

    def test_the_fixture_converts_to_the_grafana_fixture(self):
        out = to_grafana.dump(to_grafana.convert(load(VARIABLE_KINDS)))
        with open(VARIABLE_KINDS_GRAFANA, encoding="utf-8") as f:
            self.assertEqual(f.read(), out)
        kinds = [v["type"] for v in json.loads(out)["templating"]["list"]]
        self.assertEqual(kinds, ["interval", "interval", "constant", "constant"])

    def test_the_placement_of_an_interval_is_dropped_with_a_warning(self):
        warnings = []
        g = to_grafana.convert(load(VARIABLE_KINDS), warnings=warnings)
        self.assertEqual(len(warnings), 1)
        self.assertTrue(all("placement" not in v for v in g["templating"]["list"]))


HCL = """
server {
  listen                  = "127.0.0.1:2"
  ops_listen              = "127.0.0.1:1"
  base_path               = "/"
  editing                 = true
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "30s"
  max_request_bytes       = 8388608
  max_result_memory_bytes = 536870912
}
log { level = "info" }
telemetry { enabled = false }
storage {
  backend = "volume"
  volume {
    path            = "%(volume)s"
    read_only       = false
    rescan_interval = "1s"
    history_keep    = 100
  }
}
access { mode = "open" }
clickhouse "prod" {
  label              = "Production"
  url                = "http://127.0.0.1:1"
  user               = "hune"
  password_file      = "%(password)s"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "1m"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "5s"
}
"""

HCL_CONF = """
clickhouse "conf" {
  label              = "Conformance"
  url                = "http://127.0.0.1:1"
  user               = "hune"
  password_file      = "%(password)s"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "1m"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "5s"
}
"""

HCL_POSTGRES = """
postgres "analytics" {
  label                = "Analytics"
  host                 = "127.0.0.1"
  port                 = 1
  database             = "analytics"
  user                 = "hune_ro"
  password_file        = "%(password)s"
  search_path          = "metrics"
  sslmode              = "disable"
  connect_timeout      = "5s"
  max_execution_time   = "1m"
  max_result_rows      = 100000
  max_result_bytes     = 33554432
  max_query_size       = 1048576
  max_concurrent       = 16
  queue_wait           = "5s"
}
"""

# The members of the report that the known losses explain (to_grafana.KNOWN_LOSSES). The form of the xychart and
# the log scale of Grafana 12.4.2 have no loss now: the importer reads them back.
KNOWN_MEMBERS = (
    "/annotations/list/",
    "transforms-more:/panels/0/transformations/10/options/conversions",
    "transforms-more:/panels/2/transformations/10/options/conversions",
    "transforms-more:/panels/0/transformations/6",
    "transforms-more:/panels/2/transformations/6",
)


def diff(a, b, path=""):
    if type(a) is not type(b):
        return [path]
    if isinstance(a, dict):
        out = []
        for k in sorted(set(a) | set(b)):
            out += diff(a.get(k, "<absent>"), b.get(k, "<absent>"), path + "/" + k)
        return out
    if isinstance(a, list):
        if len(a) != len(b):
            return [path]
        return [d for i, (x, y) in enumerate(zip(a, b)) for d in diff(x, y, "%s/%d" % (path, i))]
    return [] if a == b else [path]


def run_round_trip(case, path, postgres=False, conf=False):
    """Converts a fixture, imports the result and returns (expected, back, grafana).

    With `postgres`, the source `analytics` is a PostgreSQL source: the converter writes the type of the
    PostgreSQL plugin for it, and the configuration has a block `postgres "analytics"`. With `conf`, the
    second ClickHouse source `conf` has the Grafana data source `conf-ds` and a block in the configuration.
    """
    doc = load(path)
    uid = doc["uid"]
    if postgres:
        grafana = to_grafana.convert(doc, {"analytics": "pg"}, {"analytics": "postgres"})
    elif conf:
        grafana = to_grafana.convert(doc, {"conf": "conf-ds"})
    else:
        grafana = to_grafana.convert(doc)
    with tempfile.TemporaryDirectory() as tmp:
        inp = os.path.join(tmp, "in")
        os.makedirs(os.path.join(inp, "dashboards"))
        vol = os.path.join(tmp, "volume")
        os.makedirs(vol)
        with open(os.path.join(inp, "dashboards", uid + ".json"), "w") as f:
            f.write(to_grafana.dump(grafana))
        for name, value in (
            ("folders.json", []),
            ("search.json", [{"id": 1, "uid": uid, "title": doc["title"], "type": "dash-db"}]),
            ("health.json", {"commit": "0", "database": "ok", "version": "12.4.2"}),
            (
                "plugin-clickhouse.json",
                {
                    "name": "ClickHouse",
                    "type": "datasource",
                    "id": "grafana-clickhouse-datasource",
                    "info": {"version": "4.5.1"},
                    "enabled": True,
                },
            ),
        ):
            with open(os.path.join(inp, name), "w") as f:
                json.dump(value, f)
        password = os.path.join(tmp, "password")
        with open(password, "w") as f:
            f.write("x")
        config = os.path.join(tmp, "hune.hcl")
        with open(config, "w") as f:
            f.write((HCL + (HCL_POSTGRES if postgres else "") + (HCL_CONF if conf else "")) % {"volume": vol, "password": password})
        base = [BINARY, "import-grafana", "--in", inp, "--source-map", "clickhouse=prod", "-c", config]
        if postgres:
            base[-2:-2] = ["--source-map", "pg=analytics"]
        if conf:
            base[-2:-2] = ["--source-map", "conf-ds=conf"]
        report = os.path.join(tmp, "report")
        dry = subprocess.run(base + ["--report", report], capture_output=True, text=True)
        case.assertTrue(os.path.exists(report + ".json"), dry.stdout + dry.stderr)
        with open(report + ".json") as f:
            items = [i for d in json.load(f)["dashboards"] for i in d["items"] if i["class"] != "mapped"]

        def hide_property(item_id):
            """True when the item names the value of a table property that Grafana 12.4.2 calls custom.hideFrom.viz."""
            pointer = item_id.split(":", 1)[1]
            if "/properties/" not in pointer or not pointer.endswith("/value"):
                return False
            node = grafana
            for part in pointer[: -len("/value")].split("/")[1:]:
                node = node[int(part)] if isinstance(node, list) else node[part]
            return node["id"] == "custom.hideFrom.viz"

        unknown = [i["id"] for i in items if not (any(k in i["id"] for k in KNOWN_MEMBERS) or hide_property(i["id"]))]
        case.assertEqual(unknown, [], dry.stdout)
        accept = os.path.join(tmp, "accept.txt")
        with open(accept, "w") as f:
            f.write("".join(i["id"] + "   # known loss of the converter\n" for i in items))
        run = subprocess.run(base + ["--accept", accept, "--write"], capture_output=True, text=True)
        case.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        found = [
            os.path.join(d, n) for d, _, files in os.walk(vol) for n in files if n == uid + ".json" and ".history" not in d
        ]
        case.assertEqual(len(found), 1, found)
        back = load(found[0])
    expected = copy.deepcopy(doc)
    for x in (expected, back):
        for s in x["sections"]:
            for p in s["panels"]:
                # The importer gives the flag viz of custom.hideFrom as `hidden`, and drops the three flags of a
                # table column: the properties hidden and hide_from are not compared (KNOWN_LOSSES).
                if p["type"] == "table":
                    for o in p["fields"]["overrides"]:
                        o["set"].pop("hidden", None)
                        o["set"].pop("hide_from", None)
                    p["fields"]["defaults"].pop("hidden", None)
                    p["fields"]["defaults"].pop("hide_from", None)
        # The text variables: the importer reads value_type from the use in code (15.6).
        for v in x["variables"]:
            if v["kind"] == "text":
                v["value_type"] = "-"
    return expected, back, items


@unittest.skipUnless(os.access(BINARY, os.X_OK), "the debug binary target/debug/hune is absent")
class RoundTrip(unittest.TestCase):
    def test_all_types(self):
        expected, back, _ = run_round_trip(self, FIXTURE)
        self.assertEqual(diff(expected, back), [])

    def test_f_members(self):
        expected, back, _ = run_round_trip(self, F_MEMBERS)
        self.assertEqual(diff(expected, back), [])

    def test_variable_kinds(self):
        """The importer reads back an interval and a constant that the converter writes (15.6)."""
        expected, back, items = run_round_trip(self, VARIABLE_KINDS)
        # The placement of a variable has no form in Grafana (KNOWN_LOSSES).
        for v in expected["variables"]:
            v.pop("placement", None)
        self.assertEqual(diff(expected, back), [])
        self.assertEqual(items, [])

    def test_postgres_source(self):
        expected, back, _ = run_round_trip(self, PG_ROUND_TRIP, postgres=True)
        self.assertEqual(diff(expected, back), [])

    def test_repeats(self):
        """The importer reads back the members of the repeat that the converter writes (15.6)."""
        for path in [REPEATS] + PARITY_REPEAT:
            expected, back, items = run_round_trip(self, path)
            self.assertEqual(diff(expected, back), [], path)
            self.assertEqual(items, [], path)

    def test_timeline_panels(self):
        """The importer reads back the two panels that the converter writes (4.33b, 15.6): the same members."""
        for path in (STATE_TIMELINE, STATUS_HISTORY):
            expected, back, items = run_round_trip(self, path)
            self.assertEqual(diff(expected, back), [], path)
            self.assertEqual(items, [], path)

    def test_bucket_panels(self):
        """The importer reads back the two panels that the converter writes (4.33c, 4.33d, 15.6): the same members."""
        for path in (HEATMAP, HISTOGRAM):
            expected, back, items = run_round_trip(self, path)
            self.assertEqual(diff(expected, back), [], path)
            self.assertEqual(items, [], path)

    def test_more_transforms(self):
        """The importer reads back the eleven kinds that the converter writes (4.25a, 15.6): the same members.

        It lists one item with the class `changed` for each `partition_by_values` and each `convert_field_type`: the
        rules of their values differ from Grafana (19.20j). Nothing else.
        """
        expected, back, items = run_round_trip(self, TRANSFORMS_MORE)
        self.assertEqual(diff(expected, back), [])
        self.assertEqual([i["class"] for i in items], ["changed"] * 4)
        self.assertEqual(sum(i["id"].endswith("/options/conversions") for i in items), 2)
        self.assertEqual(sum(i["id"].endswith("/transformations/6") for i in items), 2)

    def test_text_panels(self):
        """The importer reads back the text panel that the converter writes (4.33a, 15.6): the same members, no item."""
        for path in [TEXT_PANEL] + PARITY_TEXT:
            expected, back, items = run_round_trip(self, path)
            self.assertEqual(diff(expected, back), [], path)
            self.assertEqual(items, [], path)


    def test_annotations(self):
        """The importer reads back the annotations that the converter writes (4.5b, 15.6): the same members.

        It lists two items for each annotation: `hide` equal to `false` and the warning that it
        runs no query on the SQL (7.37). Nothing else.
        """
        # The fixture `annotations.json` is not here: its `panels` names a stat and a table, which the importer
        # removes from the list (15.6).
        for path in PARITY_ANNOTATIONS:
            expected, back, items = run_round_trip(self, path)
            self.assertEqual(diff(expected, back), [], path)
            kinds = sorted({i["id"].rsplit("/", 1)[1] for i in items})
            self.assertEqual(kinds, ["hide", "rawSql"], path)
            self.assertEqual(len(items), 2 * len(expected["annotations"]), path)


    def test_several_queries(self):
        """The importer reads back the targets that the converter writes (4.15a, 15.6): the same members, no item.

        The fixture has a panel with 3 queries on two sources (the data source `-- Mixed --`), a table with two
        sources, a hidden query, a first query with a ref and `hide`, and the matcher `query`.
        """
        for path in [QUERIES] + PARITY_QUERIES:
            expected, back, items = run_round_trip(self, path, conf=True)
            self.assertEqual(diff(expected, back), [], path)
            self.assertEqual(items, [], path)


class MultiQuery(unittest.TestCase):
    """The converter writes one target for each query (4.15a, 15.14c)."""

    def setUp(self):
        self.doc = load(QUERIES)
        self.g = to_grafana.convert(self.doc, {"conf": "conf-ds"})
        self.p = {p["id"]: p for p in panels(self.g)}

    def refs(self, pid):
        return [t["refId"] for t in self.p[pid]["targets"]]

    def test_one_target_for_each_query_in_the_order_of_the_queries(self):
        self.assertEqual(self.refs(1), ["A", "B", "C"])
        self.assertEqual(self.refs(2), ["A", "B"])
        self.assertEqual(self.refs(3), ["A", "B"])
        self.assertEqual(self.refs(4), ["C", "A"], "the first query has the ref C: it stays first")
        self.assertIn("avg(v) AS mem", self.p[1]["targets"][1]["rawSql"])

    def test_hide_is_written_for_true_only(self):
        self.assertEqual([t.get("hide") for t in self.p[3]["targets"]], [None, True])
        self.assertEqual([t.get("hide") for t in self.p[4]["targets"]], [True, None])
        self.assertEqual([t.get("hide") for t in self.p[1]["targets"]], [None, None, None])

    def test_a_panel_with_two_sources_has_the_data_source_mixed(self):
        mixed = {"type": "datasource", "uid": "-- Mixed --"}
        self.assertEqual(self.p[1]["datasource"], mixed)
        self.assertEqual(self.p[2]["datasource"], mixed)
        uids = [t["datasource"]["uid"] for t in self.p[1]["targets"]]
        self.assertEqual(uids, ["clickhouse", "clickhouse", "conf-ds"], "each target names its own data source")
        # A panel with one source keeps the data source of that source.
        doc = copy.deepcopy(self.doc)
        doc["sections"][0]["panels"][0]["data"]["extra_queries"][1]["source"] = "prod"
        g = to_grafana.convert(doc)
        self.assertEqual(panels(g)[0]["datasource"]["uid"], "clickhouse")

    def test_the_panel_members_stay_on_the_panel(self):
        self.assertEqual(self.p[1]["interval"], "1m")
        self.assertNotIn("interval", self.p[1]["targets"][1])

    def test_the_matcher_query_is_by_frame_ref_id(self):
        o = self.p[1]["fieldConfig"]["overrides"][0]
        self.assertEqual(o["matcher"], {"id": "byFrameRefID", "options": "C"})

    def test_a_panel_with_one_query_has_one_target_and_the_ref_a(self):
        g = to_grafana.convert(load(FIXTURE))
        for p in panels(g):
            if p["type"] != "stat" or p["targets"][0].get("panelId") is None:
                self.assertEqual(len(p["targets"]), 1, p["id"])

    def test_the_postgres_kind_writes_its_members_for_each_query(self):
        doc = copy.deepcopy(self.doc)
        doc["sections"][0]["panels"][1]["data"]["extra_queries"][0]["source"] = "analytics"
        g = to_grafana.convert(doc, {"conf": "conf-ds", "analytics": "pg"}, {"analytics": "postgres"})
        t = panels(g)[1]["targets"][1]
        self.assertEqual((t["editorMode"], t["rawQuery"], t["format"]), ("code", True, "table"))
        self.assertEqual(panels(g)[1]["datasource"]["uid"], "-- Mixed --")

    def test_the_parity_fixtures_convert_to_the_grafana_fixtures(self):
        for doc_path, grafana_path in zip(PARITY_QUERIES, GRAFANA_QUERIES):
            with open(grafana_path, encoding="utf-8") as f:
                converted = to_grafana.dump(to_grafana.convert(load(doc_path), {"conf": "conf-ds"}))
                self.assertEqual(f.read(), converted, grafana_path)

    def test_the_six_parity_fixtures_hold_the_cases_of_15_14c(self):
        docs = [load(p) for p in PARITY_QUERIES]
        queries = [to_grafana.queries_of(d["sections"][0]["panels"][0]["data"]) for d in docs]
        self.assertEqual([len(q) for q in queries], [3, 2, 2, 2, 2, 2])
        self.assertEqual([q["hide"] for q in queries[2]], [False, True], "a stat with a hidden query")
        self.assertEqual([q["hide"] for q in queries[5]], [True, True], "a panel with all queries hidden")
        self.assertEqual({q["source"] for q in queries[3]}, {"prod", "conf"}, "a panel with Mixed sources")
        self.assertEqual([d["sections"][0]["panels"][0]["type"] for d in docs][:2], ["timeseries", "table"])


class FMembers(unittest.TestCase):
    """The members that milestone F added, written in the form of Grafana 12.4.2."""

    def setUp(self):
        self.doc = load(F_MEMBERS)
        self.g = to_grafana.convert(self.doc)
        self.by_id = {p["id"]: p for p in panels(self.g)}

    def test_stat_justify(self):
        self.assertEqual(self.by_id[30]["options"]["justifyMode"], "center")

    def test_table_wrap_and_footer_and_cells(self):
        t = self.by_id[10]
        self.assertTrue(t["fieldConfig"]["defaults"]["custom"]["wrapText"])
        self.assertEqual(t["fieldConfig"]["defaults"]["custom"]["footer"], {"reducers": ["sum", "count", "stdDev"]})
        props = {o["matcher"]["options"] if isinstance(o["matcher"]["options"], str) else "names": o for o in t["fieldConfig"]["overrides"]}
        self.assertEqual(
            {p["id"]: p["value"] for p in props["size"]["properties"]}["custom.cellOptions"], {"type": "gauge", "mode": "lcd"}
        )
        names = props["names"]
        self.assertEqual(names["matcher"], {"id": "byNames", "options": {"mode": "include", "names": ["payload", "extra"]}})
        values = {p["id"]: p["value"] for p in names["properties"]}
        self.assertEqual(values["custom.cellOptions"], {"type": "json-view"})
        self.assertEqual(values["custom.footer.reducers"], ["diffperc"])

    def test_hide_from(self):
        ts = self.by_id[20]
        first = ts["fieldConfig"]["overrides"][0]
        self.assertEqual(first["matcher"]["options"], {"mode": "exclude", "names": ["FRA p99", "FRA p50"]})
        self.assertEqual(
            first["properties"], [{"id": "custom.hideFrom", "value": {"legend": False, "tooltip": True, "viz": True}}]
        )
        table = self.by_id[10]["fieldConfig"]["defaults"]["custom"]["hideFrom"]
        self.assertEqual(table, {"legend": False, "tooltip": False, "viz": False})

    def test_timeseries_and_barchart(self):
        ts = self.by_id[20]
        self.assertEqual(ts["fieldConfig"]["defaults"]["custom"]["thresholdsStyle"], {"mode": "dashed+area"})
        self.assertEqual(ts["fieldConfig"]["defaults"]["custom"]["scaleDistribution"], {"type": "log", "log": 10})
        self.assertEqual(
            ts["options"]["legend"],
            {
                "showLegend": True,
                "displayMode": "table",
                "placement": "bottom",
                "calcs": ["max", "median", "lastNotNull", "range"],
                "sortBy": "Max",
                "sortDesc": True,
            },
        )
        self.assertEqual(ts["options"]["tooltip"], {"mode": "multi", "sort": "desc", "hideZeros": True})
        bar = self.by_id[40]
        self.assertEqual(bar["fieldConfig"]["defaults"]["custom"]["thresholdsStyle"], {"mode": "line"})
        self.assertEqual(bar["options"]["legend"]["calcs"], ["changeCount", "distinctCount"])
        self.assertEqual(bar["options"]["legend"]["sortBy"], "Name")
        self.assertEqual(bar["options"]["tooltip"], {"mode": "multi", "sort": "asc"})

    def test_pie_xy_and_bargauge_calcs(self):
        pie = self.by_id[50]
        self.assertEqual(pie["options"]["reduceOptions"]["calcs"], ["variance"])
        self.assertEqual((pie["options"]["legend"]["sortBy"], pie["options"]["legend"]["sortDesc"]), ("Percent", True))
        xy = self.by_id[60]
        self.assertEqual(xy["options"]["legend"]["calcs"], ["delta", "first", "firstNotNull", "count", "mean"])
        self.assertEqual(xy["options"]["mapping"], "manual")
        self.assertEqual([s["y"]["matcher"]["options"] for s in xy["options"]["series"]], ["y1", "y2"])
        self.assertEqual(xy["fieldConfig"]["defaults"]["custom"]["show"], "points+lines")
        self.assertEqual(self.by_id[70]["options"]["reduceOptions"]["calcs"], ["diffperc"])

    def test_new_units_keep_their_grafana_ids(self):
        units = {p["id"]: p["fieldConfig"]["defaults"].get("unit") for p in panels(self.g)}
        self.assertEqual(units[20], "ns")
        self.assertEqual(units[30], "bool_yes_no")
        self.assertEqual(units[40], "currencyUSD")
        self.assertEqual(units[50], "locale")
        self.assertEqual(units[60], "iops")
        self.assertEqual(units[70], "dateTimeAsIso")

    def test_default_members_are_not_written(self):
        # A document with no F member gives no F member (the base fixture).
        g = to_grafana.convert(load(FIXTURE))
        text = to_grafana.dump(g)
        for member in ("justifyMode", "sortDesc", "hideZeros", "wrapText", "byNames"):
            self.assertNotIn(member, text)


class VariablePlacement(unittest.TestCase):
    """The member placement of a variable (4.7a) has no Grafana form: the converter drops it and warns."""

    def setUp(self):
        self.doc = load(FIXTURE)
        self.doc["variables"][1]["placement"] = {"section": 0, "x": 0, "y": 40, "w": 6, "h": 2}
        self.doc["variables"][2]["placement"] = {"section": 0, "x": 6, "y": 40, "w": 6, "h": 2, "title": "Pick"}
        self.warnings = []
        self.g = to_grafana.convert(self.doc, warnings=self.warnings)

    def test_each_variable_stays_and_none_has_a_placement(self):
        self.assertEqual([v["name"] for v in self.g["templating"]["list"]], [v["name"] for v in self.doc["variables"]])
        self.assertTrue(all("placement" not in v for v in self.g["templating"]["list"]))

    def test_the_panels_do_not_change(self):
        plain = to_grafana.convert(load(FIXTURE))
        self.assertEqual(self.g, plain)

    def test_one_warning_for_each_placement(self):
        name = self.doc["variables"][1]["name"]
        self.assertEqual(len(self.warnings), 2)
        self.assertIn("the placement of the variable %s is dropped" % name, self.warnings[0])

    def test_no_warning_without_a_list_and_none_for_a_document_without_placement(self):
        to_grafana.convert(self.doc)
        warnings = []
        to_grafana.convert(load(FIXTURE), warnings=warnings)
        self.assertEqual(warnings, [])

    def test_the_command_line_writes_the_warnings_to_the_standard_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = os.path.join(tmp, "in.json")
            out = os.path.join(tmp, "out.json")
            with open(src, "w") as f:
                json.dump(self.doc, f)
            r = subprocess.run(
                [sys.executable, "-I", os.path.join(ROOT, "tools", "parity", "to_grafana.py"), src, out],
                capture_output=True,
                text=True,
            )
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(r.stdout, "")
            self.assertEqual(r.stderr.count("warning: the placement"), 2, r.stderr)
            self.assertEqual(load(out), self.g)

    def test_the_volume_form_prefixes_the_uid(self):
        with tempfile.TemporaryDirectory() as tmp:
            vol = os.path.join(tmp, "vol", "f1")
            os.makedirs(vol)
            with open(os.path.join(vol, "_folder.json"), "w") as f:
                json.dump({"format_version": 1, "title": "Folder One"}, f)
            with open(os.path.join(vol, "all-types.json"), "w") as f:
                json.dump(self.doc, f)
            warnings = []
            to_grafana.convert_volume(os.path.join(tmp, "vol"), os.path.join(tmp, "out"), set(), {}, None, warnings)
            self.assertEqual(len(warnings), 2)
            self.assertTrue(all(w.startswith("all-types: the placement") for w in warnings), warnings)


class Repeat(unittest.TestCase):
    """The repeat of a panel and of a section (4.13a, 15.6): the members of the v1 form."""

    def setUp(self):
        self.doc = load(REPEATS)
        self.g = to_grafana.convert(self.doc)

    def by_id(self, pid):
        return {p["id"]: p for p in panels(self.g)}[pid]

    def test_a_panel_with_h_writes_repeat_direction_and_max_per_row(self):
        p = self.by_id(1)
        self.assertEqual((p["repeat"], p["repeatDirection"], p["maxPerRow"]), ("host", "h", 3))

    def test_a_panel_with_v_writes_no_max_per_row(self):
        p = self.by_id(2)
        self.assertEqual((p["repeat"], p["repeatDirection"]), ("region", "v"))
        self.assertNotIn("maxPerRow", p)

    def test_an_absent_max_per_row_is_not_written_and_the_default_is_not_added(self):
        p = self.by_id(6)
        self.assertEqual((p["repeat"], p["repeatDirection"]), ("host", "h"))
        self.assertNotIn("maxPerRow", p)

    def test_a_section_with_a_repeat_writes_the_repeat_of_its_row(self):
        rows = [p for p in self.g["panels"] if p["type"] == "row"]
        self.assertEqual([r.get("repeat") for r in rows], ["host", None])
        self.assertEqual([r["title"] for r in rows], ["Host ${host}", "Other"])

    def test_a_panel_with_no_repeat_has_no_member_of_the_repeat(self):
        for pid in (3, 4, 5, 7, 8):
            p = self.by_id(pid)
            for key in ("repeat", "repeatDirection", "maxPerRow"):
                self.assertNotIn(key, p, (pid, key))

    def test_a_document_with_no_repeat_gives_no_member_of_the_repeat(self):
        g = to_grafana.convert(load(FIXTURE))
        text = json.dumps(g)
        for key in ("repeat", "repeatDirection", "maxPerRow"):
            self.assertNotIn('"%s"' % key, text)

    def test_the_three_parity_fixtures_convert(self):
        for path in PARITY_REPEAT:
            g = to_grafana.convert(load(path))
            # The row of a repeated section holds the repeat. Only the other two fixtures have a panel with one.
            repeating = [p for p in panels(g) if "repeat" in p]
            self.assertEqual(len(repeating), 0 if path.endswith("section.json") else 1, path)
        h = to_grafana.convert(load(PARITY_REPEAT[0]))
        self.assertEqual([(p["repeat"], p["repeatDirection"], p["maxPerRow"]) for p in h["panels"] if "repeat" in p], [("host", "h", 3)])
        section = to_grafana.convert(load(PARITY_REPEAT[2]))
        self.assertEqual([r["repeat"] for r in section["panels"] if r["type"] == "row" and "repeat" in r], ["host"])


class TimelinePanels(unittest.TestCase):
    """The state timeline and the status history (4.33b, 15.6, 15.14c): the members of Grafana 12.4.2."""

    def setUp(self):
        self.timeline = load(STATE_TIMELINE)
        self.history = load(STATUS_HISTORY)
        self.t = {p["id"]: p for p in panels(to_grafana.convert(self.timeline))}
        self.h = {p["id"]: p for p in panels(to_grafana.convert(self.history))}

    def test_the_two_types_have_the_ids_of_grafana(self):
        self.assertEqual({p["type"] for p in self.t.values()}, {"state-timeline"})
        self.assertEqual({p["type"] for p in self.h.values()}, {"status-history"})

    def test_a_state_timeline_writes_its_options_and_its_custom_members(self):
        p = self.t[1]
        self.assertEqual(
            p["options"],
            {
                "showValue": "auto",
                "rowHeight": 0.9,
                "legend": {"showLegend": True, "displayMode": "list", "placement": "bottom", "calcs": []},
                "tooltip": {"mode": "single", "sort": "none"},
                "mergeValues": True,
                "alignValue": "left",
            },
        )
        self.assertEqual(
            p["fieldConfig"]["defaults"]["custom"], {"lineWidth": 0, "fillOpacity": 70, "spanNulls": False}
        )
        self.assertEqual(
            [m["type"] for m in p["fieldConfig"]["defaults"]["mappings"]], ["value", "value", "range", "regex", "special"]
        )

    def test_the_boundaries_write_the_fractions_and_the_hidden_axis(self):
        low, high = self.t[2], self.t[3]
        self.assertEqual((low["options"]["rowHeight"], low["options"]["mergeValues"], low["options"]["alignValue"]), (0.1, False, "center"))
        self.assertEqual(low["options"]["tooltip"], {"mode": "multi", "sort": "none"})
        self.assertEqual(high["options"]["tooltip"], {"mode": "none", "sort": "none"})
        self.assertEqual(low["options"]["legend"], {"showLegend": False, "displayMode": "list", "placement": "right", "calcs": []})
        self.assertEqual(low["fieldConfig"]["defaults"]["custom"]["axisPlacement"], "hidden")
        self.assertEqual((high["options"]["rowHeight"], high["options"]["showValue"], high["options"]["alignValue"]), (1.0, "always", "right"))
        self.assertEqual(
            high["fieldConfig"]["defaults"]["custom"], {"lineWidth": 10, "fillOpacity": 100, "spanNulls": False}
        )
        self.assertNotIn("axisPlacement", self.t[1]["fieldConfig"]["defaults"]["custom"])

    def test_a_status_history_writes_the_col_width_and_no_merge_member(self):
        p, narrow, wide = self.h[1], self.h[2], self.h[3]
        self.assertEqual((p["options"]["rowHeight"], p["options"]["colWidth"]), (0.9, 0.9))
        self.assertEqual((narrow["options"]["colWidth"], wide["options"]["colWidth"]), (0.1, 1.0))
        for q in (p, narrow, wide):
            for key in ("mergeValues", "alignValue"):
                self.assertNotIn(key, q["options"])
            self.assertNotIn("spanNulls", q["fieldConfig"]["defaults"].get("custom", {}))
        self.assertEqual(p["fieldConfig"]["defaults"]["custom"], {"lineWidth": 1, "fillOpacity": 70})
        self.assertEqual(wide["fieldConfig"]["defaults"]["custom"], {"lineWidth": 10, "fillOpacity": 100})
        hide = {"id": "custom.hideFrom", "value": {"legend": True, "tooltip": False, "viz": False}}
        self.assertIn(hide, wide["fieldConfig"]["overrides"][0]["properties"])
        self.assertEqual(narrow["fieldConfig"]["defaults"]["custom"]["axisPlacement"], "hidden")

    def test_the_field_properties_and_the_query_are_written_as_for_a_time_series(self):
        p = self.t[3]
        d = p["fieldConfig"]["defaults"]
        self.assertEqual((d["unit"], d["decimals"], d["color"]), ("percent", 1, {"mode": "thresholds"}))
        self.assertEqual([s["value"] for s in d["thresholds"]["steps"]], [None, 50, 90])
        properties = p["fieldConfig"]["overrides"][0]["properties"]
        self.assertIn({"id": "displayName", "value": "Status"}, properties)
        self.assertIn({"id": "custom.hideFrom", "value": {"legend": False, "tooltip": True, "viz": False}}, properties)
        self.assertEqual(p["targets"][0]["rawSql"], "\n".join(self.timeline["sections"][0]["panels"][2]["data"]["query"]["sql"]))
        self.assertEqual(self.t[4]["targets"][0]["panelId"], 1, "a reuse stays a reuse")
        self.assertEqual(self.t[5]["repeat"], "host")

    def test_the_colour_modes_that_are_gradients_have_the_ids_of_grafana(self):
        for mode, gid in to_grafana.CONTINUOUS.items():
            self.assertEqual(to_grafana.color_out({"mode": mode, "fixed": None}), {"mode": gid})
        self.assertEqual(len(to_grafana.CONTINUOUS), 10)
        self.assertEqual(self.h[5]["fieldConfig"]["defaults"]["color"], {"mode": "continuous-GrYlRd"})

    def test_a_document_with_no_such_panel_does_not_change(self):
        g = to_grafana.convert(load(FIXTURE))
        self.assertNotIn("state-timeline", json.dumps(g))
        self.assertEqual(to_grafana.GRAFANA_TYPE, {"state_timeline": "state-timeline", "status_history": "status-history"})


class BucketPanels(unittest.TestCase):
    """The heatmap and the histogram (4.33c, 4.33d, 15.6, 15.14c): the members of Grafana 12.4.2."""

    def setUp(self):
        self.heatmap = load(HEATMAP)
        self.histogram = load(HISTOGRAM)
        self.h = {p["id"]: p for p in panels(to_grafana.convert(self.heatmap))}
        self.g = {p["id"]: p for p in panels(to_grafana.convert(self.histogram))}

    def test_the_two_types_have_the_ids_of_grafana(self):
        self.assertEqual({p["type"] for p in self.h.values()}, {"heatmap"})
        self.assertEqual({p["type"] for p in self.g.values()}, {"histogram"})
        self.assertNotIn("heatmap", to_grafana.GRAFANA_TYPE)
        self.assertNotIn("histogram", to_grafana.GRAFANA_TYPE)

    def test_a_heatmap_writes_its_options(self):
        p = self.h[1]
        self.assertEqual(
            p["options"],
            {
                "calculate": False,
                "calculation": {"xBuckets": {"mode": "count"}, "yBuckets": {"mode": "count"}},
                "yAxis": {"axisPlacement": "left", "reverse": False},
                "color": {
                    "mode": "scheme",
                    "scheme": "Oranges",
                    "fill": "dark-orange",
                    "scale": "exponential",
                    "exponent": 0.5,
                    "steps": 64,
                    "reverse": False,
                },
                "cellGap": 1,
                "filterValues": {"le": 0, "ge": None},
                "legend": {"show": True},
                "tooltip": {"mode": "single"},
            },
        )
        self.assertEqual(p["fieldConfig"]["defaults"]["unit"], "ms")

    def test_the_boundaries_write_the_texts_of_the_buckets_and_the_scales(self):
        low, high, small, log2, log10 = self.h[2], self.h[3], self.h[4], self.h[5], self.h[6]
        self.assertEqual(low["options"]["calculation"], {"xBuckets": {"mode": "count", "value": "1"}, "yBuckets": {"mode": "count", "value": "1"}})
        self.assertEqual(high["options"]["calculation"]["xBuckets"]["value"], "1000")
        self.assertEqual(small["options"]["calculation"], {"xBuckets": {"mode": "size", "value": "1"}, "yBuckets": {"mode": "size", "value": "0.0001"}})
        self.assertEqual(
            log2["options"]["calculation"],
            {"xBuckets": {"mode": "size", "value": "31536000000"}, "yBuckets": {"mode": "size", "value": "16", "scale": {"type": "log", "log": 2}}},
        )
        self.assertEqual(log10["options"]["calculation"]["yBuckets"], {"mode": "size", "scale": {"type": "log", "log": 10}})
        self.assertEqual((low["options"]["yAxis"], low["options"]["tooltip"]), ({"axisPlacement": "hidden", "reverse": True, "min": -100, "max": 100}, {"mode": "none"}))
        self.assertEqual(low["options"]["color"]["scheme"], "Viridis")
        self.assertEqual((low["options"]["color"]["min"], low["options"]["color"]["max"]), (-5, 5))
        self.assertEqual(low["options"]["filterValues"], {"le": -1.5, "ge": -0.5})
        self.assertEqual(low["options"]["legend"], {"show": False})
        self.assertEqual(high["options"]["tooltip"], {"mode": "multi"})
        self.assertEqual(self.h[8]["repeat"], "host")
        self.assertEqual(self.h[7]["targets"][0]["panelId"], 1, "a reuse stays a reuse")

    def test_a_histogram_writes_its_options_and_its_custom_members(self):
        p = self.g[1]
        self.assertEqual(
            p["options"],
            {
                "bucketCount": 30,
                "bucketOffset": 0,
                "combine": False,
                "legend": {"showLegend": True, "displayMode": "list", "placement": "bottom", "calcs": []},
                "tooltip": {"mode": "single", "sort": "none"},
            },
        )
        self.assertEqual(
            p["fieldConfig"]["defaults"]["custom"],
            {"stacking": {"group": "A", "mode": "none"}, "lineWidth": 1, "fillOpacity": 80},
        )
        low, high, query = self.g[2], self.g[3], self.g[4]
        self.assertEqual((low["options"]["bucketSize"], low["options"]["bucketOffset"], low["options"]["combine"]), (1e-9, -12.5, True))
        self.assertEqual(high["fieldConfig"]["defaults"]["custom"]["stacking"]["mode"], "percent")
        self.assertEqual((query["options"]["bucketSize"], query["options"]["bucketOffset"]), (25, 10))
        self.assertEqual(self.g[6]["repeat"], "host")

    def test_a_document_with_no_such_panel_does_not_change(self):
        g = json.dumps(to_grafana.convert(load(FIXTURE)))
        self.assertNotIn("heatmap", g)
        self.assertNotIn("histogram", g)

    def test_the_text_of_a_bucket_value(self):
        for value, text in ((1, "1"), (1.0, "1"), (0.5, "0.5"), (31536000000, "31536000000"), (0.0001, "0.0001")):
            self.assertEqual(to_grafana._number_text(value), text)


class TextPanel(unittest.TestCase):
    """The text panel (4.33a, 15.6, 15.14c): the members of the form of Grafana 12.4.2."""

    def setUp(self):
        self.doc = load(TEXT_PANEL)
        self.g = to_grafana.convert(self.doc)

    def text_panels(self):
        return [p for p in panels(self.g) if p["type"] == "text"]

    def test_a_text_panel_writes_the_mode_the_joined_content_and_the_default_code(self):
        p = {x["id"]: x for x in self.text_panels()}[7]
        self.assertEqual(p["options"]["mode"], "markdown")
        self.assertEqual(
            p["options"]["content"], "# Runbook\n\nRegion: $region. See [the wiki](https://wiki.example/runbook)."
        )
        self.assertEqual(p["options"]["code"], {"language": "plaintext", "showLineNumbers": False, "showMiniMap": False})
        self.assertEqual(sorted(p["options"]), ["code", "content", "mode"])

    def test_a_text_panel_has_no_query_and_no_field_member(self):
        for p in self.text_panels():
            for key in ("targets", "datasource", "transformations", "interval", "maxDataPoints", "repeat"):
                self.assertNotIn(key, p, (p["id"], key))
            self.assertEqual(p["fieldConfig"], {"defaults": {}, "overrides": []})

    def test_an_empty_content_gives_an_empty_text_and_the_lines_keep_their_blanks(self):
        by_id = {x["id"]: x for x in self.text_panels()}
        self.assertEqual(by_id[8]["options"]["content"], "")
        content = by_id[10]["options"]["content"]
        lines = self.doc["sections"][1]["panels"][2]["options"]["content"]
        self.assertEqual(content, "\n".join(lines))
        self.assertEqual(content.split("\n"), lines)
        self.assertEqual(by_id[9]["options"]["content"].split("\n")[0], "Tab:\there")

    def test_a_text_panel_keeps_the_common_members(self):
        by_id = {x["id"]: x for x in self.text_panels()}
        p = by_id[9]
        self.assertEqual((p["title"], p["transparent"]), ("Characters", True))
        self.assertEqual(p["description"], "A tab, U+2028 and a character outside the plane 0.")
        self.assertEqual(p["links"], [])
        self.assertEqual(p["pluginVersion"], to_grafana.GRAFANA_VERSION)
        # The grid is absolute below the row of the section.
        self.assertEqual(p["gridPos"], {"h": 4, "w": 12, "x": 12, "y": 10})
        doc = copy.deepcopy(self.doc)
        doc["sections"][0]["panels"][1]["links"] = [{"title": "Wiki", "url": "https://wiki.example", "target_blank": True}]
        g = to_grafana.convert(doc)
        link = {x["id"]: x for x in panels(g)}[7]["links"]
        self.assertEqual(link, [{"title": "Wiki", "url": "https://wiki.example", "targetBlank": True}])

    def test_the_other_panels_of_the_document_keep_their_form(self):
        table = {x["id"]: x for x in panels(self.g)}[1]
        self.assertEqual(table["type"], "table")
        self.assertIn("targets", table)
        self.assertEqual(table["targets"][0]["rawSql"], "SELECT 1")

    def test_the_three_parity_fixtures_convert(self):
        for path in PARITY_TEXT:
            doc = load(path)
            g = to_grafana.convert(doc)
            found = panels(g)
            self.assertEqual([p["type"] for p in found], ["text"], path)
            self.assertEqual(found[0]["options"]["content"], "\n".join(doc["sections"][0]["panels"][0]["options"]["content"]))
            self.assertNotIn("targets", found[0], path)
        variables = to_grafana.convert(load(PARITY_TEXT[1]))["templating"]["list"]
        self.assertEqual([v["name"] for v in variables], ["region", "tier"])
        constructs = "\n".join(load(PARITY_TEXT[0])["sections"][0]["panels"][0]["options"]["content"])
        self.assertNotIn("$", constructs, "the first fixture has no variable")


class Annotations(unittest.TestCase):
    """The annotations (4.5b, 15.6, 15.14c): the items of `annotations.list`."""

    def setUp(self):
        self.doc = load(ANNOTATIONS)
        self.g = to_grafana.convert(self.doc)
        self.items = self.g["annotations"]["list"]

    def test_the_builtin_item_comes_first_with_hide_true(self):
        self.assertEqual(self.items[0]["builtIn"], 1)
        self.assertIs(self.items[0]["hide"], True)
        self.assertEqual(len(self.items), 1 + len(self.doc["annotations"]))

    def test_an_item_has_the_members_of_grafana(self):
        item = self.items[1]
        self.assertEqual(sorted(item), ["datasource", "enable", "filter", "hide", "iconColor", "name", "target"])
        self.assertEqual(item["datasource"], {"type": "grafana-clickhouse-datasource", "uid": "clickhouse"})
        self.assertIs(item["hide"], False)
        self.assertEqual((item["iconColor"], item["name"]), ("red", "Deploys"))
        t = item["target"]
        self.assertEqual((t["refId"], t["format"], t["queryType"]), ("Anno", 1, "table"))
        self.assertEqual(t["rawSql"], "\n".join(self.doc["annotations"][0]["sql"]))

    def test_the_filter_names_the_panels_and_does_not_exclude(self):
        self.assertEqual(self.items[1]["filter"], {"exclude": False, "ids": [20, 21]})
        self.assertNotIn("filter", self.items[2])

    def test_a_disabled_annotation_writes_enable_false(self):
        self.assertEqual([i["enable"] for i in self.items[1:]], [True, True, False, True, True, True, True, True])

    def test_the_colour_keeps_its_text(self):
        self.assertEqual(
            [i["iconColor"] for i in self.items[1:]],
            [a["color"] for a in self.doc["annotations"]],
        )

    def test_a_document_with_no_annotation_has_the_builtin_item_only(self):
        g = to_grafana.convert(load(FIXTURE))
        self.assertEqual([i["name"] for i in g["annotations"]["list"]], ["Annotations & Alerts"])

    def test_the_four_grafana_fixtures_are_the_conversion_of_the_four_parity_documents(self):
        for doc_path, grafana_path in zip(PARITY_ANNOTATIONS, GRAFANA_ANNOTATIONS):
            with open(grafana_path, encoding="utf-8") as f:
                self.assertEqual(f.read(), to_grafana.dump(to_grafana.convert(load(doc_path))), grafana_path)

    def test_the_fourth_fixture_has_1000_points(self):
        self.assertIn("numbers(1000)", "\n".join(load(PARITY_ANNOTATIONS[3])["annotations"][0]["sql"]))


class StatPercentChange(unittest.TestCase):
    """The percent change and the text sizes of a stat (4.29) get the names of Grafana 12.4.2."""

    BASE = {"reduce": {"calc": "last_not_null", "fields": None, "values": False}, "color_mode": "value", "graph_mode": "none", "text_mode": "auto", "orientation": "auto"}

    def test_absent_members_write_nothing(self):
        opts, custom = to_grafana.options_out("stat", dict(self.BASE))
        for key in ("showPercentChange", "percentChangeColorMode", "text"):
            self.assertNotIn(key, opts)
        self.assertEqual(custom, {})

    def test_the_four_members(self):
        o = dict(self.BASE, percent_change=True, percent_change_color="same_as_value", title_size=20, value_size=40)
        opts, _ = to_grafana.options_out("stat", o)
        self.assertIs(opts["showPercentChange"], True)
        self.assertEqual(opts["percentChangeColorMode"], "same_as_value")
        self.assertEqual(opts["text"], {"titleSize": 20, "valueSize": 40})
        opts, _ = to_grafana.options_out("stat", dict(self.BASE, value_size=1))
        self.assertEqual(opts["text"], {"valueSize": 1})


if __name__ == "__main__":
    unittest.main()
