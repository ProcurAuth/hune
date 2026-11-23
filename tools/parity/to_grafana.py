#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Convert a Hune dashboard document (format 2) to a Grafana dashboard (classic JSON).

Use:
  python3 -I tools/parity/to_grafana.py DOC.json [OUT.json]
  python3 -I tools/parity/to_grafana.py --volume VOLUME --out DIR [--skip UID]...

The first form converts one document. It writes to OUT.json, or to the standard output.
The second form converts each document of a volume. It writes DIR/<folder title>/<uid>.json.
This is the layout of the provisioned folders of the disposable Grafana of the comparison.

The output is the inverse of the importer of section 15.6 (crates/hune-grafana):
the importer reads it back to the same document, except for the known items that
`KNOWN_LOSSES` lists. The query text is not changed: Hune and the Grafana plugins
use the same macros and the same variables.

A source of the kind `postgres` gets the data source type `grafana-postgresql-datasource`, and its
queries have the members of the PostgreSQL plugin: `editorMode` `code`, `rawQuery` and a format text
(`table` or `time_series`). Use `--source-kind SOURCE=postgres` to name such a source. The other
sources have the kind `clickhouse`.

Only the Python standard library is used.
"""
import argparse
import json
import os
import sys

SCHEMA_VERSION = 42  # Grafana 12.4.2
GRAFANA_VERSION = "12.4.2"
PLUGIN_TYPE = "grafana-clickhouse-datasource"
PLUGIN_VERSION = "4.5.1"
POSTGRES_PLUGIN_TYPE = "grafana-postgresql-datasource"
DEFAULT_DS_UID = "clickhouse"
DASHBOARD_DS = {"type": "datasource", "uid": "-- Dashboard --"}
MIXED_DS = {"type": "datasource", "uid": "-- Mixed --"}

# What the round trip (to_grafana.py, then `hune import-grafana`) does not give back.
KNOWN_LOSSES = (
    "text variable with value_type number that no SQL uses: the importer gives value_type text",
    "text variable with value_type text that a SQL uses: the importer blocks the dashboard (19.11)",
    "table field properties hidden and hide_from: both are written in custom.hideFrom (Grafana 12.4.2), and the importer gives back the flag viz as hidden only: hide_from of a table column is lost",
    "a section with title null and no panel: the importer makes no section",
    "a document that the validation of section 4 refuses (grid-errors): the importer blocks it",
    "the member placement of a variable (4.7a): Grafana has no such member, so the converter drops it with a warning and the importer gives back a variable with no placement",
    "a panel with one query whose ref is not A (4.15a): the importer writes no ref for a panel with one query (4.42), so the ref is lost; the converter writes the refId, and the importer reads it as metadata",
    "an annotation (4.5b): the importer lists the item hide=false (15.6) and the warning that it runs no query on the SQL (7.37); the document that it gives back is the same",
    "a convert_field_type or partition_by_values transform (4.25a): the importer lists one item with the class changed, because the rules of the values differ from Grafana (19.20j); the document that it gives back is the same",
)

HIDE = {"none": 0, "label": 1, "variable": 2}
REFRESH = {"load": 1, "time_change": 2}
SORT = {
    "none": 0,
    "alpha_asc": 1,
    "alpha_desc": 2,
    "num_asc": 3,
    "num_desc": 4,
    "alpha_ci_asc": 5,
    "alpha_ci_desc": 6,
    "natural_asc": 7,
    "natural_desc": 8,
}
CALC = {
    "last_not_null": "lastNotNull",
    "last": "last",
    "first_not_null": "firstNotNull",
    "first": "first",
    "min": "min",
    "max": "max",
    "mean": "mean",
    "total": "sum",
    "count": "count",
    "median": "median",
    "std_dev": "stdDev",
    "variance": "variance",
    "range": "range",
    "diff": "diff",
    "diff_percent": "diffperc",
    "delta": "delta",
    "distinct_count": "distinctCount",
    "change_count": "changeCount",
}
# The legend table, the footer and the reduction use the same calculations (4.26).
LEGEND_CALC = CALC
# The title of the legend column of a calculation in Grafana: the value of `legend.sortBy`.
LEGEND_TITLE = {
    "last_not_null": "Last *",
    "last": "Last",
    "first_not_null": "First *",
    "first": "First",
    "min": "Min",
    "max": "Max",
    "mean": "Mean",
    "median": "Median",
    "variance": "Variance",
    "std_dev": "StdDev",
    "total": "Total",
    "count": "Count",
    "range": "Range",
    "delta": "Delta",
    "diff": "Difference",
    "diff_percent": "Difference percent",
    "distinct_count": "Distinct Count",
    "change_count": "Change Count",
    "name": "Name",
    "value": "Value",
    "percent": "Percent",
}
THRESHOLDS_STYLE = {
    "off": "off",
    "line": "line",
    "dashed": "dashed",
    "area": "area",
    "line_area": "line+area",
    "dashed_area": "dashed+area",
}
INTERPOLATION = {"linear": "linear", "smooth": "smooth", "step_before": "stepBefore", "step_after": "stepAfter"}
TOOLTIP = {"single": "single", "multi": "multi", "hidden": "none"}
XY_SHOW = {"points": "points", "lines": "lines", "points_and_lines": "points+lines"}
MATCHER_ID = {
    "equal": "equal",
    "not_equal": "notEqual",
    "greater": "greater",
    "greater_or_equal": "greaterOrEqual",
    "lower": "lower",
    "lower_or_equal": "lowerOrEqual",
    "range": "between",
    "regex": "regex",
    "is_null": "isNull",
    "is_not_null": "isNotNull",
}
# The colour modes of 4.19 that are gradients, and the ids of Grafana.
CONTINUOUS = {
    "continuous_gr_yl_rd": "continuous-GrYlRd",
    "continuous_rd_yl_gr": "continuous-RdYlGr",
    "continuous_bl_yl_rd": "continuous-BlYlRd",
    "continuous_yl_rd": "continuous-YlRd",
    "continuous_bl_pu": "continuous-BlPu",
    "continuous_yl_bl": "continuous-YlBl",
    "continuous_blues": "continuous-blues",
    "continuous_reds": "continuous-reds",
    "continuous_greens": "continuous-greens",
    "continuous_purples": "continuous-purples",
}
# The types of 4.13 that have another id in Grafana.
GRAFANA_TYPE = {"state_timeline": "state-timeline", "status_history": "status-history"}
SPECIAL = {"null": "null", "nan": "nan", "null_nan": "null+nan", "true": "true", "false": "false", "empty": "empty"}


def ds(uid, kind="clickhouse"):
    """The data source reference of a source of the kind `clickhouse` or `postgres`."""
    return {"type": POSTGRES_PLUGIN_TYPE if kind == "postgres" else PLUGIN_TYPE, "uid": uid}


def _clickhouse_kind(_source):
    return "clickhouse"


def target_out(kind, uid, fmt_name, text, ref_id="A", hide=False):
    """The query of a panel for a source of the given kind. `fmt_name` is `table` or `time_series`.

    `hide` writes `"hide": true` (4.15a, rule 3). The member is not written for `false`, as Grafana does.
    """
    if kind == "postgres":
        out = {
            "datasource": ds(uid, kind),
            "editorMode": "code",
            "format": fmt_name,
            "rawQuery": True,
            "rawSql": text,
            "refId": ref_id,
        }
    else:
        fmt = 1 if fmt_name == "table" else 0
        out = {
            "datasource": ds(uid),
            "editorType": "sql",
            "format": fmt,
            "pluginVersion": PLUGIN_VERSION,
            "queryType": "table" if fmt == 1 else "timeseries",
            "rawSql": text,
            "refId": ref_id,
        }
    if hide:
        out["hide"] = True
    return out


def queries_of(data):
    """The queries of the member `data` of a panel in order: `data.query`, then the items of `data.extra_queries`
    (4.15a).

    Each result is a dict with `ref`, `source`, `format`, `sql` and `hide`. The first query has the ref `A`
    when the member `ref` is absent.
    """
    q = data["query"]
    first = {
        "ref": q.get("ref", "A"),
        "source": q["source"],
        "format": q["format"],
        "sql": q["sql"],
        "hide": bool(q.get("hide", False)),
    }
    return [first] + [
        {
            "ref": e["ref"],
            "source": e["source"],
            "format": e["format"],
            "sql": e["sql"],
            "hide": bool(e.get("hide", False)),
        }
        for e in data.get("extra_queries", [])
    ]


def sql_text(lines):
    return "\n".join(lines)


# ---------------------------------------------------------------- field config


def _mapping_result(m, index):
    result = {"index": index}
    if m.get("text") is not None:
        result["text"] = m["text"]
    if m.get("color") is not None:
        result["color"] = m["color"]
    return result


def mappings_out(items):
    """One Grafana item for each mapping, in the order of the array (the index gives the order)."""
    out = []
    for i, m in enumerate(items):
        kind = m["kind"]
        if kind == "value":
            out.append({"type": "value", "options": {m["value"]: _mapping_result(m, i)}})
        elif kind == "range":
            out.append(
                {
                    "type": "range",
                    "options": {"from": m.get("from"), "to": m.get("to"), "result": _mapping_result(m, i)},
                }
            )
        elif kind == "regex":
            out.append({"type": "regex", "options": {"pattern": m["pattern"], "result": _mapping_result(m, i)}})
        elif kind == "special":
            out.append(
                {"type": "special", "options": {"match": SPECIAL[m["match"]], "result": _mapping_result(m, i)}}
            )
        else:
            raise ValueError("unknown mapping kind %r" % kind)
    return out


def color_out(c):
    mode = c["mode"]
    if mode == "fixed":
        return {"mode": "fixed", "fixedColor": c["fixed"]}
    if mode == "thresholds":
        return {"mode": "thresholds"}
    if mode == "palette":
        return {"mode": "palette-classic"}
    if mode in CONTINUOUS:
        return {"mode": CONTINUOUS[mode]}
    raise ValueError("unknown colour mode %r" % mode)


def thresholds_out(t):
    return {
        "mode": t["mode"],
        "steps": [{"color": s["color"], "value": s["value"]} for s in t["steps"]],
    }


def cell_out(cell):
    if cell["type"] == "auto":
        return {"type": "auto"}
    if cell["type"] == "color_text":
        return {"type": "color-text"}
    if cell["type"] == "color_background":
        return {"type": "color-background", "mode": cell.get("mode") or "gradient"}
    if cell["type"] == "gauge":
        return {"type": "gauge", "mode": cell.get("mode") or "gradient"}
    if cell["type"] == "json":
        return {"type": "json-view"}
    raise ValueError("unknown cell type %r" % cell["type"])


def footer_out(footer):
    """The footer of a column: the calculations of Grafana in `custom.footer.reducers`."""
    return [CALC[r] for r in footer["reducers"]]


def hide_from_out(h):
    return {"legend": bool(h["legend"]), "tooltip": bool(h["tooltip"]), "viz": bool(h["viz"])}


def links_out(items):
    return [{"title": l["title"], "url": l["url"], "targetBlank": bool(l.get("target_blank"))} for l in items]


# Property of the set (4.18) -> (Grafana id, converter). The id is the member of `defaults`
# (a dotted id is a member of `defaults.custom`) and the `id` of an override property.
PROPS = (
    ("unit", "unit", lambda v: v),
    ("decimals", "decimals", lambda v: v),
    ("min", "min", lambda v: v),
    ("max", "max", lambda v: v),
    ("color", "color", color_out),
    ("thresholds", "thresholds", thresholds_out),
    ("mappings", "mappings", mappings_out),
    ("links", "links", links_out),
    ("display_name", "displayName", lambda v: v),
    ("no_value", "noValue", lambda v: v),
    ("width", "custom.width", lambda v: v),
    ("align", "custom.align", lambda v: v),
    ("cell", "custom.cellOptions", cell_out),
    ("filterable", "custom.filterable", lambda v: v),
    ("footer", "custom.footer.reducers", footer_out),
    # hide_from comes before hidden: a table with both has the flag viz of `hidden`.
    ("hide_from", "custom.hideFrom", hide_from_out),
    # Grafana 12 writes "Hide in table" as custom.hideFrom.viz. It reads custom.hidden only in an old panel (the
    # migration of the table panel), so the converter writes the form that the 12.4.2 table reads.
    ("hidden", "custom.hideFrom.viz", lambda v: v),
    ("axis_grid", "custom.axisGridShow", lambda v: v),
)


def defaults_out(props):
    out = {}
    custom = {}
    for name, gid, conv in PROPS:
        if name not in props:
            continue
        if gid.startswith("custom."):
            node = custom
            *parents, last = gid[len("custom.") :].split(".")
            for part in parents:
                node = node.setdefault(part, {})
            node[last] = conv(props[name])
        else:
            out[gid] = conv(props[name])
    return out, custom


def overrides_out(items):
    out = []
    for o in items:
        match = o["match"]
        matcher_id = {"name": "byName", "regex": "byRegexp", "names": "byNames", "query": "byFrameRefID"}[match["by"]]
        if match["by"] == "names":
            options = {"mode": match["mode"], "names": list(match["names"])}
        else:
            options = match["value"]
        properties = []
        for name, gid, conv in PROPS:
            if name in o["set"]:
                properties.append({"id": gid, "value": conv(o["set"][name])})
        out.append({"matcher": {"id": matcher_id, "options": options}, "properties": properties})
    return out


# --------------------------------------------------------------------- options


def legend_out(lg, pie=False):
    out = {
        "showLegend": bool(lg["show"]),
        "displayMode": lg["mode"],
        "placement": lg["placement"],
    }
    if pie:
        out["values"] = list(lg["values"])
    else:
        out["calcs"] = [LEGEND_CALC[v] for v in lg["values"]]
    if "sort_by" in lg:
        out["sortBy"] = LEGEND_TITLE[lg["sort_by"]]
    if "sort_desc" in lg:
        out["sortDesc"] = bool(lg["sort_desc"])
    return out


def tooltip_out(t):
    out = {"mode": TOOLTIP[t["mode"]], "sort": t.get("sort", "none")}
    if "hide_zeros" in t:
        out["hideZeros"] = bool(t["hide_zeros"])
    return out


def reduce_out(r):
    return {"calcs": [CALC[r["calc"]]], "fields": r["fields"] or "", "values": bool(r["values"])}


def _thresholds_style(o, custom):
    """The option `thresholds_style` is in `custom.thresholdsStyle.mode` (4.28, 4.30)."""
    if "thresholds_style" in o:
        custom["thresholdsStyle"] = {"mode": THRESHOLDS_STYLE[o["thresholds_style"]]}
    return custom


def _ts_custom(o):
    custom = {
        "drawStyle": o["draw"],
        "lineInterpolation": INTERPOLATION[o["interpolation"]],
        "lineWidth": o["line_width"],
        "fillOpacity": o["fill_opacity"],
        "showPoints": o["show_points"],
        "pointSize": o["point_size"],
        "spanNulls": o["span_nulls"],
        "stacking": {"group": "A", "mode": o["stacking"]},
        "scaleDistribution": {"type": o["y_scale"]},
    }
    if o["y_scale"] == "log":
        custom["scaleDistribution"]["log"] = 10
    return _thresholds_style(o, custom)


def _xy_custom(o):
    """Grafana 12 reads the draw style, the point size and the line width of an xychart in the field config."""
    return {
        "show": XY_SHOW[o["draw"]],
        "pointSize": {"fixed": o["point_size"], "min": 1, "max": 100},
        "lineWidth": o["line_width"],
    }


def _xy_matcher(name):
    return {"matcher": {"id": "byName", "options": name}}


def options_out(ptype, o):
    """Returns (options, defaults.custom) of a panel."""
    if ptype == "table":
        sort = [{"displayName": k["field"], "desc": bool(k["desc"])} for k in o["sort"]]
        opts = {"showHeader": bool(o["show_header"])}
        if sort:
            opts["sortBy"] = sort
        # Grafana 12 holds the wrap of the text in custom.wrapText.
        return opts, ({"wrapText": bool(o["wrap"])} if "wrap" in o else {})
    if ptype == "timeseries":
        return {"legend": legend_out(o["legend"]), "tooltip": tooltip_out(o["tooltip"])}, _ts_custom(o)
    if ptype == "stat":
        opts = {
            "reduceOptions": reduce_out(o["reduce"]),
            "colorMode": o["color_mode"],
            "graphMode": o["graph_mode"],
            "textMode": o["text_mode"],
            "orientation": o["orientation"],
        }
        if "justify" in o:
            opts["justifyMode"] = o["justify"]
        # The percent change and the text sizes (4.29).
        if o.get("percent_change"):
            opts["showPercentChange"] = True
        if "percent_change_color" in o:
            opts["percentChangeColorMode"] = o["percent_change_color"]
        text = {name: o[member] for member, name in (("title_size", "titleSize"), ("value_size", "valueSize")) if member in o}
        if text:
            opts["text"] = text
        return opts, {}
    if ptype == "barchart":
        opts = {
            "orientation": o["orientation"],
            "stacking": o["stacking"],
            "showValue": o["show_values"],
            "barWidth": round(o["bar_width"] / 100, 4),
            "groupWidth": round(o["group_width"] / 100, 4),
            "xTickLabelRotation": o["x_tick_rotation"],
            "legend": legend_out(o["legend"]),
            "tooltip": tooltip_out(o["tooltip"]),
        }
        if o["x_field"]:
            opts["xField"] = o["x_field"]
        # The optional members of 4.30 (P03): an absent member is the Grafana default (80 and 1).
        custom = {k: o[n] for n, k in (("fill_opacity", "fillOpacity"), ("line_width", "lineWidth")) if n in o}
        return opts, _thresholds_style(o, custom)
    if ptype == "piechart":
        return (
            {
                "pieType": o["pie_type"],
                "reduceOptions": reduce_out(o["reduce"]),
                "displayLabels": list(o["labels"]),
                "legend": legend_out(o["legend"], pie=True),
                "tooltip": tooltip_out(o["tooltip"]),
            },
            {},
        )
    if ptype == "xychart":
        opts = {"legend": legend_out(o["legend"]), "tooltip": tooltip_out(o["tooltip"])}
        if o["x_field"] and o["y_fields"]:
            # Grafana 12: one series for each y field, with matchers (the mode "manual").
            opts["mapping"] = "manual"
            opts["series"] = [
                {"frame": {"matcher": {"id": "byIndex", "options": 0}}, "x": _xy_matcher(o["x_field"]), "y": _xy_matcher(y)}
                for y in o["y_fields"]
            ]
        else:
            # The mode "auto": the first number field is x, the other number fields are y.
            opts["mapping"] = "auto"
            opts["series"] = [{}]
        return opts, _xy_custom(o)
    if ptype == "bargauge":
        return (
            {
                "reduceOptions": reduce_out(o["reduce"]),
                "orientation": o["orientation"],
                "displayMode": o["display_mode"],
                "showUnfilled": bool(o["show_unfilled"]),
            },
            {},
        )
    if ptype in ("state_timeline", "status_history"):
        return _timeline_options(ptype, o)
    if ptype == "heatmap":
        return _heatmap_options(o), {}
    if ptype == "histogram":
        return _histogram_options(o)
    raise ValueError("unknown panel type %r" % ptype)


def _timeline_options(ptype, o):
    """The options of a state timeline or a status history in the form of Grafana 12.4.2 (4.33b, 15.6).

    The percent members are fractions. The members of the series are in `custom`. The axis placement `hidden`
    stands for `show_labels` equal to false. The placement `auto` is the default, so the converter leaves it out.
    The legend and the tooltip have the members that 4.33b permits: no calculation, no sort.
    """
    opts = {
        "showValue": o["show_values"],
        "rowHeight": round(o["row_height"] / 100, 4),
        "legend": legend_out(o["legend"]),
        "tooltip": tooltip_out(o["tooltip"]),
    }
    custom = {"lineWidth": o["line_width"], "fillOpacity": o["fill_opacity"]}
    if ptype == "state_timeline":
        opts["mergeValues"] = bool(o["merge_values"])
        opts["alignValue"] = o["align_values"]
        custom["spanNulls"] = bool(o["span_nulls"])
    else:
        opts["colWidth"] = round(o["col_width"] / 100, 4)
    if not o["show_labels"]:
        custom["axisPlacement"] = "hidden"
    return opts, custom


# The names of the 10 schemes of the heatmap in Grafana (4.33c).
HEAT_SCHEME = {
    "oranges": "Oranges",
    "blues": "Blues",
    "greens": "Greens",
    "greys": "Greys",
    "purples": "Purples",
    "reds": "Reds",
    "yl_or_rd": "YlOrRd",
    "rd_yl_gn": "RdYlGn",
    "spectral": "Spectral",
    "viridis": "Viridis",
}


def _number_text(v):
    """The text of a number for a value of the buckets: a whole number has no fraction."""
    return str(int(v)) if float(v).is_integer() else repr(float(v))


def _heat_calculation(o):
    """The member `calculation` of a heatmap: the two bucket objects (4.33c). A `null` value is not written."""
    x, y = o["x_buckets"], o["y_buckets"]
    xb = {"mode": x["mode"]}
    if x["value"] is not None:
        xb["value"] = _number_text(x["value"])
    yb = {"mode": y["mode"]}
    if y["value"] is not None:
        yb["value"] = _number_text(y["value"])
    if y["scale"] != "linear":
        yb["scale"] = {"type": "log", "log": 2 if y["scale"] == "log2" else 10}
    return {"xBuckets": xb, "yBuckets": yb}


def _heatmap_options(o):
    """The options of a heatmap in the form of Grafana 12.4.2 (4.33c, 15.6).

    The unit and the decimals are field properties: the converter writes them in `defaults`.
    The importer reads them back from there. The tooltip has no sort.
    """
    c = o["color"]
    color = {
        "mode": c["mode"],
        "scheme": HEAT_SCHEME[c["scheme"]],
        "fill": c["fill"],
        "scale": c["scale"],
        "exponent": c["exponent"],
        "steps": c["steps"],
        "reverse": bool(c["reverse"]),
    }
    for key in ("min", "max"):
        if c[key] is not None:
            color[key] = c[key]
    y_axis = {"axisPlacement": o["y_axis"], "reverse": bool(o["y_reverse"])}
    for key, member in (("y_min", "min"), ("y_max", "max")):
        if o[key] is not None:
            y_axis[member] = o[key]
    return {
        "calculate": bool(o["calculate"]),
        "calculation": _heat_calculation(o),
        "yAxis": y_axis,
        "color": color,
        "cellGap": o["cell_gap"],
        "filterValues": {"le": o["hide_le"], "ge": o["hide_ge"]},
        "legend": {"show": bool(o["show_legend"])},
        "tooltip": {"mode": TOOLTIP[o["tooltip"]["mode"]]},
    }


def _histogram_options(o):
    """The options of a histogram and its `custom` members in the form of Grafana 12.4.2 (4.33d, 15.6)."""
    opts = {
        "bucketCount": o["bucket_count"],
        "bucketOffset": o["bucket_offset"],
        "combine": bool(o["combine"]),
        "legend": legend_out(o["legend"]),
        "tooltip": tooltip_out(o["tooltip"]),
    }
    if o["bucket_size"] is not None:
        opts["bucketSize"] = o["bucket_size"]
    custom = {
        "stacking": {"group": "A", "mode": o["stacking"]},
        "lineWidth": o["line_width"],
        "fillOpacity": o["fill_opacity"],
    }
    return opts, custom


# ---------------------------------------------------------------- transforms


def transforms_out(items):
    out = []
    for t in items:
        if t["kind"] == "filter_by_value":
            filters = []
            for f in t["filters"]:
                m = f["matcher"]
                mid = MATCHER_ID[m["kind"]]
                if m["kind"] == "range":
                    options = {"from": m["from"], "to": m["to"]}
                elif m["kind"] == "regex":
                    options = {"value": m["pattern"]}
                elif m["kind"] in ("is_null", "is_not_null"):
                    options = {}
                else:
                    options = {"value": m["value"]}
                filters.append({"fieldName": f["field"], "config": {"id": mid, "options": options}})
            out.append(
                {
                    "id": "filterByValue",
                    "options": {"type": t["mode"], "match": t["match"], "filters": filters},
                }
            )
        elif t["kind"] == "grouping_to_matrix":
            out.append(
                {
                    "id": "groupingToMatrix",
                    "options": {
                        "columnField": t["column_field"],
                        "rowField": t["row_field"],
                        "valueField": t["value_field"],
                        "emptyValue": t["empty_value"],
                    },
                }
            )
        elif t["kind"] in MORE_TRANSFORMS:
            out.append(more_transform_out(t))
        else:
            raise ValueError("unknown transform kind %r" % t["kind"])
    return out


BINARY_SYMBOL = {"add": "+", "subtract": "-", "multiply": "*", "divide": "/"}
CONVERT_TYPE = {"number": "number", "text": "string", "time": "time", "boolean": "boolean"}
MORE_TRANSFORMS = (
    "organize",
    "calculate_field",
    "sort_by",
    "limit",
    "merge",
    "reduce",
    "partition_by_values",
    "join_by_field",
    "filter_fields_by_name",
    "rename_by_regex",
    "convert_field_type",
)


def number_text(v):
    """The text of a number operand: Grafana reads a text that is a number as a constant (15.6)."""
    if isinstance(v, float) and v == int(v) and abs(v) < 1e15:
        return str(int(v))
    return repr(v)


def operand_out(o):
    return o["field"] if o["kind"] == "field" else number_text(o["value"])


def calculate_field_options(t):
    options = {"mode": {"reduce_row": "reduceRow"}.get(t["mode"], t["mode"])}
    if t["alias"] is not None:
        options["alias"] = t["alias"]
    options["replaceFields"] = bool(t["replace_fields"])
    if t["mode"] == "binary":
        b = t["binary"]
        options["binary"] = {
            "left": operand_out(b["left"]),
            "operator": BINARY_SYMBOL[b["operator"]],
            "right": operand_out(b["right"]),
        }
    elif t["mode"] == "reduce_row":
        r = t["reduce"]
        options["reduce"] = {"reducer": CALC[r["reducer"]]}
        if r["include"]:
            options["reduce"]["include"] = list(r["include"])
    elif t["mode"] == "unary":
        options["unary"] = {"operator": t["unary"]["operator"], "fieldName": t["unary"]["field"]}
    else:
        options["index"] = {"asPercentile": bool(t["index"]["as_percentile"])}
    return options


def name_filter_out(f):
    out = {}
    if f["names"]:
        out["names"] = list(f["names"])
    if f["pattern"] is not None:
        out["pattern"] = f["pattern"]
    return out


def more_transform_out(t):
    """A transform of the eleven kinds of 4.25a as the Grafana item `{id, options}` (15.6)."""
    kind = t["kind"]
    if kind == "organize":
        return {
            "id": "organize",
            "options": {
                "excludeByName": {n: True for n in t["hide"]},
                "indexByName": {n: i for i, n in enumerate(t["order"])},
                "renameByName": {r["field"]: r["name"] for r in t["rename"]},
            },
        }
    if kind == "calculate_field":
        return {"id": "calculateField", "options": calculate_field_options(t)}
    if kind == "sort_by":
        return {"id": "sortBy", "options": {"sort": [{"field": k["field"], "desc": k["desc"]} for k in t["sort"]]}}
    if kind == "limit":
        count = t["count"]
        return {"id": "limit", "options": {"limitField": -count if t["end"] == "last" else count}}
    if kind == "merge":
        return {"id": "merge", "options": {}}
    if kind == "reduce":
        mode = {"series_to_rows": "seriesToRows", "reduce_fields": "reduceFields"}[t["mode"]]
        return {"id": "reduce", "options": {"mode": mode, "reducers": [CALC[r] for r in t["reducers"]]}}
    if kind == "partition_by_values":
        return {"id": "partitionByValues", "options": {"fields": list(t["fields"]), "keepFields": t["keep_fields"]}}
    if kind == "join_by_field":
        options = {"mode": t["mode"]}
        if t["field"] is not None:
            options["byField"] = t["field"]
        return {"id": "joinByField", "options": options}
    if kind == "filter_fields_by_name":
        return {
            "id": "filterFieldsByName",
            "options": {"include": name_filter_out(t["include"]), "exclude": name_filter_out(t["exclude"])},
        }
    if kind == "rename_by_regex":
        return {"id": "renameByRegex", "options": {"regex": t["regex"], "renamePattern": t["rename_pattern"]}}
    return {
        "id": "convertFieldType",
        "options": {
            "conversions": [
                {"targetField": c["field"], "destinationType": CONVERT_TYPE[c["to"]]} for c in t["conversions"]
            ]
        },
    }


# --------------------------------------------------------------------- panels


def text_options_out(o):
    """The options of a text panel in the form of Grafana 12.4.2 (4.33a, 15.6, 15.14c).

    The mode is `markdown`. The content is the lines joined with one line end. The member `code` has the
    default values of Grafana.
    """
    return {
        "mode": o["mode"],
        "content": "\n".join(o["content"]),
        "code": {"language": "plaintext", "showLineNumbers": False, "showMiniMap": False},
    }


def panel_out(p, y0, source_uid, source_kind=_clickhouse_kind):
    ptype = p["type"]
    g = p["grid"]
    out = {
        "id": p["id"],
        "type": GRAFANA_TYPE.get(ptype, ptype),
        "title": p["title"],
        "description": p["description"],
        "transparent": bool(p["transparent"]),
        "gridPos": {"h": g["h"], "w": g["w"], "x": g["x"], "y": y0 + g["y"]},
        "pluginVersion": GRAFANA_VERSION,
    }
    if ptype == "text":
        # A text panel has no query, no transform and no field option (4.33a): it has no `targets` and no
        # `datasource`.
        out["fieldConfig"] = {"defaults": {}, "overrides": []}
        out["options"] = text_options_out(p["options"])
        out["links"] = links_out(p["links"])
        return out
    data = p["data"]
    if "query" in data:
        q = data["query"]
        queries = queries_of(data)
        sources = []
        for item in queries:
            if item["source"] not in sources:
                sources.append(item["source"])
        # A panel whose queries have more than one source has the data source `-- Mixed --`, and each target
        # names its own data source (15.14c). Else the data source of the panel is the data source of the targets.
        if len(sources) > 1:
            out["datasource"] = dict(MIXED_DS)
        else:
            out["datasource"] = ds(source_uid(sources[0]), source_kind(sources[0]))
        out["targets"] = [
            target_out(
                source_kind(item["source"]),
                source_uid(item["source"]),
                item["format"],
                sql_text(item["sql"]),
                item["ref"],
                item["hide"],
            )
            for item in queries
        ]
        if q.get("min_interval"):
            out["interval"] = q["min_interval"]
        if q.get("max_data_points") is not None:
            out["maxDataPoints"] = q["max_data_points"]
    else:
        r = data["reuse"]
        out["datasource"] = dict(DASHBOARD_DS)
        out["targets"] = [
            {
                "datasource": dict(DASHBOARD_DS),
                "panelId": r["panel"],
                "refId": "A",
                "withTransforms": bool(r["with_transforms"]),
            }
        ]
    defaults, custom = defaults_out(p["fields"]["defaults"])
    options, opt_custom = options_out(ptype, p["options"])
    custom.update(opt_custom)
    if custom:
        defaults["custom"] = custom
    out["fieldConfig"] = {"defaults": defaults, "overrides": overrides_out(p["fields"]["overrides"])}
    out["options"] = options
    out["links"] = links_out(p["links"])
    out["transformations"] = transforms_out(p["transforms"])
    out.update(repeat_out(p.get("repeat")))
    return out


def repeat_out(r):
    """The members of the repeat of a panel (4.13a, 15.6): `repeat`, `repeatDirection` and `maxPerRow`."""
    if not r:
        return {}
    out = {"repeat": r["variable"], "repeatDirection": r["direction"]}
    if r.get("max_per_row") is not None:
        out["maxPerRow"] = r["max_per_row"]
    return out


def section_height(section):
    return max((p["grid"]["y"] + p["grid"]["h"] for p in section["panels"]), default=0)


def layout_out(doc, source_uid, source_kind=_clickhouse_kind):
    """Sections to a flat panel list with row panels (the v1 form)."""
    panels = []
    used = {p["id"] for s in doc["sections"] for p in s["panels"]}
    next_id = max(used, default=0) + 1
    y = 0
    for s in doc["sections"]:
        if s["title"] is None:
            panels.extend(panel_out(p, y, source_uid, source_kind) for p in s["panels"])
            y += section_height(s)
            continue
        row = {
            "type": "row",
            "id": next_id,
            "title": s["title"],
            "collapsed": bool(s["collapsed"]),
            "gridPos": {"h": 1, "w": 24, "x": 0, "y": y},
            "panels": [],
        }
        if s.get("repeat"):
            row["repeat"] = s["repeat"]["variable"]
        next_id += 1
        inner = [panel_out(p, y + 1, source_uid, source_kind) for p in s["panels"]]
        if s["collapsed"]:
            row["panels"] = inner
            panels.append(row)
            y += 1
        else:
            panels.append(row)
            panels.extend(inner)
            y += 1 + section_height(s)
    return panels


# ------------------------------------------------------------------ variables


def _current(var):
    """The `current` member: the default of the document."""
    default = var["default"]
    kind = var["kind"]
    texts = {}
    if kind == "custom":
        texts = {o["value"]: o["text"] for o in var["options"]}
    multi = var["multi"]
    if default == ["$__all"]:
        text, value = ("All", "$__all")
        return {"selected": True, "text": [text] if multi else text, "value": [value] if multi else value}
    if not default:
        return {}
    shown = [texts.get(v, v) for v in default]
    if multi:
        return {"selected": True, "text": shown, "value": list(default)}
    return {"selected": True, "text": shown[0], "value": default[0]}


def _escape_custom(text):
    return text.replace("\\", "\\\\").replace(",", "\\,")


def _interval_out(v, out):
    """An `interval` variable (4.10a) as a variable of the type `interval` (15.6).

    The query is the list with commas. `auto`, `auto_count` and `auto_min` keep their names. The option
    "Auto" has the value `$__auto`. A `default` of `null` gives no `current`: Grafana takes the first option.
    """
    default = v["default"]
    options = []
    if v["auto"]:
        options.append({"selected": default == "$__auto", "text": "Auto", "value": "$__auto"})
    options += [{"selected": i == default, "text": i, "value": i} for i in v["intervals"]]
    if default is None:
        current = {}
    else:
        current = {"selected": True, "text": "Auto" if default == "$__auto" else default, "value": default}
    out.update(
        {
            "type": "interval",
            "query": ",".join(v["intervals"]),
            "auto": bool(v["auto"]),
            "auto_count": v["auto_count"],
            "auto_min": v["auto_min"],
            "refresh": 2,
            "options": options,
            "current": current,
        }
    )
    return out


def variable_out(v, source_uid, source_kind=_clickhouse_kind):
    kind = v["kind"]
    out = {
        "name": v["name"],
        "label": v["label"] or "",
        "hide": HIDE[v["hide"]],
        "skipUrlSync": False,
    }
    if kind == "text":
        out.update(
            {
                "type": "textbox",
                "query": v["default"],
                "current": {"selected": False, "text": v["default"], "value": v["default"]},
                "options": [{"selected": True, "text": v["default"], "value": v["default"]}],
            }
        )
        return out
    if kind == "interval":
        return _interval_out(v, out)
    if kind == "constant":
        # Grafana shows a constant for `hide` 0 and 1. A constant of the document has `hide` 2 (4.10b). The
        # stored `current` and `options` only repeat the value, so the converter does not write them.
        out.update({"type": "constant", "query": v["value"]})
        return out
    out["multi"] = bool(v["multi"])
    out["includeAll"] = bool(v["include_all"])
    # Grafana reads an absent `allowCustomValue` as true (19.20h), so the member is always written: the control
    # of the capture then has the same behaviour as the control of Hune (15.14b).
    out["allowCustomValue"] = bool(v.get("allow_custom_value", False))
    if v["all_value"] is not None:
        out["allValue"] = v["all_value"]
    if kind == "query":
        uid = source_uid(v["source"])
        text = sql_text(v["sql"])
        if source_kind(v["source"]) == "postgres":
            # The PostgreSQL plugin stores the query of a variable as the SQL text.
            out.update(
                {
                    "type": "query",
                    "datasource": ds(uid, "postgres"),
                    "definition": text,
                    "query": text,
                    "refresh": REFRESH[v["refresh"]],
                    "sort": SORT[v["sort"]],
                    "regex": v["regex"] or "",
                    "options": [],
                    "current": _current(v),
                }
            )
            return out
        out.update(
            {
                "type": "query",
                "datasource": ds(uid),
                "definition": text,
                "query": {
                    "editorType": "sql",
                    "format": 1,
                    "pluginVersion": PLUGIN_VERSION,
                    "queryType": "table",
                    "rawSql": text,
                    "refId": "variable",
                },
                "refresh": REFRESH[v["refresh"]],
                "sort": SORT[v["sort"]],
                "regex": v["regex"] or "",
                "options": [],
                "current": _current(v),
            }
        )
        return out
    parts = []
    for o in v["options"]:
        if o["text"] == o["value"]:
            parts.append(_escape_custom(o["value"]))
        else:
            parts.append("%s : %s" % (_escape_custom(o["text"]), _escape_custom(o["value"])))
    selected = set(v["default"])
    options = [{"selected": o["value"] in selected, "text": o["text"], "value": o["value"]} for o in v["options"]]
    if v["include_all"]:
        options.insert(0, {"selected": "$__all" in selected, "text": "All", "value": "$__all"})
    out.update({"type": "custom", "query": ", ".join(parts), "options": options, "current": _current(v)})
    return out


# ------------------------------------------------------------------ annotations


def annotation_out(a, source_uid, source_kind=_clickhouse_kind):
    """One annotation (4.5b) as an item of `annotations.list` (15.6, 15.14c).

    The item has the members that Grafana 12.4.2 writes for a query annotation: `datasource`, `enable`,
    `hide` equal to `false`, `iconColor`, `name` and `target`, and `filter` when the annotation names panels.
    The target is the query of a panel with the format `table` and the ref id `Anno`. The colour keeps its
    text: Grafana reads a token, a hex colour and an rgb colour as Hune does. It draws the tokens `text` and
    `transparent` as an unknown name (19.20e).
    """
    kind = source_kind(a["source"])
    uid = source_uid(a["source"])
    out = {
        "datasource": ds(uid, kind),
        "enable": a.get("enabled", True),
        "hide": False,
        "iconColor": a["color"],
        "name": a["name"],
        "target": target_out(kind, uid, "table", sql_text(a["sql"]), "Anno"),
    }
    if "panels" in a:
        out["filter"] = {"exclude": False, "ids": list(a["panels"])}
    return out


BUILTIN_ANNOTATION = {
    "builtIn": 1,
    "datasource": {"type": "grafana", "uid": "-- Grafana --"},
    "enable": True,
    "hide": True,
    "iconColor": "rgba(0, 211, 255, 1)",
    "name": "Annotations & Alerts",
    "type": "dashboard",
}


# ------------------------------------------------------------------ dashboard


def convert(doc, source_map=None, source_kinds=None, warnings=None):
    """Convert one document. `source_map` maps a Hune source id to a Grafana data source uid.

    A source id that the map does not name gets the uid `clickhouse`. `source_kinds` maps a Hune
    source id to its kind, `clickhouse` or `postgres`. A source id that it does not name has the kind
    `clickhouse`. The member `placement` of a variable has no Grafana form (4.7a): the converter drops it,
    keeps the variable, and adds one text to the list `warnings`, when the caller gives one.
    """
    source_map = source_map or {}
    source_kinds = source_kinds or {}

    def source_uid(source):
        return source_map.get(source, DEFAULT_DS_UID)

    def source_kind(source):
        return source_kinds.get(source, "clickhouse")

    for v in doc["variables"]:
        if "placement" in v and warnings is not None:
            warnings.append(
                "the placement of the variable %s is dropped: Grafana has no such member; "
                "the variable stays in the variables of the dashboard" % v["name"]
            )
    t = doc["time"]
    return {
        "annotations": {
            "list": [dict(BUILTIN_ANNOTATION)] + [annotation_out(a, source_uid, source_kind) for a in doc.get("annotations", [])]
        },
        "description": doc["description"],
        "editable": True,
        "fiscalYearStartMonth": 0,
        "graphTooltip": 1 if doc["cursor_sync"] == "crosshair" else 0,
        "links": [],
        "liveNow": False,
        "panels": layout_out(doc, source_uid, source_kind),
        "refresh": t["refresh"] or "",
        "schemaVersion": SCHEMA_VERSION,
        "tags": list(doc["tags"]),
        "templating": {"list": [variable_out(v, source_uid, source_kind) for v in doc["variables"]]},
        "time": {"from": t["from"], "to": t["to"]},
        "timepicker": {},
        "timezone": t["timezone"],
        "title": doc["title"],
        "uid": doc["uid"],
        "version": 1,
        "weekStart": "",
    }


def dump(obj):
    return json.dumps(obj, indent=2, ensure_ascii=False) + "\n"


def _read(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def convert_volume(volume, out_dir, skip, source_map, source_kinds=None, warnings=None):
    """Convert each document of a volume to OUT/<folder title>/<uid>.json. Returns the written paths.

    Each text of `warnings` starts with the uid of its document."""
    written = []
    for folder in sorted(os.listdir(volume)):
        folder_dir = os.path.join(volume, folder)
        meta = os.path.join(folder_dir, "_folder.json")
        if not os.path.isdir(folder_dir) or not os.path.exists(meta):
            continue
        title = _read(meta)["title"]
        for name in sorted(os.listdir(folder_dir)):
            if name == "_folder.json" or not name.endswith(".json"):
                continue
            doc = _read(os.path.join(folder_dir, name))
            if doc["uid"] in skip:
                continue
            target_dir = os.path.join(out_dir, title)
            os.makedirs(target_dir, exist_ok=True)
            target = os.path.join(target_dir, doc["uid"] + ".json")
            with open(target, "w", encoding="utf-8") as f:
                own = []
                f.write(dump(convert(doc, source_map, source_kinds, own)))
            if warnings is not None:
                warnings.extend("%s: %s" % (doc["uid"], w) for w in own)
            written.append(target)
    return written


def report(warnings):
    """Write each warning to the standard error."""
    for w in warnings:
        print("warning: " + w, file=sys.stderr)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("input", nargs="?", help="a Hune document (format 2)")
    ap.add_argument("output", nargs="?", help="the Grafana file (default: standard output)")
    ap.add_argument("--volume", help="convert each document of this volume")
    ap.add_argument("--out", help="with --volume: the output directory (one folder for each Hune folder)")
    ap.add_argument("--skip", action="append", default=[], help="with --volume: a uid to leave out")
    ap.add_argument(
        "--source-map",
        action="append",
        default=[],
        metavar="SOURCE=UID",
        help="a Hune source id and a Grafana data source uid (default uid: clickhouse)",
    )
    ap.add_argument(
        "--source-kind",
        action="append",
        default=[],
        metavar="SOURCE=KIND",
        help="a Hune source id and its kind, clickhouse or postgres (default kind: clickhouse)",
    )
    args = ap.parse_args(argv)
    source_kinds = {}
    for item in args.source_kind:
        key, _, kind = item.partition("=")
        if not key or kind not in ("clickhouse", "postgres"):
            ap.error("--source-kind needs SOURCE=clickhouse or SOURCE=postgres")
        source_kinds[key] = kind
    source_map = {}
    for item in args.source_map:
        key, _, uid = item.partition("=")
        if not key or not uid:
            ap.error("--source-map needs SOURCE=UID")
        source_map[key] = uid
    if args.volume:
        if not args.out or args.input:
            ap.error("--volume needs --out and no input file")
        warnings = []
        for path in convert_volume(args.volume, args.out, set(args.skip), source_map, source_kinds, warnings):
            print(path)
        report(warnings)
        return 0
    if not args.input:
        ap.error("give an input file or --volume")
    doc = _read(args.input)
    warnings = []
    text = dump(convert(doc, source_map, source_kinds, warnings))
    report(warnings)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
