# Import report

## Input

| Name | Value |
|---|---|
| Input directory | `/private/grafana-export-20261120T090000Z` |
| Grafana version | 12.2.1 |
| ClickHouse plugin version | 4.5.1 |
| Mode | `dry_run` |

## Totals

| Total | Count |
|---|---|
| dashboards | 10 |
| panels | 82 |
| mapped | 1487 |
| changed | 71 |
| dropped | 24 |
| unsupported | 88 |
| metadata | 787 |
| accepted | 0 |
| unaccepted | 183 |
| stale_accept_lines | 0 |

## Objects that are not imported

- History: not in the export.
- Public dashboards: 1.
- Short links: 4.

## Dashboards

### `Mixed_Case_Uid`: Mixed case uid

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `mixed-case-uid` | `nodes` | 1 | 13 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `Mixed_Case_Uid:/dashboard/uid` | changed |  | `uid` | the uid does not fit the rule of 4.3: the new uid is mixed-case-uid |  | no |

### `Prod_Main`: Prod main again

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `g-301f976e365b83e3` | `general` | 1 | 13 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `Prod_Main:/dashboard/uid` | changed |  | `uid` | the uid does not fit the rule of 4.3: the new uid is g-301f976e365b83e3 |  | no |

### `bad-folder`: In a folder without id

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `bad-folder` | `` | 1 | 13 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `bad-folder:/meta/folderUid` | unsupported |  | `meta.folderUid` | the folder uid does not fit the rule of 4.3 and the folder map has no entry (15.7) | the dashboard is not imported | no |

### `monitor`: Node monitor

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `monitor` | `transit` | 19 | 107 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `monitor:/annotations/list/1/target` | unsupported |  | `annotations.list.1.target` | the annotation has no SQL text (rawSql) | item removed | no |
| `monitor:/graphTooltip` | changed |  | `graphTooltip` | the shared tooltip becomes the shared crosshair |  | no |
| `monitor:/panels/0/fieldConfig/overrides/0/properties/9/value/mode` | changed | 1: Weight over time | `fieldConfig.overrides.0.properties.9.value.mode` | the Grafana palette mode palette-classic-by-name becomes the series palette of Hune (mode palette) |  | no |
| `monitor:/panels/1/options/colorMode` | changed | 2: Total weight | `options.colorMode` | the value background_solid becomes background |  | no |
| `monitor:/panels/10/panels/0/options/reduceOptions/calcs` | changed | 10: Inner stat | `options.reduceOptions.calcs` | no reducer: the Grafana default last_not_null |  | no |
| `monitor:/panels/10/panels/1/targets/0/datasource` | unsupported | 11: Mongo documents | `targets.0.datasource` | the data source mongo1 is not in the source map | panel removed | no |
| `monitor:/panels/11/repeat` | unsupported |  | `panels.11.repeat` | the variable env must be a query or custom variable with several values (multi): Hune repeats by such a variable only (4.13a) | no repeat | no |
| `monitor:/panels/11/title` | changed |  | `panels.11.title` | an empty title becomes the text Row |  | no |
| `monitor:/panels/13/libraryPanel` | unsupported | 14: Library | `libraryPanel` | library panels are out of scope | panel removed | no |
| `monitor:/panels/14/targets/0/datasource` | unsupported | 15: Unmapped source | `targets.0.datasource` | the data source other-ch is not in the source map | panel removed | no |
| `monitor:/panels/15/targets/0/editorType` | changed | 16: Builder query | `targets.0.editorType` | the SQL that the builder made stays: the builder form is removed |  | no |
| `monitor:/panels/16/targets/0/rawSql` | unsupported | 17: Bad macro | `targets.0.rawSql` | SQL: line 1, column 47: $__range_s is not a macro of Hune | panel removed | no |
| `monitor:/panels/16/targets/0/rawSql` | unsupported | 17: Bad macro | `targets.0.rawSql` | SQL: line 1, column 66: the variable node has the format json: this format is permitted only inside a string | panel removed | no |
| `monitor:/panels/17/targets/0/panelId` | unsupported | 18: Reuse of a reuse | `targets.0.panelId` | the source panel 2 is absent, removed, or reuses a frame itself | panel removed | no |
| `monitor:/panels/18/targets/0/datasource` | unsupported | 19: Mixed | `targets.0.datasource` | the data source -- Mixed -- has no equal in format 1 | panel removed | no |
| `monitor:/panels/3/fieldConfig/overrides/1/properties/3/value/seriesBy` | dropped | 3: Nodes | `fieldConfig.overrides.1.properties.3.value.seriesBy` | seriesBy has no meaning in Hune |  | no |
| `monitor:/panels/3/fieldConfig/overrides/2/properties/0/value/0/options/2` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.0.options.2` | the mapping has no text and no colour |  | no |
| `monitor:/panels/3/fieldConfig/overrides/2/properties/0/value/4/options` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.4.options` | the mapping has no text and no colour |  | no |
| `monitor:/panels/3/fieldConfig/overrides/2/properties/2/value` | unsupported | 3: Nodes | `fieldConfig.overrides.2.properties.2.value` | no_value is permitted only in the defaults (4.18) | property removed | no |
| `monitor:/panels/3/fieldConfig/overrides/3/properties/0/value/1/url` | unsupported | 3: Nodes | `fieldConfig.overrides.3.properties.0.value.1.url` | the prefix of the url breaks the rule of 4.23 | link removed | no |
| `monitor:/panels/3/fieldConfig/overrides/4/matcher/id` | unsupported | 3: Nodes | `fieldConfig.overrides.4.matcher.id` | this matcher has no equal in format 2 | override removed | no |
| `monitor:/panels/3/fieldConfig/overrides/5/properties/1/value` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.1.value` | the unit is not in the list of 4.20 | property removed | no |
| `monitor:/panels/3/fieldConfig/overrides/5/properties/2/value/type` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.2.value.type` | this cell type is not in the property set (19.15) | auto | no |
| `monitor:/panels/3/options/footer` | changed | 3: Nodes | `options.footer` | Grafana 12 moves this footer to custom.footer.reducers: the importer moves it in the same way (4.18) |  | no |
| `monitor:/panels/3/targets/0/rawSql` | changed | 3: Nodes | `targets.0.rawSql` | the text has a carriage return: each \r\n and \r is a line end |  | no |
| `monitor:/panels/3/targets/0/rawSql` | changed | 3: Nodes | `targets.0.rawSql` | the old form [[name]] is rewritten to ${name} for: node |  | no |
| `monitor:/panels/3/transformations/2/disabled` | dropped | 3: Nodes | `transformations.2.disabled` | the transformation is disabled: it is removed |  | no |
| `monitor:/panels/3/transformations/3/options/filters/0/config/id` | unsupported | 3: Nodes | `transformations.3.options.filters.0.config.id` | this matcher has no equal in format 1 | transformation removed | no |
| `monitor:/panels/7/options/namePlacement` | unsupported | 7: Ratio | `options.namePlacement` | unknown member | member removed | no |
| `monitor:/panels/7/options/orientation` | changed | 7: Ratio | `options.orientation` | the value auto becomes horizontal |  | no |
| `monitor:/panels/7/options/reduceOptions/calcs` | unsupported | 7: Ratio | `options.reduceOptions.calcs` | format 1 has one reducer: the other items are removed | the first item: max | no |
| `monitor:/panels/8/transformations/0/options/emptyValue` | unsupported | 8: Matrix | `transformations.0.options.emptyValue` | this emptyValue has no equal in format 1 | transformation removed | no |
| `monitor:/panels/8/transformations/2` | changed | 8: Matrix | `transformations.2` | after long-to-wide, the result is the table form: format table |  | no |
| `monitor:/panels/9/options/legend/calcs/1` | unsupported | 9: Wide series | `options.legend.calcs.1` | this reducer has no equal in format 2 | item removed | no |
| `monitor:/panels/9/transformations/0` | changed | 9: Wide series | `transformations.0` | the multi-frame form is the long-to-wide of format time_series |  | no |
| `monitor:/refresh` | changed |  | `refresh` | refresh 5s is not a choice of 4.6: the next larger choice is 10s |  | no |
| `monitor:/style` | unsupported |  | `style` | unknown member | member removed | no |
| `monitor:/tags/2` | changed |  | `tags.2` | two equal tags are changed to one tag |  | no |
| `monitor:/templating/list/1/allowCustomValue` | changed |  | `templating.list.1.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `monitor:/templating/list/3/query` | changed |  | `templating.list.3.query` | the variable top_n stands in code: value_type is number |  | no |
| `monitor:/templating/list/3/skipUrlSync` | dropped |  | `templating.list.3.skipUrlSync` | skipUrlSync has no meaning in Hune |  | no |
| `monitor:/templating/list/4/description` | dropped |  | `templating.list.4.description` | format 1 has no description of a variable (4.7) |  | no |
| `monitor:/templating/list/4/refresh` | changed |  | `templating.list.4.refresh` | Hune computes the options at load: Grafana keeps stored options |  | no |
| `monitor:/zzz` | unsupported |  | `zzz` | unknown member | member removed | no |

### `monitor-v2a`: Node monitor

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `monitor-v2a` | `transit` | 19 | 236 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `monitor-v2a:/metadata/annotations/grafana.app~1createdBy` | unsupported |  | `metadata.annotations.grafana.app/createdBy` | unknown member | member removed | no |
| `monitor-v2a:/metadata/annotations/grafana.app~1updatedTimestamp` | unsupported |  | `metadata.annotations.grafana.app/updatedTimestamp` | unknown member | member removed | no |
| `monitor-v2a:/spec/annotations/1` | unsupported |  | `spec.annotations.1` | the data source of the annotation is not in the source map | item removed | no |
| `monitor-v2a:/spec/cursorSync` | changed |  | `spec.cursorSync` | the shared tooltip becomes the shared crosshair |  | no |
| `monitor-v2a:/spec/elements/panel-1/spec/vizConfig/spec/fieldConfig/overrides/0/properties/9/value/mode` | changed | 1: Weight over time | `fieldConfig.overrides.0.properties.9.value.mode` | the Grafana palette mode palette-classic-by-name becomes the series palette of Hune (mode palette) |  | no |
| `monitor-v2a:/spec/elements/panel-10/spec/vizConfig/spec/options/reduceOptions/calcs` | changed | 10: Inner stat | `options.reduceOptions.calcs` | no reducer: the Grafana default last_not_null |  | no |
| `monitor-v2a:/spec/elements/panel-11/spec/data/spec/queries/0/spec/datasource` | unsupported | 11: Mongo documents | `queries.0.spec.datasource` | the data source mongo1 is not in the source map | panel removed | no |
| `monitor-v2a:/spec/elements/panel-14/spec/libraryPanel` | unsupported | 14: Library | `libraryPanel` | library panels are out of scope | panel removed | no |
| `monitor-v2a:/spec/elements/panel-15/spec/data/spec/queries/0/spec/datasource` | unsupported | 15: Unmapped source | `queries.0.spec.datasource` | the data source other-ch is not in the source map | panel removed | no |
| `monitor-v2a:/spec/elements/panel-16/spec/data/spec/queries/0/spec/query/spec/editorType` | changed | 16: Builder query | `queries.0.spec.query.spec.editorType` | the SQL that the builder made stays: the builder form is removed |  | no |
| `monitor-v2a:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: Bad macro | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 47: $__range_s is not a macro of Hune | panel removed | no |
| `monitor-v2a:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: Bad macro | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 66: the variable node has the format json: this format is permitted only inside a string | panel removed | no |
| `monitor-v2a:/spec/elements/panel-18/spec/data/spec/queries/0/spec/query/spec/panelId` | unsupported | 18: Reuse of a reuse | `queries.0.spec.query.spec.panelId` | the source panel 2 is absent, removed, or reuses a frame itself | panel removed | no |
| `monitor-v2a:/spec/elements/panel-19/spec/data/spec/queries/0/spec/datasource` | unsupported | 19: Mixed | `queries.0.spec.datasource` | the data source -- Mixed -- has no equal in format 1 | panel removed | no |
| `monitor-v2a:/spec/elements/panel-2/spec/vizConfig/spec/options/colorMode` | changed | 2: Total weight | `options.colorMode` | the value background_solid becomes background |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 3: Nodes | `queries.0.spec.query.spec.rawSql` | the text has a carriage return: each \r\n and \r is a line end |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 3: Nodes | `queries.0.spec.query.spec.rawSql` | the old form [[name]] is rewritten to ${name} for: node |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/data/spec/transformations/2/spec/disabled` | dropped | 3: Nodes | `transformations.2.spec.disabled` | the transformation is disabled: it is removed |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/data/spec/transformations/3/spec/options/filters/0/config/id` | unsupported | 3: Nodes | `transformations.3.spec.options.filters.0.config.id` | this matcher has no equal in format 1 | transformation removed | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/1/properties/3/value/seriesBy` | dropped | 3: Nodes | `fieldConfig.overrides.1.properties.3.value.seriesBy` | seriesBy has no meaning in Hune |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/0/value/0/options/2` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.0.options.2` | the mapping has no text and no colour |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/0/value/4/options` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.4.options` | the mapping has no text and no colour |  | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/2/value` | unsupported | 3: Nodes | `fieldConfig.overrides.2.properties.2.value` | no_value is permitted only in the defaults (4.18) | property removed | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/3/properties/0/value/1/url` | unsupported | 3: Nodes | `fieldConfig.overrides.3.properties.0.value.1.url` | the prefix of the url breaks the rule of 4.23 | link removed | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/4/matcher/id` | unsupported | 3: Nodes | `fieldConfig.overrides.4.matcher.id` | this matcher has no equal in format 2 | override removed | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/5/properties/1/value` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.1.value` | the unit is not in the list of 4.20 | property removed | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/5/properties/2/value/type` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.2.value.type` | this cell type is not in the property set (19.15) | auto | no |
| `monitor-v2a:/spec/elements/panel-3/spec/vizConfig/spec/options/footer` | changed | 3: Nodes | `options.footer` | Grafana 12 moves this footer to custom.footer.reducers: the importer moves it in the same way (4.18) |  | no |
| `monitor-v2a:/spec/elements/panel-7/spec/vizConfig/spec/options/namePlacement` | unsupported | 7: Ratio | `options.namePlacement` | unknown member | member removed | no |
| `monitor-v2a:/spec/elements/panel-7/spec/vizConfig/spec/options/orientation` | changed | 7: Ratio | `options.orientation` | the value auto becomes horizontal |  | no |
| `monitor-v2a:/spec/elements/panel-7/spec/vizConfig/spec/options/reduceOptions/calcs` | unsupported | 7: Ratio | `options.reduceOptions.calcs` | format 1 has one reducer: the other items are removed | the first item: max | no |
| `monitor-v2a:/spec/elements/panel-8/spec/data/spec/transformations/0/spec/options/emptyValue` | unsupported | 8: Matrix | `transformations.0.spec.options.emptyValue` | this emptyValue has no equal in format 1 | transformation removed | no |
| `monitor-v2a:/spec/elements/panel-8/spec/data/spec/transformations/2` | changed | 8: Matrix | `transformations.2` | after long-to-wide, the result is the table form: format table |  | no |
| `monitor-v2a:/spec/elements/panel-9/spec/data/spec/transformations/0` | changed | 9: Wide series | `transformations.0` | the multi-frame form is the long-to-wide of format time_series |  | no |
| `monitor-v2a:/spec/elements/panel-9/spec/vizConfig/spec/options/legend/calcs/1` | unsupported | 9: Wide series | `options.legend.calcs.1` | this reducer has no equal in format 2 | item removed | no |
| `monitor-v2a:/spec/layout/spec/rows/3/spec/repeat/value` | unsupported |  | `spec.layout.spec.rows.3.spec.repeat.value` | the variable env must be a query or custom variable with several values (multi): Hune repeats by such a variable only (4.13a) | no repeat | no |
| `monitor-v2a:/spec/layout/spec/rows/3/spec/title` | changed |  | `spec.layout.spec.rows.3.spec.title` | an empty title becomes the text Row |  | no |
| `monitor-v2a:/spec/tags/2` | changed |  | `spec.tags.2` | two equal tags are changed to one tag |  | no |
| `monitor-v2a:/spec/timeSettings/autoRefresh` | changed |  | `spec.timeSettings.autoRefresh` | refresh 5s is not a choice of 4.6: the next larger choice is 10s |  | no |
| `monitor-v2a:/spec/variables/1/spec/allowCustomValue` | changed |  | `spec.variables.1.spec.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `monitor-v2a:/spec/variables/3/spec/query` | changed |  | `spec.variables.3.spec.query` | the variable top_n stands in code: value_type is number |  | no |
| `monitor-v2a:/spec/variables/3/spec/skipUrlSync` | dropped |  | `spec.variables.3.spec.skipUrlSync` | skipUrlSync has no meaning in Hune |  | no |
| `monitor-v2a:/spec/variables/4/spec/description` | dropped |  | `spec.variables.4.spec.description` | format 1 has no description of a variable (4.7) |  | no |
| `monitor-v2a:/spec/variables/4/spec/refresh` | changed |  | `spec.variables.4.spec.refresh` | Hune computes the options at load: Grafana keeps stored options |  | no |
| `monitor-v2a:/spec/zzz` | unsupported |  | `spec.zzz` | unknown member | member removed | no |

### `monitor-v2b`: Node monitor

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `monitor-v2b` | `general` | 19 | 256 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `monitor-v2b:/metadata/annotations/grafana.app~1createdBy` | unsupported |  | `metadata.annotations.grafana.app/createdBy` | unknown member | member removed | no |
| `monitor-v2b:/metadata/annotations/grafana.app~1updatedTimestamp` | unsupported |  | `metadata.annotations.grafana.app/updatedTimestamp` | unknown member | member removed | no |
| `monitor-v2b:/spec/annotations/1` | unsupported |  | `spec.annotations.1` | the data source of the annotation is not in the source map | item removed | no |
| `monitor-v2b:/spec/cursorSync` | changed |  | `spec.cursorSync` | the shared tooltip becomes the shared crosshair |  | no |
| `monitor-v2b:/spec/elements/panel-1/spec/vizConfig/spec/fieldConfig/overrides/0/properties/9/value/mode` | changed | 1: Weight over time | `fieldConfig.overrides.0.properties.9.value.mode` | the Grafana palette mode palette-classic-by-name becomes the series palette of Hune (mode palette) |  | no |
| `monitor-v2b:/spec/elements/panel-10/spec/vizConfig/spec/options/reduceOptions/calcs` | changed | 10: Inner stat | `options.reduceOptions.calcs` | no reducer: the Grafana default last_not_null |  | no |
| `monitor-v2b:/spec/elements/panel-11/spec/data/spec/queries/0/spec/query/datasource` | unsupported | 11: Mongo documents | `queries.0.spec.query.datasource` | the data source mongo1 is not in the source map | panel removed | no |
| `monitor-v2b:/spec/elements/panel-14/spec/libraryPanel` | unsupported | 14: Library | `libraryPanel` | library panels are out of scope | panel removed | no |
| `monitor-v2b:/spec/elements/panel-15/spec/data/spec/queries/0/spec/query/datasource` | unsupported | 15: Unmapped source | `queries.0.spec.query.datasource` | the data source other-ch is not in the source map | panel removed | no |
| `monitor-v2b:/spec/elements/panel-16/spec/data/spec/queries/0/spec/query/spec/editorType` | changed | 16: Builder query | `queries.0.spec.query.spec.editorType` | the SQL that the builder made stays: the builder form is removed |  | no |
| `monitor-v2b:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: Bad macro | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 47: $__range_s is not a macro of Hune | panel removed | no |
| `monitor-v2b:/spec/elements/panel-17/spec/data/spec/queries/0/spec/query/spec/rawSql` | unsupported | 17: Bad macro | `queries.0.spec.query.spec.rawSql` | SQL: line 1, column 66: the variable node has the format json: this format is permitted only inside a string | panel removed | no |
| `monitor-v2b:/spec/elements/panel-18/spec/data/spec/queries/0/spec/query/spec/panelId` | unsupported | 18: Reuse of a reuse | `queries.0.spec.query.spec.panelId` | the source panel 2 is absent, removed, or reuses a frame itself | panel removed | no |
| `monitor-v2b:/spec/elements/panel-19/spec/data/spec/queries/0/spec/query/datasource` | unsupported | 19: Mixed | `queries.0.spec.query.datasource` | the data source -- Mixed -- has no equal in format 1 | panel removed | no |
| `monitor-v2b:/spec/elements/panel-2/spec/vizConfig/spec/options/colorMode` | changed | 2: Total weight | `options.colorMode` | the value background_solid becomes background |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 3: Nodes | `queries.0.spec.query.spec.rawSql` | the text has a carriage return: each \r\n and \r is a line end |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/data/spec/queries/0/spec/query/spec/rawSql` | changed | 3: Nodes | `queries.0.spec.query.spec.rawSql` | the old form [[name]] is rewritten to ${name} for: node |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/data/spec/transformations/2/spec/disabled` | dropped | 3: Nodes | `transformations.2.spec.disabled` | the transformation is disabled: it is removed |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/data/spec/transformations/3/spec/options/filters/0/config/id` | unsupported | 3: Nodes | `transformations.3.spec.options.filters.0.config.id` | this matcher has no equal in format 1 | transformation removed | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/1/properties/3/value/seriesBy` | dropped | 3: Nodes | `fieldConfig.overrides.1.properties.3.value.seriesBy` | seriesBy has no meaning in Hune |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/0/value/0/options/2` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.0.options.2` | the mapping has no text and no colour |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/0/value/4/options` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.4.options` | the mapping has no text and no colour |  | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/2/properties/2/value` | unsupported | 3: Nodes | `fieldConfig.overrides.2.properties.2.value` | no_value is permitted only in the defaults (4.18) | property removed | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/3/properties/0/value/1/url` | unsupported | 3: Nodes | `fieldConfig.overrides.3.properties.0.value.1.url` | the prefix of the url breaks the rule of 4.23 | link removed | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/4/matcher/id` | unsupported | 3: Nodes | `fieldConfig.overrides.4.matcher.id` | this matcher has no equal in format 2 | override removed | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/5/properties/1/value` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.1.value` | the unit is not in the list of 4.20 | property removed | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/fieldConfig/overrides/5/properties/2/value/type` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.2.value.type` | this cell type is not in the property set (19.15) | auto | no |
| `monitor-v2b:/spec/elements/panel-3/spec/vizConfig/spec/options/footer` | changed | 3: Nodes | `options.footer` | Grafana 12 moves this footer to custom.footer.reducers: the importer moves it in the same way (4.18) |  | no |
| `monitor-v2b:/spec/elements/panel-7/spec/vizConfig/spec/options/namePlacement` | unsupported | 7: Ratio | `options.namePlacement` | unknown member | member removed | no |
| `monitor-v2b:/spec/elements/panel-7/spec/vizConfig/spec/options/orientation` | changed | 7: Ratio | `options.orientation` | the value auto becomes horizontal |  | no |
| `monitor-v2b:/spec/elements/panel-7/spec/vizConfig/spec/options/reduceOptions/calcs` | unsupported | 7: Ratio | `options.reduceOptions.calcs` | format 1 has one reducer: the other items are removed | the first item: max | no |
| `monitor-v2b:/spec/elements/panel-8/spec/data/spec/transformations/0/spec/options/emptyValue` | unsupported | 8: Matrix | `transformations.0.spec.options.emptyValue` | this emptyValue has no equal in format 1 | transformation removed | no |
| `monitor-v2b:/spec/elements/panel-8/spec/data/spec/transformations/2` | changed | 8: Matrix | `transformations.2` | after long-to-wide, the result is the table form: format table |  | no |
| `monitor-v2b:/spec/elements/panel-9/spec/data/spec/transformations/0` | changed | 9: Wide series | `transformations.0` | the multi-frame form is the long-to-wide of format time_series |  | no |
| `monitor-v2b:/spec/elements/panel-9/spec/vizConfig/spec/options/legend/calcs/1` | unsupported | 9: Wide series | `options.legend.calcs.1` | this reducer has no equal in format 2 | item removed | no |
| `monitor-v2b:/spec/layout/spec/rows/3/spec/repeat/value` | unsupported |  | `spec.layout.spec.rows.3.spec.repeat.value` | the variable env must be a query or custom variable with several values (multi): Hune repeats by such a variable only (4.13a) | no repeat | no |
| `monitor-v2b:/spec/layout/spec/rows/3/spec/title` | changed |  | `spec.layout.spec.rows.3.spec.title` | an empty title becomes the text Row |  | no |
| `monitor-v2b:/spec/tags/2` | changed |  | `spec.tags.2` | two equal tags are changed to one tag |  | no |
| `monitor-v2b:/spec/timeSettings/autoRefresh` | changed |  | `spec.timeSettings.autoRefresh` | refresh 5s is not a choice of 4.6: the next larger choice is 10s |  | no |
| `monitor-v2b:/spec/variables/3/spec/query` | changed |  | `spec.variables.3.spec.query` | the variable top_n stands in code: value_type is number |  | no |
| `monitor-v2b:/spec/variables/3/spec/skipUrlSync` | dropped |  | `spec.variables.3.spec.skipUrlSync` | skipUrlSync has no meaning in Hune |  | no |
| `monitor-v2b:/spec/variables/4/spec/description` | dropped |  | `spec.variables.4.spec.description` | format 1 has no description of a variable (4.7) |  | no |
| `monitor-v2b:/spec/variables/4/spec/refresh` | changed |  | `spec.variables.4.spec.refresh` | Hune computes the options at load: Grafana keeps stored options |  | no |
| `monitor-v2b:/spec/zzz` | unsupported |  | `spec.zzz` | unknown member | member removed | no |

### `monitor-wrapped`: Node monitor

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `monitor-wrapped` | `transit` | 19 | 116 |

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `monitor-wrapped:/dashboard/annotations/list/1/target` | unsupported |  | `annotations.list.1.target` | the annotation has no SQL text (rawSql) | item removed | no |
| `monitor-wrapped:/dashboard/graphTooltip` | changed |  | `graphTooltip` | the shared tooltip becomes the shared crosshair |  | no |
| `monitor-wrapped:/dashboard/panels/0/fieldConfig/overrides/0/properties/9/value/mode` | changed | 1: Weight over time | `fieldConfig.overrides.0.properties.9.value.mode` | the Grafana palette mode palette-classic-by-name becomes the series palette of Hune (mode palette) |  | no |
| `monitor-wrapped:/dashboard/panels/1/options/colorMode` | changed | 2: Total weight | `options.colorMode` | the value background_solid becomes background |  | no |
| `monitor-wrapped:/dashboard/panels/10/panels/0/options/reduceOptions/calcs` | changed | 10: Inner stat | `options.reduceOptions.calcs` | no reducer: the Grafana default last_not_null |  | no |
| `monitor-wrapped:/dashboard/panels/10/panels/1/targets/0/datasource` | unsupported | 11: Mongo documents | `targets.0.datasource` | the data source mongo1 is not in the source map | panel removed | no |
| `monitor-wrapped:/dashboard/panels/11/repeat` | unsupported |  | `panels.11.repeat` | the variable env must be a query or custom variable with several values (multi): Hune repeats by such a variable only (4.13a) | no repeat | no |
| `monitor-wrapped:/dashboard/panels/11/title` | changed |  | `panels.11.title` | an empty title becomes the text Row |  | no |
| `monitor-wrapped:/dashboard/panels/13/libraryPanel` | unsupported | 14: Library | `libraryPanel` | library panels are out of scope | panel removed | no |
| `monitor-wrapped:/dashboard/panels/14/targets/0/datasource` | unsupported | 15: Unmapped source | `targets.0.datasource` | the data source other-ch is not in the source map | panel removed | no |
| `monitor-wrapped:/dashboard/panels/15/targets/0/editorType` | changed | 16: Builder query | `targets.0.editorType` | the SQL that the builder made stays: the builder form is removed |  | no |
| `monitor-wrapped:/dashboard/panels/16/targets/0/rawSql` | unsupported | 17: Bad macro | `targets.0.rawSql` | SQL: line 1, column 47: $__range_s is not a macro of Hune | panel removed | no |
| `monitor-wrapped:/dashboard/panels/16/targets/0/rawSql` | unsupported | 17: Bad macro | `targets.0.rawSql` | SQL: line 1, column 66: the variable node has the format json: this format is permitted only inside a string | panel removed | no |
| `monitor-wrapped:/dashboard/panels/17/targets/0/panelId` | unsupported | 18: Reuse of a reuse | `targets.0.panelId` | the source panel 2 is absent, removed, or reuses a frame itself | panel removed | no |
| `monitor-wrapped:/dashboard/panels/18/targets/0/datasource` | unsupported | 19: Mixed | `targets.0.datasource` | the data source -- Mixed -- has no equal in format 1 | panel removed | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/1/properties/3/value/seriesBy` | dropped | 3: Nodes | `fieldConfig.overrides.1.properties.3.value.seriesBy` | seriesBy has no meaning in Hune |  | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/2/properties/0/value/0/options/2` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.0.options.2` | the mapping has no text and no colour |  | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/2/properties/0/value/4/options` | dropped | 3: Nodes | `fieldConfig.overrides.2.properties.0.value.4.options` | the mapping has no text and no colour |  | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/2/properties/2/value` | unsupported | 3: Nodes | `fieldConfig.overrides.2.properties.2.value` | no_value is permitted only in the defaults (4.18) | property removed | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/3/properties/0/value/1/url` | unsupported | 3: Nodes | `fieldConfig.overrides.3.properties.0.value.1.url` | the prefix of the url breaks the rule of 4.23 | link removed | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/4/matcher/id` | unsupported | 3: Nodes | `fieldConfig.overrides.4.matcher.id` | this matcher has no equal in format 2 | override removed | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/5/properties/1/value` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.1.value` | the unit is not in the list of 4.20 | property removed | no |
| `monitor-wrapped:/dashboard/panels/3/fieldConfig/overrides/5/properties/2/value/type` | unsupported | 3: Nodes | `fieldConfig.overrides.5.properties.2.value.type` | this cell type is not in the property set (19.15) | auto | no |
| `monitor-wrapped:/dashboard/panels/3/options/footer` | changed | 3: Nodes | `options.footer` | Grafana 12 moves this footer to custom.footer.reducers: the importer moves it in the same way (4.18) |  | no |
| `monitor-wrapped:/dashboard/panels/3/targets/0/rawSql` | changed | 3: Nodes | `targets.0.rawSql` | the text has a carriage return: each \r\n and \r is a line end |  | no |
| `monitor-wrapped:/dashboard/panels/3/targets/0/rawSql` | changed | 3: Nodes | `targets.0.rawSql` | the old form [[name]] is rewritten to ${name} for: node |  | no |
| `monitor-wrapped:/dashboard/panels/3/transformations/2/disabled` | dropped | 3: Nodes | `transformations.2.disabled` | the transformation is disabled: it is removed |  | no |
| `monitor-wrapped:/dashboard/panels/3/transformations/3/options/filters/0/config/id` | unsupported | 3: Nodes | `transformations.3.options.filters.0.config.id` | this matcher has no equal in format 1 | transformation removed | no |
| `monitor-wrapped:/dashboard/panels/7/options/namePlacement` | unsupported | 7: Ratio | `options.namePlacement` | unknown member | member removed | no |
| `monitor-wrapped:/dashboard/panels/7/options/orientation` | changed | 7: Ratio | `options.orientation` | the value auto becomes horizontal |  | no |
| `monitor-wrapped:/dashboard/panels/7/options/reduceOptions/calcs` | unsupported | 7: Ratio | `options.reduceOptions.calcs` | format 1 has one reducer: the other items are removed | the first item: max | no |
| `monitor-wrapped:/dashboard/panels/8/transformations/0/options/emptyValue` | unsupported | 8: Matrix | `transformations.0.options.emptyValue` | this emptyValue has no equal in format 1 | transformation removed | no |
| `monitor-wrapped:/dashboard/panels/8/transformations/2` | changed | 8: Matrix | `transformations.2` | after long-to-wide, the result is the table form: format table |  | no |
| `monitor-wrapped:/dashboard/panels/9/options/legend/calcs/1` | unsupported | 9: Wide series | `options.legend.calcs.1` | this reducer has no equal in format 2 | item removed | no |
| `monitor-wrapped:/dashboard/panels/9/transformations/0` | changed | 9: Wide series | `transformations.0` | the multi-frame form is the long-to-wide of format time_series |  | no |
| `monitor-wrapped:/dashboard/refresh` | changed |  | `refresh` | refresh 5s is not a choice of 4.6: the next larger choice is 10s |  | no |
| `monitor-wrapped:/dashboard/style` | unsupported |  | `style` | unknown member | member removed | no |
| `monitor-wrapped:/dashboard/tags/2` | changed |  | `tags.2` | two equal tags are changed to one tag |  | no |
| `monitor-wrapped:/dashboard/templating/list/1/allowCustomValue` | changed |  | `templating.list.1.allowCustomValue` | Grafana reads an absent allowCustomValue as true; Hune keeps the variable closed, so the server keeps its option check |  | no |
| `monitor-wrapped:/dashboard/templating/list/3/query` | changed |  | `templating.list.3.query` | the variable top_n stands in code: value_type is number |  | no |
| `monitor-wrapped:/dashboard/templating/list/3/skipUrlSync` | dropped |  | `templating.list.3.skipUrlSync` | skipUrlSync has no meaning in Hune |  | no |
| `monitor-wrapped:/dashboard/templating/list/4/description` | dropped |  | `templating.list.4.description` | format 1 has no description of a variable (4.7) |  | no |
| `monitor-wrapped:/dashboard/templating/list/4/refresh` | changed |  | `templating.list.4.refresh` | Hune computes the options at load: Grafana keeps stored options |  | no |
| `monitor-wrapped:/dashboard/zzz` | unsupported |  | `zzz` | unknown member | member removed | no |

### `nested-case`: In a nested folder

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `blocked` | `nested-case` | `` | 1 | 13 |

No item needs acceptance.

### `plain-raw`: Plain raw dashboard

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `would_write` | `plain-raw` | `general` | 1 | 7 |

No item needs acceptance.

### `prod-main`: Prod main

| Result | Hune uid | Folder | Panels | Metadata members |
|---|---|---|---|---|
| `would_write` | `prod-main` | `general` | 1 | 13 |

No item needs acceptance.

### Folders and short links

| Id | Class | Panel | Member | Reason | Fallback | Accepted |
|---|---|---|---|---|---|---|
| `folder:kid0nested:/2/parentUid` | unsupported |  | `2.parentUid` | Folders have one level: a folder with a parent is not imported | its dashboards are not imported | no |
| `short-url:bad uid:/2/uid` | changed |  | `2.uid` | the short uid has a character outside [A-Za-z0-9_-]: the nginx map has no line for it |  | no |
| `short-url:Lm0pqRs:/3/path` | changed |  | `3.path` | the path is not a link to an imported dashboard: the redirect goes to the start page |  | no |

## Redirects

```nginx
# Written by hune import-grafana. Include in the http context.
map $hune_grafana_uid $hune_new_uid {
    default "";
    "Mixed_Case_Uid" "mixed-case-uid";
    "Prod_Main" "g-301f976e365b83e3";
    "monitor" "monitor";
    "monitor-v2a" "monitor-v2a";
    "monitor-v2b" "monitor-v2b";
    "monitor-wrapped" "monitor-wrapped";
    "plain-raw" "plain-raw";
    "prod-main" "prod-main";
}
map $hune_grafana_short $hune_short_target {
    default "";
    "Lm0pqRs" "/dashboards/";
    "Qw9-erT" "/dashboards/d/mixed-case-uid";
    "xYz12AB" "/dashboards/d/monitor/node-monitor?orgId=1&from=now-7d&to=now&var-node=%24__all";
}
```

## Exit code

Exit code: 1.
Reason: 183 item(s) need acceptance and have none, and 0 acceptance line(s) are stale.
