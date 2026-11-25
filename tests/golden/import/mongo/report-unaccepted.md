# Import report

## Input

| Name | Value |
|---|---|
| Input directory | `/private/grafana-export-mongo` |
| Grafana version | unknown |
| ClickHouse plugin version | unknown |
| Mode | `dry_run` |

## Totals

| Total | Count |
|---|---|
| dashboards | 4 |
| panels | 72 |
| mapped | 438 |
| changed | 28 |
| dropped | 0 |
| unsupported | 36 |
| metadata | 301 |
| accepted | 0 |
| unaccepted | 64 |
| stale_accept_lines | 0 |

## Objects that are not imported

- History: not in the export.
- Public dashboards: 0.
- Short links: 0.

## Dashboards

### `mongo-main`: MongoDB panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `mongo-main` | no folder | 18 | 23 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `mongo-main:/panels/10/targets/0/queryText` | unsupported | 11: Placeholder in a key | `targets.0.queryText` | MongoDB query: line 4, column 16: /aggregate/0/$match/${node}: a placeholder is not permitted in a key (7.34) | panel removed | no |
| `mongo-main:/panels/11/targets/0/queryType` | unsupported | 12: Another query type | `targets.0.queryType` | the queryType of a MongoDB query must be the text table or timeseries | panel removed | no |
| `mongo-main:/panels/12/targets/0/queryText` | changed | 13: Old form | `targets.0.queryText` | the old form [[name]] is rewritten to ${name} for: region |  | no |
| `mongo-main:/panels/13/targets/0/queryText` | changed | 14: Macro in a string | `targets.0.queryText` | MongoDB query: line 4, column 23: /aggregate/0/$match/note: $__timeFrom stays text in a longer string |  | no |
| `mongo-main:/panels/14/targets/0/datasource` | unsupported | 15: Unmapped data source | `targets.0.datasource` | the data source unmapped-mongo is not in the source map | panel removed | no |
| `mongo-main:/panels/15/targets/0/queryText` | unsupported | 16: Find object | `targets.0.queryText` | queryText must be one JSON array of stages: another form has no row in 15.6 | panel removed | no |
| `mongo-main:/panels/16/targets/0/rawSql` | unsupported | 17: SQL member | `targets.0.rawSql` | unknown member | member removed | no |
| `mongo-main:/panels/3/targets/0/queryType` | changed | 4: No query type | `targets.0.queryType` | the queryType is absent: the format is table |  | no |
| `mongo-main:/panels/4/targets/0/queryText` | changed | 5: Wrapper | `targets.0.queryText` | 2 wrapper(s) of $__from or $__to are written as the macros $__timeFrom and $__timeTo (7.34) |  | no |
| `mongo-main:/panels/5/targets/0/queryText` | changed | 6: Bare token | `targets.0.queryText` | 2 bare token(s) $__from or $__to are written as a string: Hune gives the Int64 as a typed value (7.34) |  | no |
| `mongo-main:/panels/6/targets/0/queryText` | unsupported | 7: Refused stage | `targets.0.queryText` | MongoDB query: line 5, column 6: /aggregate/1/$out: $out is not permitted in a MongoDB query of Hune: They write to a collection. | panel removed | no |
| `mongo-main:/panels/7/targets/0/queryText` | unsupported | 8: Lookup outside the list | `targets.0.queryText` | MongoDB query: line 4, column 24: /aggregate/0/$lookup/from: the collection is not in the key collections of the source (7.33) | panel removed | no |
| `mongo-main:/panels/8/targets/0/queryText` | unsupported | 9: Shell syntax | `targets.0.queryText` | queryText is not a JSON text of the profile of 7.28 (Hune reads no shell syntax and no JavaScript): an unexpected byte, line 1, column 1 | panel removed | no |
| `mongo-main:/panels/9/targets/0/queryText` | unsupported | 10: Binary form | `targets.0.queryText` | MongoDB query: line 4, column 21: /aggregate/0/$match/b/$binary: $binary is not permitted in a MongoDB query of Hune | panel removed | no |
| `mongo-main:/templating/list/0/allowCustomValue` | changed |  | `templating.list.0.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `mongo-main:/templating/list/1/allowCustomValue` | changed |  | `templating.list.1.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |

### `mongo-v2a`: MongoDB panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `mongo-v2a` | no folder | 18 | 119 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `mongo-v2a:/spec/elements/panel-10/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 10: Binary form | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 21: /aggregate/0/$match/b/$binary: $binary is not permitted in a MongoDB query of Hune | panel removed | no |
| `mongo-v2a:/spec/elements/panel-11/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 11: Placeholder in a key | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 16: /aggregate/0/$match/${node}: a placeholder is not permitted in a key (7.34) | panel removed | no |
| `mongo-v2a:/spec/elements/panel-12/spec/data/spec/queries/0/spec/query/spec/queryType` | unsupported | 12: Another query type | `queries.0.spec.query.spec.queryType` | the queryType of a MongoDB query must be the text table or timeseries | panel removed | no |
| `mongo-v2a:/spec/elements/panel-13/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 13: Old form | `queries.0.spec.query.spec.queryText` | the old form [[name]] is rewritten to ${name} for: region |  | no |
| `mongo-v2a:/spec/elements/panel-14/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 14: Macro in a string | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 23: /aggregate/0/$match/note: $__timeFrom stays text in a longer string |  | no |
| `mongo-v2a:/spec/elements/panel-15/spec/data/spec/queries/0/spec/datasource` | unsupported | 15: Unmapped data source | `queries.0.spec.datasource` | the data source unmapped-mongo is not in the source map | panel removed | no |
| `mongo-v2a:/spec/elements/panel-16/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 16: Find object | `queries.0.spec.query.spec.queryText` | queryText must be one JSON array of stages: another form has no row in 15.6 | panel removed | no |
| `mongo-v2a:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: SQL member | `queries.0.spec.query.spec.rawSql` | unknown member | member removed | no |
| `mongo-v2a:/spec/elements/panel-4/spec/data/spec/queries/0/queryType` | changed | 4: No query type | `queries.0.queryType` | the queryType is absent: the format is table |  | no |
| `mongo-v2a:/spec/elements/panel-5/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 5: Wrapper | `queries.0.spec.query.spec.queryText` | 2 wrapper(s) of $__from or $__to are written as the macros $__timeFrom and $__timeTo (7.34) |  | no |
| `mongo-v2a:/spec/elements/panel-6/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 6: Bare token | `queries.0.spec.query.spec.queryText` | 2 bare token(s) $__from or $__to are written as a string: Hune gives the Int64 as a typed value (7.34) |  | no |
| `mongo-v2a:/spec/elements/panel-7/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 7: Refused stage | `queries.0.spec.query.spec.queryText` | MongoDB query: line 5, column 6: /aggregate/1/$out: $out is not permitted in a MongoDB query of Hune: They write to a collection. | panel removed | no |
| `mongo-v2a:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 8: Lookup outside the list | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 24: /aggregate/0/$lookup/from: the collection is not in the key collections of the source (7.33) | panel removed | no |
| `mongo-v2a:/spec/elements/panel-9/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 9: Shell syntax | `queries.0.spec.query.spec.queryText` | queryText is not a JSON text of the profile of 7.28 (Hune reads no shell syntax and no JavaScript): an unexpected byte, line 1, column 1 | panel removed | no |
| `mongo-v2a:/spec/variables/0/spec/allowCustomValue` | changed |  | `spec.variables.0.spec.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `mongo-v2a:/spec/variables/1/spec/allowCustomValue` | changed |  | `spec.variables.1.spec.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |

### `mongo-v2b`: MongoDB panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `mongo-v2b` | no folder | 18 | 129 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `mongo-v2b:/spec/elements/panel-10/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 10: Binary form | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 21: /aggregate/0/$match/b/$binary: $binary is not permitted in a MongoDB query of Hune | panel removed | no |
| `mongo-v2b:/spec/elements/panel-11/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 11: Placeholder in a key | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 16: /aggregate/0/$match/${node}: a placeholder is not permitted in a key (7.34) | panel removed | no |
| `mongo-v2b:/spec/elements/panel-12/spec/data/spec/queries/0/spec/query/spec/queryType` | unsupported | 12: Another query type | `queries.0.spec.query.spec.queryType` | the queryType of a MongoDB query must be the text table or timeseries | panel removed | no |
| `mongo-v2b:/spec/elements/panel-13/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 13: Old form | `queries.0.spec.query.spec.queryText` | the old form [[name]] is rewritten to ${name} for: region |  | no |
| `mongo-v2b:/spec/elements/panel-14/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 14: Macro in a string | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 23: /aggregate/0/$match/note: $__timeFrom stays text in a longer string |  | no |
| `mongo-v2b:/spec/elements/panel-15/spec/data/spec/queries/0/spec/query/datasource` | unsupported | 15: Unmapped data source | `queries.0.spec.query.datasource` | the data source unmapped-mongo is not in the source map | panel removed | no |
| `mongo-v2b:/spec/elements/panel-16/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 16: Find object | `queries.0.spec.query.spec.queryText` | queryText must be one JSON array of stages: another form has no row in 15.6 | panel removed | no |
| `mongo-v2b:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: SQL member | `queries.0.spec.query.spec.rawSql` | unknown member | member removed | no |
| `mongo-v2b:/spec/elements/panel-4/spec/data/spec/queries/0/queryType` | changed | 4: No query type | `queries.0.queryType` | the queryType is absent: the format is table |  | no |
| `mongo-v2b:/spec/elements/panel-5/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 5: Wrapper | `queries.0.spec.query.spec.queryText` | 2 wrapper(s) of $__from or $__to are written as the macros $__timeFrom and $__timeTo (7.34) |  | no |
| `mongo-v2b:/spec/elements/panel-6/spec/data/spec/queries/0/spec/query/spec/queryText` | changed | 6: Bare token | `queries.0.spec.query.spec.queryText` | 2 bare token(s) $__from or $__to are written as a string: Hune gives the Int64 as a typed value (7.34) |  | no |
| `mongo-v2b:/spec/elements/panel-7/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 7: Refused stage | `queries.0.spec.query.spec.queryText` | MongoDB query: line 5, column 6: /aggregate/1/$out: $out is not permitted in a MongoDB query of Hune: They write to a collection. | panel removed | no |
| `mongo-v2b:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 8: Lookup outside the list | `queries.0.spec.query.spec.queryText` | MongoDB query: line 4, column 24: /aggregate/0/$lookup/from: the collection is not in the key collections of the source (7.33) | panel removed | no |
| `mongo-v2b:/spec/elements/panel-9/spec/data/spec/queries/0/spec/query/spec/queryText` | unsupported | 9: Shell syntax | `queries.0.spec.query.spec.queryText` | queryText is not a JSON text of the profile of 7.28 (Hune reads no shell syntax and no JavaScript): an unexpected byte, line 1, column 1 | panel removed | no |
| `mongo-v2b:/spec/variables/0/spec/allowCustomValue` | changed |  | `spec.variables.0.spec.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `mongo-v2b:/spec/variables/1/spec/allowCustomValue` | changed |  | `spec.variables.1.spec.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |

### `mongo-wrapped`: MongoDB panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `mongo-wrapped` | no folder | 18 | 30 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `mongo-wrapped:/dashboard/panels/10/targets/0/queryText` | unsupported | 11: Placeholder in a key | `targets.0.queryText` | MongoDB query: line 4, column 16: /aggregate/0/$match/${node}: a placeholder is not permitted in a key (7.34) | panel removed | no |
| `mongo-wrapped:/dashboard/panels/11/targets/0/queryType` | unsupported | 12: Another query type | `targets.0.queryType` | the queryType of a MongoDB query must be the text table or timeseries | panel removed | no |
| `mongo-wrapped:/dashboard/panels/12/targets/0/queryText` | changed | 13: Old form | `targets.0.queryText` | the old form [[name]] is rewritten to ${name} for: region |  | no |
| `mongo-wrapped:/dashboard/panels/13/targets/0/queryText` | changed | 14: Macro in a string | `targets.0.queryText` | MongoDB query: line 4, column 23: /aggregate/0/$match/note: $__timeFrom stays text in a longer string |  | no |
| `mongo-wrapped:/dashboard/panels/14/targets/0/datasource` | unsupported | 15: Unmapped data source | `targets.0.datasource` | the data source unmapped-mongo is not in the source map | panel removed | no |
| `mongo-wrapped:/dashboard/panels/15/targets/0/queryText` | unsupported | 16: Find object | `targets.0.queryText` | queryText must be one JSON array of stages: another form has no row in 15.6 | panel removed | no |
| `mongo-wrapped:/dashboard/panels/16/targets/0/rawSql` | unsupported | 17: SQL member | `targets.0.rawSql` | unknown member | member removed | no |
| `mongo-wrapped:/dashboard/panels/3/targets/0/queryType` | changed | 4: No query type | `targets.0.queryType` | the queryType is absent: the format is table |  | no |
| `mongo-wrapped:/dashboard/panels/4/targets/0/queryText` | changed | 5: Wrapper | `targets.0.queryText` | 2 wrapper(s) of $__from or $__to are written as the macros $__timeFrom and $__timeTo (7.34) |  | no |
| `mongo-wrapped:/dashboard/panels/5/targets/0/queryText` | changed | 6: Bare token | `targets.0.queryText` | 2 bare token(s) $__from or $__to are written as a string: Hune gives the Int64 as a typed value (7.34) |  | no |
| `mongo-wrapped:/dashboard/panels/6/targets/0/queryText` | unsupported | 7: Refused stage | `targets.0.queryText` | MongoDB query: line 5, column 6: /aggregate/1/$out: $out is not permitted in a MongoDB query of Hune: They write to a collection. | panel removed | no |
| `mongo-wrapped:/dashboard/panels/7/targets/0/queryText` | unsupported | 8: Lookup outside the list | `targets.0.queryText` | MongoDB query: line 4, column 24: /aggregate/0/$lookup/from: the collection is not in the key collections of the source (7.33) | panel removed | no |
| `mongo-wrapped:/dashboard/panels/8/targets/0/queryText` | unsupported | 9: Shell syntax | `targets.0.queryText` | queryText is not a JSON text of the profile of 7.28 (Hune reads no shell syntax and no JavaScript): an unexpected byte, line 1, column 1 | panel removed | no |
| `mongo-wrapped:/dashboard/panels/9/targets/0/queryText` | unsupported | 10: Binary form | `targets.0.queryText` | MongoDB query: line 4, column 21: /aggregate/0/$match/b/$binary: $binary is not permitted in a MongoDB query of Hune | panel removed | no |
| `mongo-wrapped:/dashboard/templating/list/0/allowCustomValue` | changed |  | `templating.list.0.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `mongo-wrapped:/dashboard/templating/list/1/allowCustomValue` | changed |  | `templating.list.1.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |

## Redirects

```nginx
# Written by hune import-grafana. Include in the http context.
map $hune_grafana_uid $hune_new_uid {
    default "";
}
map $hune_grafana_short $hune_short_target {
    default "";
}
```

## Exit code

Exit code: 1.
Reason: 64 item(s) need acceptance and have none, and 0 acceptance line(s) are stale.
