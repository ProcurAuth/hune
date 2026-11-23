# Import report

## Input

| Name | Value |
|---|---|
| Input directory | `/private/grafana-export-pg` |
| Grafana version | unknown |
| ClickHouse plugin version | unknown |
| Mode | `dry_run` |

## Totals

| Total | Count |
|---|---|
| dashboards | 4 |
| panels | 52 |
| mapped | 406 |
| changed | 8 |
| dropped | 0 |
| unsupported | 32 |
| metadata | 307 |
| accepted | 0 |
| unaccepted | 40 |
| stale_accept_lines | 0 |

## Objects that are not imported

- History: not in the export.
- Public dashboards: 0.
- Short links: 0.

## Dashboards

### `pg-main`: PostgreSQL panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `pg-main` | `general` | 13 | 33 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `pg-main:/panels/12/targets/0/datasource` | unsupported | 13: Data source not in the map | `targets.0.datasource` | the data source unmapped-pg is not in the source map | panel removed | no |
| `pg-main:/panels/2/targets/0/format` | changed | 3: Row count | `targets.0.format` | the format is absent: the plugin default is table |  | no |
| `pg-main:/panels/3/targets/0/format` | unsupported | 4: Logs format | `targets.0.format` | the format of a PostgreSQL query must be the text table or time_series | panel removed | no |
| `pg-main:/panels/4/targets/0/editorMode` | unsupported | 5: Builder with SQL | `targets.0.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | the rawSql stays as the SQL text, and the builder members are removed | no |
| `pg-main:/panels/5/targets/0/editorMode` | unsupported | 6: Builder without SQL | `targets.0.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | panel removed | no |
| `pg-main:/panels/6/targets/0/rawSql` | unsupported | 7: Gap fill value | `targets.0.rawSql` | 1 group macro(s) with a third argument: Hune does not fill the gaps of a series (7.22) | the third argument is removed, and the panel shows the gaps | no |
| `pg-main:/panels/7/targets/0/rawSql` | unsupported | 8: Macro of the other dialect | `targets.0.rawSql` | SQL: line 1, column 8: $__timeInterval is not a macro of Hune | panel removed | no |
| `pg-main:/panels/7/targets/0/rawSql` | unsupported | 8: Macro of the other dialect | `targets.0.rawSql` | SQL: line 1, column 77: $__timeFilter_ms is not a macro of Hune | panel removed | no |
| `pg-main:/panels/8/targets/0/rawSql` | unsupported | 9: Interval with two units | `targets.0.rawSql` | SQL: line 1, column 8: $__timeGroup: the interval string must be a whole number and one unit: ms, s, m, h, d, w, M or y | panel removed | no |
| `pg-main:/panels/9/targets/0/rawSql` | changed | 10: Macro in a string | `targets.0.rawSql` | SQL: line 1, column 9: $__timeFrom stays text inside a string or a quoted name: Grafana replaces it |  | no |

### `pg-v2a`: PostgreSQL panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `pg-v2a` | `general` | 13 | 112 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `pg-v2a:/spec/elements/panel-10/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 10: Macro in a string | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 9: $__timeFrom stays text inside a string or a quoted name: Grafana replaces it |  | no |
| `pg-v2a:/spec/elements/panel-13/spec/data/spec/queries/0/spec/datasource` | unsupported | 13: Data source not in the map | `queries.0.spec.datasource` | the data source unmapped-pg is not in the source map | panel removed | no |
| `pg-v2a:/spec/elements/panel-3/spec/data/spec/queries/0/format` | changed | 3: Row count | `queries.0.format` | the format is absent: the plugin default is table |  | no |
| `pg-v2a:/spec/elements/panel-4/spec/data/spec/queries/0/spec/query/spec/format` | unsupported | 4: Logs format | `queries.0.spec.query.spec.format` | the format of a PostgreSQL query must be the text table or time_series | panel removed | no |
| `pg-v2a:/spec/elements/panel-5/spec/data/spec/queries/0/spec/query/spec/editorMode` | unsupported | 5: Builder with SQL | `queries.0.spec.query.spec.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | the rawSql stays as the SQL text, and the builder members are removed | no |
| `pg-v2a:/spec/elements/panel-6/spec/data/spec/queries/0/spec/query/spec/editorMode` | unsupported | 6: Builder without SQL | `queries.0.spec.query.spec.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | panel removed | no |
| `pg-v2a:/spec/elements/panel-7/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 7: Gap fill value | `queries.0.spec.query.spec.rawSql` | 1 group macro(s) with a third argument: Hune does not fill the gaps of a series (7.22) | the third argument is removed, and the panel shows the gaps | no |
| `pg-v2a:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 8: Macro of the other dialect | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 8: $__timeInterval is not a macro of Hune | panel removed | no |
| `pg-v2a:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 8: Macro of the other dialect | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 77: $__timeFilter_ms is not a macro of Hune | panel removed | no |
| `pg-v2a:/spec/elements/panel-9/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 9: Interval with two units | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 8: $__timeGroup: the interval string must be a whole number and one unit: ms, s, m, h, d, w, M or y | panel removed | no |

### `pg-v2b`: PostgreSQL panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `pg-v2b` | `general` | 13 | 122 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `pg-v2b:/spec/elements/panel-10/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 10: Macro in a string | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 9: $__timeFrom stays text inside a string or a quoted name: Grafana replaces it |  | no |
| `pg-v2b:/spec/elements/panel-13/spec/data/spec/queries/0/spec/query/datasource` | unsupported | 13: Data source not in the map | `queries.0.spec.query.datasource` | the data source unmapped-pg is not in the source map | panel removed | no |
| `pg-v2b:/spec/elements/panel-3/spec/data/spec/queries/0/format` | changed | 3: Row count | `queries.0.format` | the format is absent: the plugin default is table |  | no |
| `pg-v2b:/spec/elements/panel-4/spec/data/spec/queries/0/spec/query/spec/format` | unsupported | 4: Logs format | `queries.0.spec.query.spec.format` | the format of a PostgreSQL query must be the text table or time_series | panel removed | no |
| `pg-v2b:/spec/elements/panel-5/spec/data/spec/queries/0/spec/query/spec/editorMode` | unsupported | 5: Builder with SQL | `queries.0.spec.query.spec.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | the rawSql stays as the SQL text, and the builder members are removed | no |
| `pg-v2b:/spec/elements/panel-6/spec/data/spec/queries/0/spec/query/spec/editorMode` | unsupported | 6: Builder without SQL | `queries.0.spec.query.spec.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | panel removed | no |
| `pg-v2b:/spec/elements/panel-7/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 7: Gap fill value | `queries.0.spec.query.spec.rawSql` | 1 group macro(s) with a third argument: Hune does not fill the gaps of a series (7.22) | the third argument is removed, and the panel shows the gaps | no |
| `pg-v2b:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 8: Macro of the other dialect | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 8: $__timeInterval is not a macro of Hune | panel removed | no |
| `pg-v2b:/spec/elements/panel-8/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 8: Macro of the other dialect | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 77: $__timeFilter_ms is not a macro of Hune | panel removed | no |
| `pg-v2b:/spec/elements/panel-9/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 9: Interval with two units | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 8: $__timeGroup: the interval string must be a whole number and one unit: ms, s, m, h, d, w, M or y | panel removed | no |

### `pg-wrapped`: PostgreSQL panels

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `pg-wrapped` | `general` | 13 | 40 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `pg-wrapped:/dashboard/panels/12/targets/0/datasource` | unsupported | 13: Data source not in the map | `targets.0.datasource` | the data source unmapped-pg is not in the source map | panel removed | no |
| `pg-wrapped:/dashboard/panels/2/targets/0/format` | changed | 3: Row count | `targets.0.format` | the format is absent: the plugin default is table |  | no |
| `pg-wrapped:/dashboard/panels/3/targets/0/format` | unsupported | 4: Logs format | `targets.0.format` | the format of a PostgreSQL query must be the text table or time_series | panel removed | no |
| `pg-wrapped:/dashboard/panels/4/targets/0/editorMode` | unsupported | 5: Builder with SQL | `targets.0.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | the rawSql stays as the SQL text, and the builder members are removed | no |
| `pg-wrapped:/dashboard/panels/5/targets/0/editorMode` | unsupported | 6: Builder without SQL | `targets.0.editorMode` | Hune has no query builder for PostgreSQL: the visual builder is not imported | panel removed | no |
| `pg-wrapped:/dashboard/panels/6/targets/0/rawSql` | unsupported | 7: Gap fill value | `targets.0.rawSql` | 1 group macro(s) with a third argument: Hune does not fill the gaps of a series (7.22) | the third argument is removed, and the panel shows the gaps | no |
| `pg-wrapped:/dashboard/panels/7/targets/0/rawSql` | unsupported | 8: Macro of the other dialect | `targets.0.rawSql` | SQL: line 1, column 8: $__timeInterval is not a macro of Hune | panel removed | no |
| `pg-wrapped:/dashboard/panels/7/targets/0/rawSql` | unsupported | 8: Macro of the other dialect | `targets.0.rawSql` | SQL: line 1, column 77: $__timeFilter_ms is not a macro of Hune | panel removed | no |
| `pg-wrapped:/dashboard/panels/8/targets/0/rawSql` | unsupported | 9: Interval with two units | `targets.0.rawSql` | SQL: line 1, column 8: $__timeGroup: the interval string must be a whole number and one unit: ms, s, m, h, d, w, M or y | panel removed | no |
| `pg-wrapped:/dashboard/panels/9/targets/0/rawSql` | changed | 10: Macro in a string | `targets.0.rawSql` | SQL: line 1, column 9: $__timeFrom stays text inside a string or a quoted name: Grafana replaces it |  | no |

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
Reason: 40 item(s) need acceptance and have none, and 0 acceptance line(s) are stale.
