#!/usr/bin/env python3
"""Generate the demo dashboards of Hune (format 1) into target/local/volume.

Each file is written atomically: a temporary name without the .json suffix, then a rename.
"""
import json
import os
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
# The test tooling variable HUNE_DEMO_VOLUME sets another volume folder.
VOL = os.environ.get("HUNE_DEMO_VOLUME") or os.path.join(ROOT, "target", "local", "volume")

HOST = "host IN (${host})"
RANGE = "$__timeFilter(ts)"


# ---------- builders ----------
def legend(show=True, placement="bottom", mode="list", values=None):
    return {"show": show, "placement": placement, "mode": mode, "values": values or []}


def query(sql, fmt="table", min_interval=None, mdp=None):
    return {"query": {"source": "prod", "format": fmt, "sql": sql if isinstance(sql, list) else sql.split("\n"),
                      "min_interval": min_interval, "max_data_points": mdp}}


def reuse(panel, with_transforms=True):
    return {"reuse": {"panel": panel, "with_transforms": with_transforms}}


def panel(pid, ptype, title, x, y, w, h, data, options, defaults=None, overrides=None, transforms=None,
          links=None, description="", transparent=False):
    return {
        "id": pid, "type": ptype, "title": title, "description": description, "transparent": transparent,
        "grid": {"x": x, "y": y, "w": w, "h": h},
        "data": data,
        "transforms": transforms or [],
        "fields": {"defaults": defaults or {}, "overrides": overrides or []},
        "options": options,
        "links": links or [],
    }


def ts_opts(draw="line", interp="linear", lw=1, fill=10, points="auto", psize=5, span=False, stacking="none",
            yscale="linear", leg=None, tip="multi"):
    return {"draw": draw, "interpolation": interp, "line_width": lw, "fill_opacity": fill, "show_points": points,
            "point_size": psize, "span_nulls": span, "stacking": stacking, "y_scale": yscale,
            "legend": leg or legend(), "tooltip": {"mode": tip}}


def reduce(calc="last", fields=None, values=False):
    return {"calc": calc, "fields": fields, "values": values}


def stat_opts(calc="last", fields=None, values=False, color_mode="value", graph="none", text="auto", orient="auto"):
    return {"reduce": reduce(calc, fields, values), "color_mode": color_mode, "graph_mode": graph,
            "text_mode": text, "orientation": orient}


def bar_opts(orient="vertical", x=None, stacking="none", show="auto", bw=70, gw=70, rot=0, leg=None, tip="multi"):
    return {"orientation": orient, "x_field": x, "stacking": stacking, "show_values": show, "bar_width": bw,
            "group_width": gw, "x_tick_rotation": rot, "legend": leg or legend(), "tooltip": {"mode": tip}}


def pie_opts(kind="pie", calc="last", fields=None, labels=None, leg=None, tip="single"):
    return {"pie_type": kind, "reduce": reduce(calc, fields, True), "labels": labels or [],
            "legend": leg or legend(), "tooltip": {"mode": tip}}


def xy_opts(x=None, ys=None, draw="points", ps=6, lw=1, leg=None, tip="single"):
    return {"x_field": x, "y_fields": ys or [], "draw": draw, "point_size": ps, "line_width": lw,
            "legend": leg or legend(), "tooltip": {"mode": tip}}


def gauge_opts(calc="last", fields=None, values=False, orient="horizontal", mode="gradient", unfilled=True):
    return {"reduce": reduce(calc, fields, values), "orientation": orient, "display_mode": mode,
            "show_unfilled": unfilled}


def thr(*steps, mode="absolute"):
    """thr(("red", None), ("green", 50)) -> thresholds object; the first step is the base."""
    return {"mode": mode, "steps": [{"value": v, "color": c} for c, v in steps]}


def color_thr():
    return {"mode": "thresholds", "fixed": None}


def color_fixed(c):
    return {"mode": "fixed", "fixed": c}


def color_palette():
    return {"mode": "palette", "fixed": None}


CPU_THR = thr(("green", None), ("yellow", 50), ("orange", 60), ("red", 70))


def var_query(name, sql, label=None, multi=True, all_=True, default=None, sort="alpha_asc", regex=None,
              hide="none", refresh="time_change", all_value=None):
    return {"kind": "query", "name": name, "label": label, "hide": hide, "source": "prod",
            "sql": sql if isinstance(sql, list) else [sql], "multi": multi, "include_all": all_,
            "all_value": all_value if all_ else None, "refresh": refresh, "sort": sort, "regex": regex,
            "default": default if default is not None else (["$__all"] if all_ else [])}


def var_custom(name, options, label=None, multi=False, all_=False, default=None, hide="none", all_value=None):
    return {"kind": "custom", "name": name, "label": label, "hide": hide,
            "options": [{"text": t, "value": v} for t, v in options], "multi": multi, "include_all": all_,
            "all_value": all_value if all_ else None, "default": default or []}


def var_text(name, default, label=None, vtype="text", hide="none"):
    return {"kind": "text", "name": name, "label": label, "hide": hide, "value_type": vtype, "default": default}


HOST_VAR = var_query("host", ["SELECT DISTINCT host", "FROM analytics.demo_metrics", "ORDER BY host"], label="Host")


def section(title, panels, collapsed=False):
    return {"title": title, "collapsed": collapsed, "panels": panels}


def dashboard(uid, title, description, tags, variables, sections, frm="now-24h", refresh=None, tz="browser",
              cursor="crosshair"):
    return {"format_version": 3, "uid": uid, "title": title, "description": description, "tags": tags,
            "time": {"from": frm, "to": "now", "timezone": tz, "refresh": refresh}, "cursor_sync": cursor,
            "links": [], "variables": variables, "sections": sections}


# ---------- common SQL ----------
SQL_CPU_TS = [
    "SELECT $__timeInterval(ts) AS t, host, avg(cpu) AS cpu",
    "FROM analytics.demo_metrics",
    f"WHERE {RANGE} AND {HOST}",
    "GROUP BY t, host",
    "ORDER BY t",
]
SQL_REQ_TS = [
    "SELECT $__timeInterval(ts) AS t, host, sum(requests) AS requests",
    "FROM analytics.demo_metrics",
    f"WHERE {RANGE} AND {HOST}",
    "GROUP BY t, host",
    "ORDER BY t",
]
SQL_PER_HOST = [
    "SELECT host, round(avg(cpu), 1) AS avg_cpu, round(max(cpu), 1) AS max_cpu, sum(requests) AS requests",
    "FROM analytics.demo_metrics",
    f"WHERE {RANGE} AND {HOST}",
    "GROUP BY host",
    "ORDER BY host",
]
SQL_WAVE = [
    "SELECT toDateTime($__fromTime + number * $__interval_s) AS t,",
    "  50 + 40 * sin(number / 12) AS sine,",
    "  50 + 30 * cos(number / 7) AS cosine,",
    "  20 + 70 * (number % 40) / 40 AS saw",
    "FROM numbers(300)",
    "WHERE toDateTime($__fromTime + number * $__interval_s) <= $__toTime",
    "ORDER BY t",
]


# ---------- dashboard 1: panel types tour ----------
def types_tour():
    p = []
    # Section 1: time series
    p.append(panel(1, "timeseries", "CPU by host (lines, legend table at the bottom)", 0, 0, 16, 9,
                   query(SQL_CPU_TS, "time_series", "1m"),
                   ts_opts(leg=legend(True, "bottom", "table", ["mean", "max", "last"]), tip="multi"),
                   {"unit": "percent", "decimals": 1, "min": 0, "max": 100, "color": color_palette()},
                   [{"match": {"by": "name", "value": "web-1"}, "set": {"color": color_fixed("red"), "display_name": "web-1 (primary)"}},
                    {"match": {"by": "regex", "value": "/web-[23]/"}, "set": {"decimals": 2}}],
                   description="Average CPU of each host. One override sets the colour of web-1, one sets the decimals of web-2 and web-3."))
    p.append(panel(2, "timeseries", "Requests (stacked bars, legend list at the right)", 16, 0, 8, 9,
                   query(SQL_REQ_TS.copy(), "time_series", "1h"),
                   ts_opts("bars", "linear", 1, 70, "never", 5, False, "normal",
                           leg=legend(True, "right", "list"), tip="multi"),
                   {"unit": "short", "decimals": 0, "color": color_palette()}))
    p.append(panel(3, "timeseries", "Generated waves (smooth, filled, legend table at the right)", 0, 9, 12, 8,
                   query(SQL_WAVE, "time_series", "1m", 200),
                   ts_opts("line", "smooth", 2, 35, "never", 5, False, "none",
                           leg=legend(True, "right", "table", ["min", "max", "mean", "last", "total"]), tip="multi"),
                   {"decimals": 1, "color": color_palette()},
                   [{"match": {"by": "name", "value": "saw"}, "set": {"unit": "percent", "color": color_fixed("purple")}},
                    {"match": {"by": "regex", "value": "/^(sine|cosine)$/"}, "set": {"unit": "none", "decimals": 2}}]))
    p.append(panel(4, "timeseries", "Steps and points (wide legend)", 12, 9, 12, 8,
                   query(SQL_WAVE, "time_series", "5m", 60),
                   ts_opts("points", "step_after", 1, 0, "always", 7, False, "none",
                           leg=legend(True, "bottom", "list"), tip="single"),
                   {"decimals": 0, "color": color_palette()}))
    # Section 2: single values
    q = ["SELECT sum(requests) AS total_requests, round(avg(cpu), 1) AS avg_cpu, round(max(cpu), 1) AS peak",
         "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}"]
    s = []
    s.append(panel(10, "stat", "Requests (value colour, area graph)", 0, 0, 6, 5,
                   query(["SELECT $__timeInterval(ts) AS t, sum(requests) AS requests",
                          "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}", "GROUP BY t ORDER BY t"],
                         "time_series", "1h"),
                   stat_opts("total", None, False, "value", "area", "auto", "auto"),
                   {"unit": "short", "decimals": 2, "color": color_thr(),
                    "thresholds": thr(("red", None), ("orange", 50000), ("green", 100000))}))
    s.append(panel(11, "stat", "Average CPU (background colour)", 6, 0, 6, 5,
                   query(q, "table"),
                   stat_opts("last", "/^avg_cpu$/", False, "background", "none", "value_and_name", "auto"),
                   {"unit": "percent", "decimals": 1, "color": color_thr(), "thresholds": CPU_THR}))
    s.append(panel(12, "stat", "Peak CPU (mapped, name only)", 12, 0, 6, 5,
                   query(q, "table"),
                   stat_opts("last", "/^peak$/", False, "background", "none", "value_and_name", "auto"),
                   {"unit": "percent", "decimals": 1, "color": color_thr(),
                    "thresholds": thr(("green", None), ("orange", 70), ("red", 75)),
                    "mappings": [{"kind": "range", "from": 74, "to": None, "text": "CRITICAL", "color": "dark-red"},
                                 {"kind": "range", "from": 70, "to": 74, "text": "High", "color": "orange"},
                                 {"kind": "special", "match": "null_nan", "text": "n/a", "color": None}]}))
    s.append(panel(13, "stat", "Per host (horizontal, no colour)", 18, 0, 6, 5,
                   query(SQL_PER_HOST, "table"),
                   stat_opts("last", "/^avg_cpu$/", True, "none", "none", "value_and_name", "horizontal"),
                   {"unit": "percent", "decimals": 1}))
    s.append(panel(14, "bargauge", "Average CPU by host (gradient)", 0, 5, 12, 6,
                   query(SQL_PER_HOST, "table"),
                   gauge_opts("last", "/^avg_cpu$/", True, "horizontal", "gradient", True),
                   {"unit": "percent", "decimals": 1, "min": 0, "max": 100, "color": color_thr(), "thresholds": CPU_THR}))
    s.append(panel(15, "bargauge", "Peak by host (retro LCD, vertical)", 12, 5, 12, 6,
                   query(SQL_PER_HOST, "table"),
                   gauge_opts("last", "/^max_cpu$/", True, "vertical", "lcd", False),
                   {"unit": "percent", "decimals": 0, "min": 0, "max": 100, "color": color_thr(), "thresholds": CPU_THR}))
    # Section 3: comparisons
    c = []
    c.append(panel(20, "barchart", "Requests by host (horizontal, values shown)", 0, 0, 8, 8,
                   query(["SELECT host, sum(requests) AS requests, requests / 2 AS half",
                          "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}", "GROUP BY host ORDER BY host"], "table"),
                   bar_opts("horizontal", "host", "none", "always", 70, 70, 0,
                            legend(True, "bottom", "list"), "multi"),
                   {"unit": "short", "decimals": 0, "color": color_palette()}))
    c.append(panel(21, "barchart", "Requests by hour (stacked, legend right)", 8, 0, 8, 8,
                   query(["SELECT toString(toHour(ts)) AS hour,",
                          "  sumIf(requests, host = 'web-1') AS web_1,",
                          "  sumIf(requests, host = 'web-2') AS web_2,",
                          "  sumIf(requests, host = 'web-3') AS web_3",
                          "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}",
                          "GROUP BY hour ORDER BY toUInt8(hour)"], "table"),
                   bar_opts("vertical", "hour", "normal", "never", 80, 90, -45,
                            legend(True, "right", "list"), "multi"),
                   {"unit": "short", "color": color_palette()}))
    c.append(panel(22, "piechart", "Share of requests (donut, legend table right)", 16, 0, 8, 8,
                   query(["SELECT host, sum(requests) AS requests", "FROM analytics.demo_metrics",
                          f"WHERE {RANGE} AND {HOST}", "GROUP BY host ORDER BY host"], "table"),
                   pie_opts("donut", "last", "/^requests$/", ["percent"],
                            legend(True, "right", "table", ["value", "percent"]), "single"),
                   {"unit": "short", "decimals": 0, "color": color_palette()}))
    c.append(panel(23, "piechart", "Pie with name and percent labels", 0, 8, 8, 8,
                   query(["SELECT host, round(avg(cpu), 1) AS cpu", "FROM analytics.demo_metrics",
                          f"WHERE {RANGE} AND {HOST}", "GROUP BY host ORDER BY host"], "table"),
                   pie_opts("pie", "last", "/^cpu$/", ["name", "percent"], legend(True, "bottom", "list"), "single"),
                   {"unit": "percent", "decimals": 1, "color": color_palette()}))
    c.append(panel(24, "xychart", "CPU against requests (scatter)", 8, 8, 8, 8,
                   query(["SELECT cpu, requests", "FROM analytics.demo_metrics",
                          f"WHERE {RANGE} AND host = 'web-1'", "ORDER BY ts LIMIT 600"], "table"),
                   xy_opts("cpu", ["requests"], "points", 5, 1, legend(True, "bottom", "list")),
                   {"decimals": 0, "color": color_palette()},
                   [{"match": {"by": "name", "value": "cpu"}, "set": {"unit": "percent", "decimals": 1}}]))
    c.append(panel(25, "xychart", "Generated spiral (points and lines)", 16, 8, 8, 8,
                   query(["SELECT number / 10 * cos(number / 6) AS x, number / 10 * sin(number / 6) AS y",
                          "FROM numbers(120)"], "table"),
                   xy_opts("x", ["y"], "points_and_lines", 4, 2, legend(False, "bottom", "list")),
                   {"decimals": 2, "color": color_fixed("blue")}))
    d = [panel(30, "table", "Hosts (collapsed section)", 0, 0, 24, 8,
               query(SQL_PER_HOST, "table"),
               {"show_header": True, "sort": [{"field": "host", "desc": False}]},
               {"align": "auto", "filterable": True})]
    return dashboard("types-tour", "Panel types tour",
                     "One example of each panel type with legends, thresholds, units, mappings and overrides.",
                     ["showcase", "panels"], [HOST_VAR],
                     [section("Time series", p), section("Single values", s), section("Comparisons", c),
                      section("Collapsed section", d, True)], refresh="5m")


# ---------- dashboard 2: tables ----------
def tables():
    t = []
    hourly = ["SELECT hour, host, round(c, 1) AS cpu, round(p, 1) AS peak, r AS requests,",
              "  multiIf(p >= 74, 'critical', p >= 70, 'warning', 'ok') AS status,",
              "  concat('/d/types-tour?var-host=', host) AS link",
              "FROM (",
              "  SELECT toStartOfHour(ts) AS hour, host, avg(cpu) AS c, max(cpu) AS p, sum(requests) AS r",
              "  FROM analytics.demo_metrics",
              f"  WHERE {RANGE} AND {HOST}",
              "  GROUP BY hour, host",
              ")",
              "ORDER BY hour DESC, host"]
    t.append(panel(1, "table", "Hourly metrics (cell types, widths, mappings, data links)", 0, 0, 24, 12,
                   query(hourly, "table"),
                   {"show_header": True, "sort": [{"field": "hour", "desc": True}, {"field": "host", "desc": False}]},
                   {"align": "auto", "filterable": True, "no_value": "-"},
                   [{"match": {"by": "name", "value": "hour"}, "set": {"width": 180, "align": "left"}},
                    {"match": {"by": "name", "value": "host"}, "set": {"width": 90, "links": [
                        {"title": "Open the tour for this host", "url": "/d/types-tour?var-host=${__value.raw}", "target_blank": False},
                        {"title": "ClickHouse docs", "url": "https://clickhouse.com/docs", "target_blank": True}]}},
                    {"match": {"by": "name", "value": "cpu"}, "set": {
                        "unit": "percent", "decimals": 1, "width": 120, "align": "right", "min": 0, "max": 100,
                        "color": color_thr(), "thresholds": CPU_THR, "cell": {"type": "color_background", "mode": "gradient"}}},
                    {"match": {"by": "name", "value": "peak"}, "set": {
                        "unit": "percent", "decimals": 1, "width": 120, "align": "right",
                        "color": color_thr(), "thresholds": CPU_THR, "cell": {"type": "color_background", "mode": "basic"}}},
                    {"match": {"by": "name", "value": "requests"}, "set": {
                        "unit": "short", "decimals": 1, "align": "right", "color": color_fixed("blue"),
                        "cell": {"type": "color_text", "mode": None}}},
                    {"match": {"by": "name", "value": "status"}, "set": {"width": 110, "align": "center", "mappings": [
                        {"kind": "value", "value": "ok", "text": "OK", "color": "green"},
                        {"kind": "value", "value": "warning", "text": "Warning", "color": "orange"},
                        {"kind": "regex", "pattern": "/^crit(ical)?$/", "text": "CRITICAL", "color": "red"}],
                        "cell": {"type": "color_background", "mode": "basic"}}},
                    {"match": {"by": "regex", "value": "/^link$/"}, "set": {"hidden": True}}],
                   links=[{"title": "All dashboards", "url": "/", "target_blank": False}],
                   description="Cell types color_text, color_background gradient and basic. Mappings on the status column. Click a column header to sort, use the filter on each column."))
    t.append(panel(2, "table", "Only hot rows (filter_by_value: peak above 70 and status not ok)", 0, 12, 12, 9,
                   query(hourly, "table"),
                   {"show_header": True, "sort": [{"field": "peak", "desc": True}]},
                   {"align": "auto", "unit": "none"},
                   [{"match": {"by": "name", "value": "peak"}, "set": {"unit": "percent", "decimals": 1, "color": color_thr(), "thresholds": CPU_THR, "cell": {"type": "color_text", "mode": None}}},
                    {"match": {"by": "regex", "value": "/^(link|hour|requests|cpu)$/"}, "set": {"hidden": True}}],
                   [{"kind": "filter_by_value", "mode": "include", "match": "all", "filters": [
                       {"field": "peak", "matcher": {"kind": "greater", "value": 70}},
                       {"field": "status", "matcher": {"kind": "not_equal", "value": "ok"}}]}]))
    t.append(panel(3, "table", "CPU by hour and host (grouping_to_matrix)", 12, 12, 12, 9,
                   query(["SELECT toHour(ts) AS hour, host, round(avg(cpu), 1) AS cpu",
                          "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}",
                          "GROUP BY hour, host ORDER BY hour"], "table"),
                   {"show_header": True, "sort": [{"field": "hour\\host", "desc": False}]},
                   {"unit": "percent", "decimals": 1, "color": color_thr(), "thresholds": CPU_THR,
                    "cell": {"type": "color_background", "mode": "gradient"}, "align": "center"},
                   [{"match": {"by": "name", "value": "hour\\host"}, "set": {"cell": {"type": "auto", "mode": None}, "unit": "none", "decimals": 0, "align": "left", "width": 90, "color": color_fixed("text")}}],
                   [{"kind": "grouping_to_matrix", "column_field": "host", "row_field": "hour", "value_field": "cpu",
                     "empty_value": "null"}]))
    t.append(panel(4, "table", "Many rows and columns (scrolling)", 0, 21, 24, 10,
                   query(["SELECT ts, host, cpu, requests, round(cpu * requests / 100) AS load, toHour(ts) AS hour,",
                          "  toDayOfWeek(ts) AS dow, concat('GET /api/v', toString(requests % 3), '/items/', toString(requests)) AS endpoint,",
                          "  if(cpu > 60, 'busy', 'idle') AS state",
                          "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}", "ORDER BY ts DESC LIMIT 500"], "table"),
                   {"show_header": True, "sort": []},
                   {"filterable": True},
                   [{"match": {"by": "name", "value": "endpoint"}, "set": {"width": 320}},
                    {"match": {"by": "name", "value": "ts"}, "set": {"width": 170}}]))
    t.append(panel(5, "table", "No header, compact", 0, 31, 12, 6,
                   query(SQL_PER_HOST, "table"),
                   {"show_header": False, "sort": []}, {"decimals": 1}))
    t.append(panel(6, "stat", "Reuse of the hourly table (reuse with its transforms)", 12, 31, 12, 6,
                   reuse(2, True),
                   stat_opts("count", "/^host$/", False, "value", "none", "value_and_name", "auto"),
                   {"unit": "none", "decimals": 0, "color": color_fixed("purple")},
                   description="This panel runs no query. It uses the frame of panel 2 and counts its rows."))
    return dashboard("tables-and-cells", "Tables, cells and transforms",
                     "Table cell types, widths, sort, filter, mappings, data links, transforms and a reuse panel.",
                     ["showcase", "tables"], [HOST_VAR], [section(None, t)], refresh=None)


# ---------- dashboard 3: variables lab ----------
def variables_lab():
    vars_ = [
        HOST_VAR,
        var_query("peak_hour", ["SELECT DISTINCT toHour(ts) AS h", "FROM analytics.demo_metrics",
                                f"WHERE {RANGE} AND {HOST}", "ORDER BY h"], label="Peak hour", multi=False,
                  all_=False, default=[], sort="num_asc", refresh="time_change"),
        var_custom("env", [("Production", "prod"), ("Staging", "stage"), ("Development", "dev")], label="Environment",
                   multi=False, all_=False, default=["prod"]),
        var_custom("window", [("1 minute", "1"), ("5 minutes", "5"), ("15 minutes", "15"), ("1 hour", "60")],
                   label="Window", multi=True, all_=True, default=["5", "15"]),
        var_text("threshold", "60", label="CPU threshold", vtype="number"),
        var_text("note", "load test", label="Note", vtype="text"),
        var_custom("hidden_label", [("A", "a"), ("B", "b")], label="Label hidden", multi=False, default=["a"], hide="label"),
        var_text("secret_mode", "1", vtype="number", hide="variable"),
    ]
    plist = []
    plist.append(panel(1, "timeseries", "Hosts above ${threshold} percent CPU", 0, 0, 14, 9,
                     query(["SELECT $__timeInterval(ts) AS t, host, max(cpu) AS cpu_max",
                            "FROM analytics.demo_metrics",
                            f"WHERE {RANGE} AND {HOST} AND cpu > ${{threshold}}",
                            "GROUP BY t, host ORDER BY t"], "time_series", "5m"),
                     ts_opts("points", "linear", 1, 0, "always", 5, False, "none",
                             leg=legend(True, "right", "table", ["max", "last"]), tip="multi"),
                     {"unit": "percent", "decimals": 1, "color": color_palette()},
                     description="Uses the text variable threshold in code. A number variable only accepts a number."))
    plist.append(panel(2, "stat", "Environment: ${env}, note: ${note}", 14, 0, 5, 9,
                     query(["SELECT '${env}' AS environment, '${note}' AS note, ${peak_hour} AS peak_hour"], "table"),
                     stat_opts("last", None, False, "none", "none", "value_and_name", "vertical"),
                     {"decimals": 0}))
    plist.append(panel(3, "table", "Selected values", 19, 0, 5, 9,
                     query(["SELECT arrayJoin([${host}]) AS selected_host, ${threshold} AS threshold"], "table"),
                     {"show_header": True, "sort": []}, {"align": "auto"}))
    plist.append(panel(4, "barchart", "Requests by window (custom multi-select)", 0, 9, 12, 8,
                     query(["SELECT toString(w) AS window, sum(requests) AS requests",
                            "FROM analytics.demo_metrics ARRAY JOIN [${window}] AS w",
                            f"WHERE {RANGE} AND {HOST}", "GROUP BY window ORDER BY toUInt32(window)"], "table"),
                     bar_opts("vertical", "window", "none", "auto", 60, 80, 0, legend(False), "single"),
                     {"unit": "short", "color": color_fixed("semi-dark-blue")}))
    plist.append(panel(5, "table", "Requests per host", 12, 9, 12, 8,
                     query(SQL_PER_HOST, "table"),
                     {"show_header": True, "sort": [{"field": "requests", "desc": True}]},
                     {"align": "auto"},
                     [{"match": {"by": "name", "value": "requests"}, "set": {"unit": "short", "decimals": 1}}]))
    plist.append(panel(6, "piechart", "Reuse of panel 5 (no extra query)", 0, 17, 8, 8,
                     reuse(5, False),
                     pie_opts("donut", "last", "/^requests$/", ["percent"], legend(True, "right", "list"), "single"),
                     {"unit": "short", "color": color_palette()},
                     description="Panel 6 reuses the frame of panel 5. The server runs no query for it."))
    plist.append(panel(7, "stat", "Rows in the sample", 8, 17, 8, 8,
                     query(["SELECT count() AS rows", "FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}"], "table"),
                     stat_opts("last", None, False, "value", "none", "value", "auto"),
                     {"unit": "short", "decimals": 1, "color": color_thr(),
                      "thresholds": thr(("red", None), ("yellow", 1000), ("green", 5000))}))
    plist.append(panel(8, "bargauge", "CPU by host", 16, 17, 8, 8,
                     query(SQL_PER_HOST, "table"),
                     gauge_opts("last", "/^avg_cpu$/", True, "horizontal", "basic", True),
                     {"unit": "percent", "min": 0, "max": 100, "decimals": 1, "color": color_thr(), "thresholds": CPU_THR}))
    return dashboard("variables-lab", "Variables lab",
                     "Query, custom and text variables: multi-select, All, number, chained, hidden label and hidden variable.",
                     ["showcase", "variables"], vars_, [section(None, plist)], frm="now-48h", refresh=None, tz="utc")


# ---------- dashboard 4: edge states ----------
def edge_states():
    e = []
    nd = "SELECT toDateTime(0) AS t, host, cpu, requests FROM analytics.demo_metrics WHERE 1 = 0"
    e.append(panel(1, "timeseries", "Error: unknown table", 0, 0, 8, 7,
                   query(["SELECT $__timeInterval(ts) AS t, avg(cpu) AS cpu", "FROM analytics.no_such_table",
                          "WHERE $__timeFilter(ts)", "GROUP BY t ORDER BY t"], "time_series", "1m"),
                   ts_opts(), {"unit": "percent"},
                   description="The SQL names a table that does not exist: the panel shows the error and a retry button."))
    e.append(panel(2, "table", "Error: bad column", 8, 0, 8, 7,
                   query(["SELECT does_not_exist FROM analytics.demo_metrics LIMIT 5"], "table"),
                   {"show_header": True, "sort": []}))
    e.append(panel(3, "stat", "Error: type mismatch", 16, 0, 8, 7,
                   query(["SELECT toUInt8('not a number') AS value"], "table"),
                   stat_opts()))
    names = [("timeseries", "No data: time series"), ("table", "No data: table"), ("stat", "No data: stat"),
             ("barchart", "No data: bar chart"), ("piechart", "No data: pie chart"), ("xychart", "No data: XY chart"),
             ("bargauge", "No data: bar gauge")]
    opts = {"timeseries": ts_opts(), "table": {"show_header": True, "sort": []}, "stat": stat_opts(),
            "barchart": bar_opts(), "piechart": pie_opts(), "xychart": xy_opts(), "bargauge": gauge_opts()}
    pid = 10
    for i, (ty, ti) in enumerate(names):
        fmt = "time_series" if ty == "timeseries" else "table"
        sql = ["SELECT $__timeInterval(ts) AS t, host, cpu FROM analytics.demo_metrics WHERE 1 = 0"] if ty == "timeseries" else [nd]
        e.append(panel(pid, ty, ti, (i % 4) * 6, 7 + (i // 4) * 6, 6, 6, query(sql, fmt, "1m" if ty == "timeseries" else None), opts[ty]))
        pid += 1
    e.append(panel(20, "stat", "No value text", 18, 13, 6, 6,
                   query(["SELECT toNullable(0) AS value WHERE 1 = 0"], "table"),
                   stat_opts("last", None, False, "value", "none", "value", "auto"),
                   {"no_value": "No data yet"}))
    e.append(panel(21, "stat", "Mapped null", 0, 19, 6, 6,
                   query(["SELECT CAST(NULL AS Nullable(Float64)) AS value"], "table"),
                   stat_opts("last", None, False, "background", "none", "value", "auto"),
                   {"mappings": [{"kind": "special", "match": "null", "text": "unknown", "color": "dark-yellow"}]}))
    f = []
    f.append(panel(30, "timeseries", "A very long panel title that does not fit in the header of the panel and must be cut with an ellipsis", 0, 0, 8, 8,
                   query(SQL_CPU_TS, "time_series", "5m"),
                   ts_opts(leg=legend(True, "bottom", "table", ["mean", "last"])),
                   {"unit": "percent", "decimals": 1},
                   links=[{"title": "Dashboards", "url": "/", "target_blank": False},
                          {"title": "Docs", "url": "https://clickhouse.com/docs", "target_blank": True}],
                   description="A long description. " * 20))
    gappy = ["SELECT toDateTime($__fromTime + number * 600) AS t,",
             "  if(number % 12 IN (4, 5, 6), NULL, 40 + 30 * sin(number / 9)) AS with_gaps,",
             "  50 + 10 * cos(number / 5) AS continuous",
             "FROM numbers(140) WHERE toDateTime($__fromTime + number * 600) <= $__toTime ORDER BY t"]
    f.append(panel(31, "timeseries", "Null gaps: lines broken", 8, 0, 8, 8,
                   query(gappy, "time_series", "10m"),
                   ts_opts("line", "linear", 2, 15, "always", 4, False, "none", leg=legend(True, "bottom", "list")),
                   {"decimals": 1}))
    f.append(panel(32, "timeseries", "Null gaps: lines joined", 16, 0, 8, 8,
                   query(gappy, "time_series", "10m"),
                   ts_opts("line", "step_before", 2, 15, "never", 4, True, "none", leg=legend(False)),
                   {"decimals": 1, "color": color_fixed("orange")}))
    f.append(panel(33, "table", "Slow query (2 s, shows the loading state)", 0, 8, 12, 6,
                   query(["SELECT sleep(2) AS slept, now() AS finished"], "table"),
                   {"show_header": True, "sort": []}))
    f.append(panel(34, "stat", "Huge and tiny numbers", 12, 8, 12, 6,
                   query(["SELECT 123456789012 AS big, 0.000001234 AS tiny, -42 AS negative, 1e15 AS huge"], "table"),
                   stat_opts("last", None, True, "value", "none", "value_and_name", "horizontal"),
                   {"decimals": 3, "unit": "short"},
                   [{"match": {"by": "name", "value": "tiny"}, "set": {"decimals": 9, "unit": "none"}},
                    {"match": {"by": "name", "value": "big"}, "set": {"unit": "bytes", "decimals": 1}},
                    {"match": {"by": "name", "value": "negative"}, "set": {"unit": "suffix: pts", "color": color_fixed("red")}}]))
    g = [panel(40, "table", "Hidden until expanded", 0, 0, 24, 6,
               query(SQL_PER_HOST, "table"), {"show_header": True, "sort": []})]
    return dashboard("edge-states", "Edge states: errors, no data, gaps",
                     "A panel with an error, a panel with no data for each type, null gaps, long titles and a slow query.",
                     ["showcase", "states"], [HOST_VAR],
                     [section("Errors and no data", e), section("Titles, gaps, slow and extreme values", f),
                      section("Collapsed with a long header to test truncation of a section title in narrow widths", g, True)],
                     frm="now-12h")


# ---------- dashboard 5: density ----------
def density():
    plist = []
    pid = 1
    kinds = [
        ("stat", "Requests", lambda: (query(["SELECT sum(requests) AS requests FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}"], "table"),
                                       stat_opts("last", None, False, "value", "none", "auto", "auto"),
                                       {"unit": "short", "decimals": 1, "color": color_thr(), "thresholds": thr(("red", None), ("green", 100000))})),
        ("timeseries", "CPU", lambda: (query(SQL_CPU_TS, "time_series", "5m"),
                                       ts_opts(leg=legend(False), tip="multi"), {"unit": "percent", "decimals": 1, "color": color_palette()})),
        ("bargauge", "Load", lambda: (query(SQL_PER_HOST, "table"),
                                      gauge_opts("last", "/^avg_cpu$/", True, "horizontal", "gradient", True),
                                      {"unit": "percent", "min": 0, "max": 100, "color": color_thr(), "thresholds": CPU_THR})),
        ("piechart", "Share", lambda: (query(["SELECT host, sum(requests) AS requests FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}", "GROUP BY host ORDER BY host"], "table"),
                                       pie_opts("donut", "last", "/^requests$/", [], legend(True, "right", "list")), {"unit": "short", "color": color_palette()})),
        ("barchart", "Hours", lambda: (query(["SELECT toString(toHour(ts)) AS hour, sum(requests) AS requests FROM analytics.demo_metrics", f"WHERE {RANGE} AND {HOST}", "GROUP BY hour ORDER BY toUInt8(hour)"], "table"),
                                       bar_opts("vertical", "hour", "none", "never", 70, 70, 0, legend(False), "single"), {"unit": "short", "color": color_fixed("blue")})),
        ("table", "Hosts", lambda: (query(SQL_PER_HOST, "table"), {"show_header": True, "sort": []}, {"decimals": 1})),
        ("xychart", "CPU / requests", lambda: (query(["SELECT cpu, requests FROM analytics.demo_metrics", f"WHERE {RANGE} AND host = 'web-2'", "LIMIT 300"], "table"),
                                               xy_opts("cpu", ["requests"], "points", 4, 1, legend(False)), {"decimals": 0})),
        ("timeseries", "Requests", lambda: (query(SQL_REQ_TS, "time_series", "1h"),
                                            ts_opts("bars", "linear", 1, 60, "never", 5, False, "normal", leg=legend(False), tip="multi"), {"unit": "short", "color": color_palette()})),
    ]
    for row in range(5):
        for col in range(4):
            i = row * 4 + col
            ty, name, mk = kinds[(i * 3 + row) % len(kinds)]
            data, opts, defaults = mk()
            plist.append(panel(pid, ty, f"{name} {pid}", col * 6, row * 6, 6, 6, data, opts, defaults))
            pid += 1
    return dashboard("density-20", "Density: 20 panels",
                     "Twenty small panels of all types to check the density of the grid and the speed of the page.",
                     ["ops", "density"], [HOST_VAR], [section(None, plist)], frm="now-24h", refresh="1m")


# ---------- dashboard 6: service overview ----------
def service_overview():
    top = []
    top.append(panel(1, "stat", "Requests / 24 h", 0, 0, 6, 4,
                     query(["SELECT sum(requests) AS requests FROM analytics.demo_metrics", f"WHERE {RANGE}"], "table"),
                     stat_opts("last", None, False, "value", "none", "value", "auto"),
                     {"unit": "short", "decimals": 2, "color": color_fixed("blue")},
                     links=[{"title": "Tour", "url": "/d/types-tour", "target_blank": False}]))
    top.append(panel(2, "stat", "Mean CPU", 6, 0, 6, 4,
                     query(["SELECT avg(cpu) AS cpu FROM analytics.demo_metrics", f"WHERE {RANGE}"], "table"),
                     stat_opts("last", None, False, "background", "none", "value", "auto"),
                     {"unit": "percent", "decimals": 1, "color": color_thr(), "thresholds": CPU_THR}))
    top.append(panel(3, "stat", "Hosts", 12, 0, 6, 4,
                     query(["SELECT uniqExact(host) AS hosts FROM analytics.demo_metrics", f"WHERE {RANGE}"], "table"),
                     stat_opts("last", None, False, "none", "none", "value", "auto"),
                     {"decimals": 0}))
    top.append(panel(4, "stat", "Availability", 18, 0, 6, 4,
                     query(["SELECT 0.9993 AS availability"], "table"),
                     stat_opts("last", None, False, "value", "area", "value", "auto"),
                     {"unit": "percentunit", "decimals": 2, "color": color_thr(),
                      "thresholds": thr(("red", None), ("orange", 0.99), ("green", 0.999))}))
    top.append(panel(5, "timeseries", "CPU", 0, 4, 12, 9,
                     query(SQL_CPU_TS, "time_series", "1m"),
                     ts_opts(leg=legend(True, "bottom", "table", ["mean", "max"])),
                     {"unit": "percent", "decimals": 1, "color": color_palette()}))
    top.append(panel(6, "timeseries", "Requests", 12, 4, 12, 9,
                     query(SQL_REQ_TS, "time_series", "30m"),
                     ts_opts("bars", "linear", 1, 70, "never", 5, False, "normal", leg=legend(True, "bottom", "list")),
                     {"unit": "short", "color": color_palette()}))
    return dashboard("service-overview", "Service overview",
                     "A realistic overview with stats on top and two charts, linked to the other demos.",
                     ["ops", "overview", "production"], [HOST_VAR], [section(None, top)], frm="now-12h", refresh="30s")


# ---------- writing ----------
def atomic_write(path, text):
    tmp = path[:-len(".json")] + ".tmp-write"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)


def write_folder(folder, title):
    d = os.path.join(VOL, folder)
    os.makedirs(d, exist_ok=True)
    path = os.path.join(d, "_folder.json")
    atomic_write(path, json.dumps({"format_version": 1, "title": title}, indent=2) + "\n")


def write_dash(folder, doc):
    path = os.path.join(VOL, folder, doc["uid"] + ".json")
    atomic_write(path, json.dumps(doc, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    write_folder("showcase", "Showcase")
    write_folder("ops", "Operations")
    for d in (types_tour(), tables(), variables_lab(), edge_states()):
        write_dash("showcase", d)
    for d in (density(), service_overview()):
        write_dash("ops", d)
    print("written")
