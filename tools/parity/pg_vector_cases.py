# SPDX-License-Identifier: MIT OR Apache-2.0
"""The cases of the PostgreSQL expansion vectors (16.13, case I22).

Each case has an id, a template, tags and the inputs of one query. The module gives the list
`cases`. The differences between Hune and Grafana are not here. They are in
`vector-differences.json`, with the same case id.

A case with `mode` equal to `api` goes to `POST /api/ds/query` with the SQL that the case gives, after
the replacement of its variables (`values_sql`). A case with `mode` equal to `explore` goes through the
front end of Grafana, because the front end computes the built-in `$__interval`.
"""
GRAFANA_VERSION = "12.4.2"
PLUGIN = "grafana-postgresql-datasource (core)"
F0, T0 = 1794211200000, 1794232800000
def C(id, template, tags, **kw):
    d = dict(id=id, template=template, tags=tags, from_ms=F0, to_ms=T0, mdp=1200, interval_ms=20000, mode="api",
             variables=[], values={}, values_sql=None, sent=None)
    d.update(kw); return d
cases = []
a = cases.append
a(C("pg-macro-time-basic", "SELECT $__time(ts), v FROM t", ["macro:$__time"]))
a(C("pg-macro-time-nested-call", "SELECT $__time(coalesce(ts, now())) FROM t", ["macro:$__time"]))
a(C("pg-macro-timeEpoch-basic", "SELECT $__timeEpoch(ts), v FROM t", ["macro:$__timeEpoch"]))
a(C("pg-macro-timeFilter-basic", "SELECT v FROM t WHERE $__timeFilter(ts)", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFilter-blanks", "SELECT v FROM t WHERE $__timeFilter(  ts  )", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFilter-qualified", "SELECT v FROM t WHERE $__timeFilter(t.ts)", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFilter-two-uses", "SELECT v FROM t WHERE $__timeFilter(ts) AND $__timeFilter(ts)", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFilter-range-fractions", "SELECT v FROM t WHERE $__timeFilter(ts)", ["macro:$__timeFilter", "time:ms"], from_ms=1794211200125, to_ms=1794232800999))
a(C("pg-macro-timeFilter-range-trailing-zero", "SELECT v FROM t WHERE $__timeFilter(ts)", ["macro:$__timeFilter", "time:ms"], from_ms=1794211200100, to_ms=1794232800120))
a(C("pg-macro-timeFilter-range-one-ms", "SELECT v FROM t WHERE $__timeFilter(ts)", ["macro:$__timeFilter", "time:ms"], from_ms=0, to_ms=1, interval_ms=1))
a(C("pg-macro-timeFilter-nested-call", "SELECT v FROM t WHERE $__timeFilter(date_trunc('day', ts))", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFilter-no-argument", "SELECT v FROM t WHERE $__timeFilter()", ["macro:$__timeFilter"]))
a(C("pg-macro-timeFrom-call", "SELECT v FROM t WHERE ts > $__timeFrom()", ["macro:$__timeFrom"]))
a(C("pg-macro-timeFrom-blanks-in-call", "SELECT v FROM t WHERE ts > $__timeFrom( )", ["macro:$__timeFrom"]))
a(C("pg-macro-timeFrom-cast", "SELECT v FROM t WHERE ts > $__timeFrom()::timestamptz", ["macro:$__timeFrom"]))
a(C("pg-macro-timeFrom-bare", "SELECT v FROM t WHERE ts > $__timeFrom", ["macro:$__timeFrom"]))
a(C("pg-macro-timeFrom-range-fractions", "SELECT $__timeFrom() AS f", ["macro:$__timeFrom", "time:ms"], from_ms=1794211200125, to_ms=1794232800999))
a(C("pg-macro-timeTo-call", "SELECT v FROM t WHERE ts <= $__timeTo()", ["macro:$__timeTo"]))
a(C("pg-macro-timeTo-bare", "SELECT v FROM t WHERE ts <= $__timeTo", ["macro:$__timeTo"]))
a(C("pg-macro-timeTo-range-fractions", "SELECT $__timeTo() AS t2", ["macro:$__timeTo", "time:ms"], from_ms=1794211200125, to_ms=1794232800999))
a(C("pg-macro-timeFrom-with-argument", "SELECT $__timeFrom( 1 ) FROM t", ["macro:$__timeFrom"]))
# group macros
a(C("pg-macro-timeGroup-5m", "SELECT $__timeGroup(ts, '5m') AS t FROM t", ["macro:$__timeGroup", "interval:5m"]))
a(C("pg-macro-timeGroup-tight-blanks", "SELECT $__timeGroup(ts,'5m') AS t FROM t", ["macro:$__timeGroup", "interval:5m"]))
a(C("pg-macro-timeGroupAlias-1h", "SELECT $__timeGroupAlias(ts, '1h'), avg(v) FROM t GROUP BY 1", ["macro:$__timeGroupAlias", "interval:1h"]))
a(C("pg-macro-timeGroup-comma-in-argument", "SELECT $__timeGroup(coalesce(ts, now()), '5m') FROM t", ["macro:$__timeGroup"]))
for unit, text, g in [("ms-1", "1ms", "0.001"), ("ms-500", "500ms", "0.5"), ("s-1", "1s", "1"), ("m-5", "5m", "300"), ("h-1", "1h", "3600"),
                      ("d-1", "1d", "86400"), ("w-1", "1w", "604800"), ("m-90", "90m", "5400")]:
    a(C(f"pg-interval-unit-{unit}", f"SELECT $__timeGroup(ts, '{text}') FROM t", ["macro:$__timeGroup", f"interval:{text}"]))
a(C("pg-interval-unit-d-365", "SELECT $__timeGroup(ts, '365d') FROM t", ["macro:$__timeGroup", "interval:365d"]))
a(C("pg-interval-unit-M-1", "SELECT $__timeGroup(ts, '1M') FROM t", ["macro:$__timeGroup", "interval:1M"]))
a(C("pg-interval-unit-y-1", "SELECT $__timeGroup(ts, '1y') FROM t", ["macro:$__timeGroup", "interval:1y"]))
for id, text, why, sql in [
    ("two-units", "1h30m", "Grafana reads two units. Hune accepts one whole number and one unit.", None),
    ("decimal", "1.5s", "Grafana reads a decimal number. Hune accepts a whole number only.", None),
    ("leading-zero", "05m", "Grafana reads a zero at the start. Hune refuses it.", None),
    ("negative", "-5m", "Grafana reads a negative interval. Hune accepts no sign.", None),
    ("zero", "0s", "Grafana gives a division by zero at run time. Hune refuses an interval of 0.", None),
    ("over-365d", "2y", "Grafana has no upper limit. Hune accepts 365 days at most.", None),
    ("blank-padded", " 5m ", "Grafana refuses blanks inside the string. Hune also refuses them, with another text.", None),
]:
    a(C(f"pg-interval-refused-{id}", f"SELECT $__timeGroup(ts, '{text}') FROM t", ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-interval-refused-empty", "SELECT $__timeGroup(ts, '') FROM t", ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-interval-refused-unknown-unit", "SELECT $__timeGroup(ts, '10x') FROM t", ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-interval-refused-number", "SELECT $__timeGroup(ts, 300) FROM t", ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-interval-refused-unquoted", "SELECT $__timeGroup(ts, 5m) FROM t", ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-interval-refused-double-quote", 'SELECT $__timeGroup(ts, "5m") FROM t', ["macro:$__timeGroup", "interval:refused"]))
a(C("pg-macro-timeGroup-fill-value", "SELECT $__timeGroup(ts, '5m', 0) FROM t", ["macro:$__timeGroup"]))
a(C("pg-macro-timeGroup-one-argument", "SELECT $__timeGroup(ts) FROM t", ["macro:$__timeGroup"]))
# built-in interval (the front end of Grafana replaces it)
a(C("pg-interval-builtin-plain", "SELECT $__timeGroup(ts, $__interval) FROM t", ["macro:$__timeGroup", "builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-braces", "SELECT $__timeGroup(ts, ${__interval}) FROM t", ["macro:$__timeGroup", "builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-quoted", "SELECT $__timeGroup(ts, '$__interval') FROM t", ["macro:$__timeGroup", "builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-unixepoch", "SELECT $__unixEpochGroup(n, $__interval) FROM t", ["macro:$__unixEpochGroup", "builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-alias", "SELECT $__timeGroupAlias(ts, $__interval), count(*) FROM t GROUP BY 1", ["macro:$__timeGroupAlias", "builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-in-string", "SELECT '$__interval' AS i FROM t", ["builtin:$__interval"], mode="explore"))
a(C("pg-interval-builtin-in-code", "SELECT $__interval AS i FROM t", ["builtin:$__interval"], mode="explore"))
a(C("pg-builtin-interval-ms", "SELECT $__interval_ms AS i FROM t", ["builtin:$__interval_ms"], mode="explore"))
a(C("pg-builtin-from-to", "SELECT $__from AS f, $__to AS t2 FROM t", ["builtin:$__from", "builtin:$__to"], mode="explore"))
a(C("pg-builtin-interval-s", "SELECT $__interval_s AS s FROM t", ["builtin:$__interval_s"]))
# unix epoch
a(C("pg-macro-unixEpochFilter-basic", "SELECT v FROM t WHERE $__unixEpochFilter(n)", ["macro:$__unixEpochFilter"]))
a(C("pg-macro-unixEpochFrom-call", "SELECT v FROM t WHERE n > $__unixEpochFrom()", ["macro:$__unixEpochFrom"]))
a(C("pg-macro-unixEpochTo-call", "SELECT v FROM t WHERE n < $__unixEpochTo()", ["macro:$__unixEpochTo"]))
a(C("pg-macro-unixEpochFrom-range-fractions", "SELECT $__unixEpochFrom() AS f, $__unixEpochTo() AS t2", ["macro:$__unixEpochFrom", "time:ms"], from_ms=1794211200999, to_ms=1794232800001))
a(C("pg-macro-unixEpochFrom-bare", "SELECT v FROM t WHERE n > $__unixEpochFrom", ["macro:$__unixEpochFrom"]))
a(C("pg-macro-unixEpochNanoFilter-basic", "SELECT v FROM t WHERE $__unixEpochNanoFilter(n)", ["macro:$__unixEpochNanoFilter"]))
a(C("pg-macro-unixEpochNanoFrom-call", "SELECT v FROM t WHERE n > $__unixEpochNanoFrom()", ["macro:$__unixEpochNanoFrom"]))
a(C("pg-macro-unixEpochNanoTo-call", "SELECT v FROM t WHERE n < $__unixEpochNanoTo()", ["macro:$__unixEpochNanoTo"]))
a(C("pg-macro-unixEpochNanoFrom-range-fractions", "SELECT $__unixEpochNanoFrom() AS f, $__unixEpochNanoTo() AS t2", ["macro:$__unixEpochNanoFrom", "time:ms"], from_ms=1794211200123, to_ms=1794232800987))
a(C("pg-macro-unixEpochGroup-5m", "SELECT $__unixEpochGroup(n, '5m') AS t FROM t", ["macro:$__unixEpochGroup", "interval:5m"]))
a(C("pg-macro-unixEpochGroupAlias-5m", "SELECT $__unixEpochGroupAlias(n, '5m'), sum(v) FROM t GROUP BY 1", ["macro:$__unixEpochGroupAlias", "interval:5m"]))
a(C("pg-macro-unixEpochGroup-expression", "SELECT $__unixEpochGroup(n + 1, '5m') AS t FROM t", ["macro:$__unixEpochGroup", "interval:5m"]))
a(C("pg-macro-unixEpochGroup-fill-value", "SELECT $__unixEpochGroup(n, '5m', 0) FROM t", ["macro:$__unixEpochGroup"]))
# places
a(C("pg-macro-in-string", "SELECT 'a$__unixEpochFrom()b' AS s FROM t", ["macro:$__unixEpochFrom"]))
a(C("pg-macro-in-block-comment", "SELECT v FROM t /* $__unixEpochFrom() */ WHERE $__timeFilter(ts)", ["macro:$__unixEpochFrom"]))
a(C("pg-macro-in-quoted-name", 'SELECT 1 AS "$__timeFrom()" FROM t', ["macro:$__timeFrom"]))
a(C("pg-unknown-macro-lowercase", "SELECT v FROM t WHERE $__timefilter(ts)", ["unknown"]))
a(C("pg-macro-in-line-comment", "SELECT v FROM t -- $__unixEpochFrom()\nWHERE $__timeFilter(ts)", ["macro:$__unixEpochFrom"]))
for nm, call in [("timeFilter_ms", "$__timeFilter_ms(ts)"), ("timeInterval", "$__timeInterval(ts)"), ("dateFilter", "$__dateFilter(ts)"),
                 ("fromTime", "$__fromTime"), ("dateTimeFilter", "$__dateTimeFilter(ts, ts)"), ("toTime", "$__toTime")]:
    a(C(f"pg-unknown-macro-{nm}", f"SELECT {call} FROM t", ["unknown"]))
a(C("pg-macro-conditionalAll-all", "SELECT v FROM t WHERE $__conditionalAll(ts > $__timeFrom(), $host)", ["macro:$__conditionalAll"],
    variables=[dict(name="host", kind="query", options=["a1", "b2"], multi=True, include_all=True, all_value=None)],
    values={"host": {"all": True, "values": []}}))
a(C("pg-macro-conditionalAll-one", "SELECT v FROM t WHERE $__conditionalAll(host IN ($host), $host)", ["macro:$__conditionalAll"],
    variables=[dict(name="host", kind="query", options=["a1", "b2"], multi=True, include_all=True, all_value=None)],
    values={"host": {"all": False, "values": ["a1"]}}, values_sql={"host": "'a1'"}))
# worked example of 7.22
WORKED = ("SELECT $__timeGroupAlias(created_at, $__interval), host, avg(load) AS load\nFROM metrics.node_load\n"
          "WHERE $__timeFilter(created_at) AND host IN ($host) AND env = '$env'\nGROUP BY 1, host\nORDER BY 1")
a(C("pg-worked-7-22", WORKED, ["worked:7.22", "macro:$__timeGroupAlias", "macro:$__timeFilter", "builtin:$__interval"],
    variables=[dict(name="host", kind="query", options=["a1", "b'2"], multi=True, include_all=False, all_value=None),
               dict(name="env", kind="custom", options=["prod", "dev"], multi=False, include_all=False, all_value=None)],
    values={"host": {"all": False, "values": ["a1", "b'2"]}, "env": {"all": False, "values": ["prod"]}},
    values_sql={"host": "'a1','b''2'", "env": "prod"}))
