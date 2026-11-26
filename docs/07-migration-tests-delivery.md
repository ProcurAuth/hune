# Part 7: Migration, tests and delivery

Part 7 of the Hune specification (see `01-overview.md`).

This part has three sections. Section 15 gives the import from Grafana and a runbook for the move from Grafana to Hune.
Section 16 gives the tests. Section 17 gives the build, the release and the deployment files.

## 15. Import and migration from Grafana

### 15.1 Scope and principles

The subcommand `hune import-grafana` converts the dashboards of Grafana to documents of section 4. It is the only part of
Hune that knows Grafana. The crate `hune-grafana` holds the conversion as a pure function: Grafana JSON and options in,
documents and a report out. The binary `hune` reads the files, writes the report and writes the documents.

The importer obeys these rules:

- It reads exported files only. It never connects to Grafana, never reads `grafana.db` and never connects to ClickHouse.
- It makes a dry run by default. It writes documents only with `--write`.
- With `--write`, it writes through the `Store` trait of section 5. It is the same code for the volume and for PostgreSQL.
- Each document that it makes passes the validation of section 4 and the statement check of section 7. These are the same
  functions that the server uses.
- It gives each Grafana member exactly one class (15.5). No member is lost without an item in the report.
- The same input files and options give the same documents, byte for byte (15.13).

### 15.2 The command

```text
hune import-grafana --in <dir>
                     --source-map <grafana-key>=<source-id> [--source-map ...]
                     [--folders <file>]
                     [--link-prefix <old>=<new>]
                     [--accept <file>]
                     [--report <path>]
                     [--nginx-map <file>]
                     [-c <config>] [--write [--replace]]
```

| Option | Value | Rule |
|---|---|---|
| `--in` | directory | Necessary. The export directory of 15.4. The importer reads only the names of 15.4. |
| `--source-map` | `<grafana-key>=<source-id>` | Necessary, 1 to 64 times. Maps a Grafana data source to a `clickhouse "<id>"`, `postgres "<id>"` or `mongodb "<id>"` block (15.9). The key is a data source uid or name, the name of a data source variable, or the name of an item of `__inputs`. |
| `--folders` | JSON file | Optional. The folder map of 15.7. |
| `--link-prefix` | `<old>=<new>` | Optional, at most one time. Both parts start and end with `/`. Example: `/grafana/=/dashboards/`. |
| `--accept` | text file | Optional. The list of accepted losses (15.10). |
| `--report` | path without extension | Optional. Writes `<path>.json` and `<path>.md` (15.11). |
| `--nginx-map` | file | Optional. Writes the nginx map of 15.17. Needs `--link-prefix`. |
| `-c`, `--config` | configuration file | The option of 13.1. Necessary with `--write` (13.18). Optional in a dry run: then the importer opens the store read-only and compares (15.13). |
| `--write` | none | Writes the documents through the store. |
| `--replace` | none | Only with `--write`. Replaces a document whose uid exists with other bytes. |

An option that is not in this table stops the command with exit code 2. An option given two times stops it with exit code
2, except `--source-map`. Two `--source-map` options with the same key stop it with exit code 2.

The importer writes a short summary to the standard output. The summary holds the totals of 15.11 and the reason of the
exit code. Errors go to the standard error.

The importer writes `--report` and `--nginx-map` files with a temporary file and a rename. It replaces a file that exists.
The parent directory must exist. The importer refuses an output path inside a git checkout, with exit code 2. A git
checkout is a directory that holds a `.git` entry, and each directory below it. Reason: a report holds confidential SQL
(15.18).

### 15.3 The export from Grafana

The export is a written procedure that an operator runs on the host of the Grafana instance. Each request is a read-only
`GET` from inside the Grafana container, which has `curl`. In this example, Grafana serves under `/grafana/`
(`root_url` and `serve_from_sub_path`). Change the path when your instance uses another one.

The procedure needs two inputs. The operator reads them from a copy of `grafana.db` (the M0 measure of section 18 does the same):

- `stored-forms.txt`: one line for each dashboard, `<uid> v2` or `<uid> v1`. The stored form is the member `apiVersion`
  in the table `resource` of the unified storage in `grafana.db`.
- The value of `V2`: the v2 API version that Grafana serves, `v2beta1` or `v2alpha1`.

```sh
# Run on the host of the Grafana instance.
# Each request is a read-only GET to Grafana, from inside its own container.
set -eu
umask 077
V2="v2beta1"                                  # the v2 API version of your Grafana
GRAFANA_CONTAINER="grafana"                   # the name of the Grafana container
out="$HOME/grafana-export-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$out/dashboards"
g() { docker exec "$GRAFANA_CONTAINER" curl -sS --fail --max-time 60 "http://localhost:3000/grafana$1"; }
g "/api/health"                                         > "$out/health.json"
g "/api/plugins/grafana-clickhouse-datasource/settings" > "$out/plugin-clickhouse.json"
g "/api/folders?limit=1000"                             > "$out/folders.json"
g "/api/search?type=dash-db&limit=5000"                 > "$out/search.json"
g "/api/dashboards/public-dashboards"                   > "$out/public-dashboards.json"
while read -r uid form; do
  case "$form" in
    v2) g "/apis/dashboard.grafana.app/$V2/namespaces/default/dashboards/$uid" > "$out/dashboards/$uid.json" ;;
    v1) g "/api/dashboards/uid/$uid"                                          > "$out/dashboards/$uid.json" ;;
    *)  echo "bad form for $uid: $form" >&2; exit 2 ;;
  esac
done < stored-forms.txt
(cd "$out" && sha256sum ./*.json dashboards/*.json > SHA256SUMS)
```

The short links are not in an API answer. The operator writes their list from the copy of `grafana.db`, on a trusted
machine. Then the operator adds the file to the export directory:

```sh
sqlite3 -readonly -json grafana.db "SELECT uid, path FROM short_url ORDER BY uid" > "$out/short-urls.json"
```

The operator copies the directory to a trusted machine. The directory and its files stay outside every git checkout
(15.18). The namespace `default` is the namespace of the Grafana organisation 1. Another organisation has another namespace.

### 15.4 The input directory

| Name | Necessary | Content |
|---|---|---|
| `dashboards/<uid>.json` | yes, 1 to 1000 files | One dashboard: a v2 resource, a v1 dashboard, or a v1 `{dashboard, meta}` object. |
| `folders.json` | yes | The answer of `GET /api/folders`. |
| `search.json` | yes when a raw v1 file exists | The answer of `GET /api/search`. It gives the folder of a raw v1 dashboard. |
| `health.json` | yes | The answer of `GET /api/health`. The report records the Grafana version. |
| `plugin-clickhouse.json` | yes | The plugin settings. The report records the plugin version. |
| `public-dashboards.json` | no | The public dashboards. The report lists them as not imported. |
| `short-urls.json` | no | The short links. The report lists them as not imported, and 15.17 redirects them. |
| `SHA256SUMS` | no | When it exists, the importer checks each listed file before it reads it. |

These rules apply to the input:

- Each file is UTF-8 JSON of 32 MiB at most. A duplicate member, a byte order mark or data after the value stops the
  command with exit code 2.
- A symbolic link in the directory stops the command with exit code 2.
- The importer ignores other names and lists them in the report (`input.ignored_files`).
- A file listed in `SHA256SUMS` that differs from its digest stops the command with exit code 2.

The importer finds the form of each dashboard file from its members:

| Members | Form | Uid | Folder uid |
|---|---|---|---|
| `apiVersion` = `dashboard.grafana.app/v2alpha1` or `dashboard.grafana.app/v2beta1` | v2 | `metadata.name` | `metadata.annotations["grafana.app/folder"]` |
| `dashboard` and `meta` (objects) | v1 wrapped | `dashboard.uid` | `meta.folderUid` |
| `schemaVersion` (number) and `panels` (array) | v1 raw | `uid` | `folderUid` of the uid in `search.json` |

Another `apiVersion`, or a file that fits no row, stops the command with exit code 2. The file stem must equal the uid of
the file, else exit code 2. An empty or absent folder uid means the root of Grafana.

### 15.5 The four classes

| Class | Meaning | Listed in the report | Needs acceptance |
|---|---|---|---|
| `mapped` | The document gives the same behaviour. | No, counted only. | No |
| `changed` | The document gives the behaviour in another form or with a known difference. | Yes | Yes |
| `dropped` | The member is not in the document, by a rule of this specification. | Yes, except metadata | Yes, except metadata |
| `unsupported` | Hune cannot give the behaviour, or the member is not in the measured set. | Yes | Yes |

A metadata member is a `dropped` member that has no behaviour in Hune. The tables of 15.6 mark them "metadata". The
report counts them for each dashboard, and they need no acceptance.

Each `unsupported` row of 15.6 names a fallback. The importer applies the fallback only when the item is accepted (15.10).
When an `unsupported` item is not accepted, the dashboard is `blocked` and the importer writes no document (exit code 1).

The importer walks every member of each input file. A member that no row of 15.6 names is `unsupported` with the reason
"unknown member" and the fallback "member removed". So a Grafana feature that this specification does not know is never
lost without an item.

One exception applies: an unknown member whose value is `null`, `false`, `0`, `""`, `[]` or `{}` is metadata. Such a value
turns no feature on. Example: `"fillScreen": false` of a v2 row is metadata, and `"fillScreen": true` is `unsupported`.

The exception does not apply to a `false` or a `0` that turns off a feature that Grafana shows by default. Such a value
changes the view, so it is never metadata. The rows of 15.6 name each case. They are `custom.axisGridShow` = `false`
and `wideLayout` = `false` of a `stat`. A `barchart` maps `custom.axisGridShow`, `custom.fillOpacity` and
`custom.lineWidth` (4.18, 4.30), so `false` and 0 keep their effect there. The list comes from a probe of Grafana.

### 15.6 The mapping table

The column "Hune" names the member of the document format of section 4. The column "v2" gives the path below the v2
resource. The column "v1" gives the path in a v1 dashboard. In the v2 form, `E` is `spec.elements.<key>.spec` (one panel),
and `Q` is one query of the panel (`E.data.spec.queries[i].spec`).

In the v2 form, the members of the plugin are in `Q.query.spec`: `rawSql`, `format`, `editorType`, `queryType`,
`builderOptions` and `meta`. In the v1 form, they are in the target. The query table below names them without a prefix.

A mapped value can break a range of section 4: a length, a count or a number. Then the item is `unsupported`. The fallback
of its row applies; a row without a fallback gives "the dashboard is not imported".

Dashboard members:

| v2 | v1 | Class | Hune | Reason or fallback |
|---|---|---|---|---|
| `metadata.name` | `uid` | mapped | `uid` and its location (4.5) | A uid outside the rule of 4.3 is `changed` to a new uid (15.8). |
| `spec.title` | `title` | mapped | `title` (4.5) | |
| `spec.description` | `description` | mapped | `description` (4.5) | |
| `spec.tags` | `tags` | mapped | `tags` (4.5) | Two equal tags are `changed` to one tag. |
| `spec.timeSettings.from`, `.to` | `time.from`, `time.to` | mapped | `time.from`, `time.to` (4.6) | Text outside the grammar of 4.4 is `unsupported`; fallback: `now-6h` and `now`. |
| `spec.timeSettings.timezone` | `timezone` | mapped | `time.timezone` (4.6) | `browser` and an empty text give `browser`. `utc` and `UTC` give `utc`. A name of the form `area/place` (4.6) maps as it is: the importer does not check that the zone exists, and the page does (9.9). `default` is `changed` to `browser`. Another text (`GMT`, `EST`, a name with a blank) is `unsupported`; fallback: `browser`. |
| `spec.timeSettings.autoRefresh` | `refresh` | mapped | `time.refresh` (4.6) | Empty gives `null`. A value of `REFRESH_CHOICES` (`10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `1d`) maps. Another duration is `changed` to the nearest larger choice (example: `5s` gives `10s`, `2m` gives `5m`); a duration above `1d` is `changed` to `1d`. A text that is not a duration is `unsupported`; fallback: `null`. |
| `spec.timeSettings.autoRefreshIntervals`, `hideTimepicker`, `fiscalYearStartMonth`, `weekStart`, `nowDelay` | `timepicker`, `fiscalYearStartMonth`, `weekStart` | dropped | none | Metadata when the value is the Grafana default; else listed. Hune has no such setting. |
| `spec.cursorSync` | `graphTooltip` | mapped | `cursor_sync` (4.5) | `Off` or 0 gives `off`. `Crosshair` or 1 gives `crosshair`. `Tooltip` or 2 is `changed` to `crosshair`. |
| `spec.links` | `links` | mapped | `links` (4.5, 4.5a) | Each item maps as the rows below say. An absent member gives `[]`. More than 16 items: each item after the 16th is `unsupported`; fallback: item removed. |
| link item, `type` = `link` | same | mapped | kind `link` | `title`, `url`, `targetBlank`, `includeVars` and `keepTime` give `title`, `url`, `target_blank`, `include_vars` and `keep_time`. An absent flag gives `false`. An empty title is `changed` to `Link`. A URL that breaks the prefix rule of 4.23 is `unsupported`; fallback: item removed. The URL follows the rewrite of `--link-prefix`, as a panel link does. |
| link item, `type` = `dashboards` | same | mapped | kind `dashboards` | `title`, `tags`, `asDropdown`, `targetBlank`, `includeVars` and `keepTime` give `title`, `tags`, `as_dropdown`, `target_blank`, `include_vars` and `keep_time`. An empty title with `asDropdown` = `true` is `changed` to `Dashboards`. More than 8 tags, or a tag that breaks 4.5a, is `unsupported`; fallback: item removed. Two equal tags are `changed` to one tag. |
| link item, `tooltip` and `icon` | same | dropped | none | Metadata when the text is empty or the default icon (`external link` for `link`, `dashboard` for `dashboards`). Else it is listed. |
| link item, another `type` or another member | same | unsupported | none | Fallback: item removed. |
| `spec.annotations`, the built-in item | `annotations.list` item with `builtIn` = 1 | dropped | none | Metadata. Hune has no alert and no stored annotation. The item shows the events that Grafana keeps in its own database, and they are not in the dashboard file. |
| `spec.annotations[]` with a ClickHouse or a PostgreSQL data source of the source map and a query | `annotations.list` item, `target.rawSql` | mapped | one item of `annotations` (4.5b) | `name`, the source (the source map, as for a panel query), `sql` (`target.rawSql` as lines, checked with the dialect of the kind: 7.18 or 7.26), `color` (`iconColor`), `enabled` (`enable`) and `panels` (`filter`). More than 8 items: each item after the 8th is `unsupported`; fallback: item removed. A `name` that repeats is `changed` to a new name (`<name> 2`). |
| item, `iconColor` | `iconColor` | mapped | `color` | A token or a colour of 4.19 maps. An absent, empty or other value is `changed` to `#808080`, the colour that Grafana draws for a missing value. |
| item, `enable` | `enable` | mapped | `enabled` | `false` gives `enabled` = `false`. |
| item, `filter` with `exclude` = `false` and `ids` | `filter` | mapped | `panels` | The ids of the panels of the type `timeseries` that exist in the imported document map. An id of a panel of another type or of a panel that does not exist is `changed`: it leaves the list. An empty list is `unsupported`; fallback: item removed. A list of more than 200 ids is `unsupported` too; the importer does not read it. |
| item, `filter` with `exclude` = `true` | `filter` | changed | `panels` | The importer lists each `timeseries` panel that the filter does not exclude. A panel that a person adds later is not in the list. An empty list is `unsupported`; fallback: item removed. |
| item, an absent `filter` | none | mapped | `panels` absent | The item draws on each time series. |
| item, `hide` | `hide` | dropped | none | Metadata when `true`. When `false`, it is listed: the page has no toggle for an annotation. |
| item, `mappings` with a member | `mappings` | unsupported | none | Hune reads the columns by their names only (7.37). Fallback: item removed. The report names the mapped fields. |
| `spec.annotations[]` with a MongoDB data source of the source map and a query | `annotations.list` item, `target.queryText`, `target.collection`, `target.database` | mapped | one item of `annotations` (4.5b) | The members of the row above. `sql` is the object of 7.29 that the importer builds from `queryText`, `collection` and `database`, as for a panel query of this plugin (its rows of the query members). The checks of 7.36 run with the use `annotation`. A fault of the query is `unsupported`; fallback: item removed. |
| item of a MongoDB data source, `target.queryType` other than `table` | `target.queryType` | changed | none | Hune runs the query of an annotation with the format `table` (7.37). |
| item of a PostgreSQL data source, `target.editorMode` = `builder` with a `rawSql` | `target.editorMode` | unsupported | none | Hune has no query builder for PostgreSQL. Fallback: the `rawSql` stays as the SQL text, and the builder members are removed. The members of a code query (`editorMode` = `code`, `rawQuery`, `sql`, `table`) are metadata. |
| item, `target.rawSql` with `$__interval`, `$__interval_ms`, `$__timeInterval` or the like, or `target.queryText` with `$__interval_ms` or `$__interval_s` | `target.rawSql`, `target.queryText` | unsupported | none | 7.18 item 5, 7.26 and 7.36 item L9 refuse these macros in an annotation. Fallback: item removed. |
| item, a data source of another type, or a data source that is not in the map (Prometheus, Loki, "-- Grafana --" with tags, and the like) | `datasource` | unsupported | none | An annotation runs on a ClickHouse, a PostgreSQL or a MongoDB source of the map. Fallback: item removed. |
| item, no `target.rawSql` (a SQL data source), or no `target.queryText` or `target.collection` (a MongoDB data source) | `target` | unsupported | none | The item has no query. Fallback: item removed. |
| item, `target.format` other than `table` | `target.format` | changed | none | Hune runs the query of an annotation with the format `table` (7.37). |
| item, another member (`type`, `showIn`, `limit`, `tags`, `expr`, `target.*` that Hune does not read, a member of the plugin of another kind) | the same | unsupported | none | Fallback: member removed. The default rule of 15.5 applies, and a value `false`, `0`, `""`, `[]` or `{}` is metadata. |

The columns of the result are the columns of 7.37. A query for Grafana that selects `timeEnd` works as it is in Hune. A query that selects `time_end` or `end` gets no region in both products. A query that has no `text` column shows the first text column in Grafana, and it shows no text in Hune.

A query that returns HTML in the text gives the error `bad_chars` in Hune. The import runs no query (19.20e). So the report adds one `changed` item at the SQL of each mapped item. Its text is "the SQL of the annotation `<name>` is not checked against the rules of 7.37".

The v2 form has the same items in `spec.annotations[].spec`. The query is in `.spec.query.spec`, and the members `name`, `enable`, `hide`, `iconColor`, `filter` and `mappings` have the names of the v1 form. The form v1 was read on exports of Grafana: the built-in item and an item with no query. It was also read on the measure of 19.20e. No export that the project has seen holds a v2 annotation with a query. So the paths of the v2 form follow the schema, and they are not verified (open question Q1).
| `spec.editable`, `spec.liveNow`, `spec.preload` | `editable`, `liveNow`, `id`, `version`, `schemaVersion`, `iteration`, `gnetId`, `__inputs`, `__requires`, `__elements` | dropped | none | Metadata. |
| none | `__inputs` item with `type` = `datasource` that `--source-map` names | changed | none | The item leaves the document. Each reference `${name}` to it in a member `datasource` gets the mapped source (15.9). Another item of `__inputs` is metadata. |
| `metadata`, other members | `meta`, other members | dropped | none | Metadata: resource version, generation, times, labels, other annotations. |
| `spec.variables` | `templating.list` | see below | `variables` (4.7 to 4.11) | |

Layout members:

| v2 | v1 | Class | Hune | Reason or fallback |
|---|---|---|---|---|
| `spec.layout` of kind `RowsLayout`, each `RowsLayoutRow` | each panel with `type` = `row` | mapped | one item of `sections` (4.12) | |
| `RowsLayoutRow.spec.title` | row `title` | mapped | `sections[].title` | An empty title gives `null` in the first section. In another section, it is `changed` to the text `Row`. |
| `RowsLayoutRow.spec.hideHeader` = `true` on the first row | none | mapped | `sections[0].title` = `null` | The Grafana converter puts the panels above the first row into such a row: it is the section without a header of 4.12. |
| `RowsLayoutRow.spec.hideHeader` = `true` on another row | none | unsupported | none | 4.12 permits `title` = `null` in the first section only. Fallback: the header shows. |
| `RowsLayoutRow.spec.hideHeader` = `false` | none | dropped | none | Metadata. |
| `RowsLayoutRow.spec.collapse` | row `collapsed` | mapped | `sections[].collapsed` | A section with `title` = `null` gets `false` (4.12); a `true` value there is `changed`. |
| `spec.layout` of kind `GridLayout` | panels before the first row | mapped | one section with `title` = `null` | |
| `GridLayoutItem.spec.x`, `.y`, `.width`, `.height` | `gridPos.x`, `.y`, `.w`, `.h` | mapped | `grid.x`, `grid.y`, `grid.w`, `grid.h` (4.14) | Same units. The v1 `y` is converted (rule below). |
| `RowsLayoutRow.spec.repeat` (`mode`, `value`) | row `repeat` | mapped | `sections[].repeat.variable` (4.13a) | `mode` MUST be `variable`, and the variable MUST be a `query` or `custom` variable of the document that has `multi` = `true`. Else the item is `unsupported`; fallback: no repeat. A variable with `multi` = `false` is refused and the importer never changes it. A row that is the first section with `hideHeader` = `true` is `unsupported`; fallback: no repeat (4.13a, rule 7). |
| `GridLayoutItem.spec.repeat` (`mode`, `value`, `direction`, `maxPerRow`) | panel `repeat`, `repeatDirection`, `maxPerRow` | mapped | `panels[].repeat` (4.13a) | The variable follows the rule of the row above. `direction` and `repeatDirection` map the values `h` and `v`. An absent or `null` value is `changed` to `h`. Another value is `unsupported`; fallback: no repeat. |
| `maxPerRow` of a panel | `maxPerRow` | mapped | `repeat.max_per_row` | A whole number from 1 to 24 with direction `h` maps. With direction `v` the value is metadata, because Grafana ignores it. A value outside 1 to 24 is `unsupported`; fallback: member removed. |
| A panel with `repeat` that shares a row unit with another panel | the same | unsupported | none | Grafana moves the other panels. Hune needs free rows for the direction `h` (4.13a, rule 5). Fallback: no repeat. The report says "no repeat: the panel shares a row unit with another panel". |
| A panel with `repeat` in a row that has `repeat`, a panel with `repeat` that is a `reuse` panel | the same | unsupported | none | Hune has one level of repeat and no repeat of a reuse panel (4.13a, rules 7 and 8). Fallback: no repeat on the panel. The repeat of the row stays. |
| More than 200 panels in all the sections, after the panels that the importer removes | the same | unsupported | none | 4.13, rule 2. Fallback: the dashboard is not imported. The importer then does not do the checks of the repeat across panels (the two rows above), because they compare the panels in pairs. The report says "the dashboard has N panels". |
| `repeatIteration`, `repeatPanelId`, `repeatedByRow`, `scopedVars` | the same | dropped | none | Metadata. Grafana writes them in the copies that it makes. A panel that has `repeatPanelId` is a stored copy: the importer removes it, and the report lists it. |
| `spec.layout` of kind `TabsLayout` or `AutoGridLayout` | none | unsupported | none | Fallback: the dashboard is not imported. |

The v1 `y` rule: a panel that follows a row panel at `y = r` gets `grid.y = y - r - 1`. A panel before the first row keeps
its `y`. A collapsed v1 row holds its panels in its member `panels`. The same rule applies to them. In the v2
form, `y` is already relative to its row.

A row whose panels have the smallest `y` equal to `r` is an exception. Grafana draws such panels on the first line of the
section (measured on Grafana 12.4.2). Then each panel of that row gets `grid.y = y - r`, and the row has one item of class
`changed`. The order of the panels and the overlap rules of 4.14 do not change. A panel with a `y` below `r` stays
`unsupported`; fallback: the dashboard is not imported.

Panel members:

| v2 | v1 | Class | Hune | Reason or fallback |
|---|---|---|---|---|
| `E.id` | `id` | mapped | `id` (4.13) | Two panels with one id in a dashboard: `unsupported`; fallback: the dashboard is not imported. |
| `E.title`, `E.description` | `title`, `description` | mapped | `title`, `description` (4.13) | |
| `E.vizConfig` type: `kind` (v2alpha1) or `group` (v2beta1) | `type` | mapped | `type` (4.13) | The types of 4.13 map. The types `state-timeline` and `status-history` have the rows below. The type `text` has the rows below. The types `heatmap` and `histogram` have the rows below. The type `gauge` has the rows below. The type `logs` has the rows below. Another type is `unsupported`; fallback: panel removed. |
| `E.vizConfig.spec.pluginVersion` (v2alpha1), `E.vizConfig.version` (v2beta1) | `pluginVersion` | dropped | none | Metadata. |
| `E.vizConfig.spec.options` | `options` | mapped | `options` (4.26 to 4.33d) | Each member of the option table of the type takes its value from the "Grafana source" of that table, with the value tables below. An absent source gives the Grafana default of that option. A Grafana option that no table names is `unsupported`; fallback: member removed. A Grafana value that no value table names is `unsupported`; fallback: the Grafana default of that option. |
| `E.vizConfig.spec.fieldConfig.defaults.custom` members that 4.28, 4.30 and 4.32 name as a source (`drawStyle`, `lineInterpolation`, `lineWidth`, `fillOpacity`, `showPoints`, `pointSize`, `spanNulls`, `stacking.mode`, `scaleDistribution.type`, `scaleDistribution.log`, `thresholdsStyle.mode`, `gradientMode`, `barAlignment`, and `show` of an `xychart`) | the same members of `fieldConfig.defaults.custom` | mapped | members of `options` (4.28, 4.30, 4.32) | In an override of a `timeseries`, five members map to the set of the override. They are `drawStyle`, `lineWidth`, `fillOpacity`, `showPoints` and `pointSize` (the rows of the field properties below). Another member in an override is `unsupported`; fallback: property removed. The five members in an override of another type are `unsupported` too. The value tables below give `thresholdsStyle.mode`, `scaleDistribution` and `pointSize`. In a panel of another type, `thresholdsStyle.mode` = `off` is metadata, and another value is `unsupported`. In an `xychart`, a `scaleDistribution` that is not the default is `unsupported` (4.32 has no `y_scale`); fallback: property removed. |
| `E.vizConfig.spec.fieldConfig.defaults.custom.wrapText`, and `cellOptions.wrapText` in the defaults (Grafana 11) | the same members | mapped | `options.wrap` of a `table` (4.27) | `true` and `false` map. In an override, the member is `unsupported`; fallback: property removed. In another panel type, `false` is metadata, and `true` is `unsupported`. |
| `E.vizConfig.spec.fieldConfig.defaults.custom.wrapHeaderText` | the same member | dropped | none | Metadata when `false`. Else it is `unsupported`; fallback: property removed. The header of Hune never wraps (9.20). |
| `E.vizConfig.spec.options.footer` of a `table` (Grafana 11 and older) | `options.footer` | changed | the property `footer` (4.18) | Metadata when `show` is not `true`. Else the importer moves the footer as Grafana 12.4.2 moves it, with one `changed` item. The ids of `reducer` give `footer.reducers` by the row `custom.footer` of the field properties. When `fields` is absent, `""` or `[]`, the property goes in `fields.defaults`. One name in `fields` gives an override with the matcher `name`. Two names or more give the matcher `names` with the mode `include`. With `countRows` = `true` and `count` as the first reducer, the property is `footer.reducers` = `["count"]` in `fields.defaults`. Then Grafana shows the row count in the first column only, and Hune shows it in each column. Another value of `fields`, or more than 64 different names, is `unsupported`; fallback: footer removed. The member `enablePagination` = `true` is `unsupported`, also when `show` is not `true`: Hune has no pages of rows. Its fallback: the panel shows all the rows. |
| `E.links` | `links` (panel links) | mapped | `links` (4.13, 4.23) | `targetBlank` gives `target_blank`. A URL that breaks the prefix rule of 4.23, or a token outside 9.18, is `unsupported`; fallback: link removed. The importer removes each U+FEFF of the URL, with no item: the link editor of Grafana writes it around a token, and Grafana 12.4.2 makes the link without it. |
| `E.vizConfig` type `text`, `options.mode` = `markdown` | `type`, `options.mode` | mapped | `type` = `text`, `options.mode` = `markdown` (4.33a) | The same rows apply in the v2 forms. A text panel has no query in Hune. |
| `options.mode` absent, or another text of Grafana that is not `html` or `code` | same | changed | `options.mode` = `markdown` | Grafana reads such a mode as `markdown`. The item is listed. |
| `options.content` | same | mapped | `options.content`, the lines (4.33a) | The text is cut at `\n`. A text with `\r\n` or `\r` is `changed`: each one becomes a line end. A control character other than U+0009 and U+000A is `unsupported`; fallback: the character is removed. A text above a limit of 4.35 is `unsupported`; fallback: panel removed. |
| `options.content` absent | same | changed | `options.content` = `[]` | Grafana shows its default text. Hune shows an empty panel. The item is listed. |
| `options.content` of a panel with `mode` `markdown` that has a construct that Hune shows in another way | same | changed | the content as it is | The report lists the first construct and its line. The constructs are in the list below this table. The panel keeps its text. |
| `options.mode` = `html` | same | unsupported | `options.mode` = `markdown`, with the HTML as a code block | Hune has no HTML mode (4.33a, rule 4). Fallback: the content becomes a code block, so the page shows the HTML as text. The fence has 3 backticks, or one more than the longest run of backticks in the content. It has no info string. |
| `options.mode` = `code` | same | changed | `options.mode` = `markdown`, with the content as a code block | The fence follows the rule of the row above. Hune does not colour the code. |
| `options.code` (`language`, `showLineNumbers`, `showMiniMap`) | same | dropped | none | Metadata when `mode` is `markdown`, or when the values are `plaintext`, `false` and `false`. Else it is listed. |
| `repeat`, `repeatDirection` and `maxPerRow` of a text panel | same | unsupported | none | A text panel does not repeat (4.33a, rule 6). Fallback: no repeat. |
| `targets`, `datasource`, `interval`, `maxDataPoints`, `timeFrom`, `timeShift`, `hideTimeOverride`, `cacheTimeout`, `fieldConfig` and `transformations` of a text panel | same | dropped | none | Metadata when the value is empty or the default of Grafana. Else it is listed. A text panel has no query and no field. |
| `E.vizConfig` type `state-timeline`, `status-history` | `type` | mapped | `type` = `state_timeline`, `status_history` (4.33b) | The same rows apply in the v2 forms. The query rows of the panel map as for a time series. |
| `options.mergeValues` (`state-timeline`) | same | mapped | `options.merge_values` | `true` and `false` map. An absent member gives `true`. In a `status-history` the member is metadata. |
| `options.showValue` | same | mapped | `options.show_values` | `auto`, `always` and `never` map. An absent member gives `auto`. |
| `options.alignValue` (`state-timeline`) | same | mapped | `options.align_values` | `left`, `center` and `right` map. An absent member gives `left`. |
| `options.rowHeight` | same | mapped | `options.row_height` | A number from 0.1 to 1, times 100, rounded to the nearest integer. A value outside the range is `unsupported`; fallback: 90. An absent member gives 90. |
| `options.colWidth` (`status-history`) | same | mapped | `options.col_width` | The same rule as `rowHeight`. |
| `options.perPage` | same | unsupported | none | Hune has no pages of rows. Fallback: the panel shows all the rows. The item is listed. |
| `options.legend` | same | mapped | `options.legend` | `showLegend` and `placement` map as in the value table of 15.6. `displayMode` `table` is `changed` to `list`. A `calcs` list that is not empty, `sortBy` and `sortDesc` are `unsupported`; fallback: member removed. |
| `options.tooltip` | same | mapped | `options.tooltip` | `mode` maps. `sort` `asc` and `desc` are `changed` to `none`. `hideZeros` `true` is `changed` to `false`. `maxHeight` and `maxWidth` are metadata when absent, else they are listed. |
| `options.timezone`, `options.annotations` | same | dropped | none | Metadata when empty. Else it is listed. Hune has one zone for the dashboard and no annotation. |
| `fieldConfig.defaults.custom.lineWidth`, `.fillOpacity` | same | mapped | `options.line_width`, `options.fill_opacity` | The ranges of 4.33b apply. A value out of range is `unsupported`; fallback: the default of the type. In an override the member is `unsupported`; fallback: property removed. |
| `fieldConfig.defaults.custom.spanNulls` (`state-timeline`) | same | mapped | `options.span_nulls` | `true` and `false` map. A number (a time threshold) is `unsupported`; fallback: `false`. In a `status-history` the member is metadata. |
| `fieldConfig.defaults.custom.insertNulls` | same | unsupported | none | `false` is metadata. `true` or a number is `unsupported`; fallback: member removed. Hune has no such option (19.20d). |
| `fieldConfig.defaults.custom.axisPlacement` | same | mapped | `options.show_labels` | `hidden` gives `false`. `auto` is metadata and gives `true`. Another value is `unsupported`; fallback: `true`. The time axis stays (19.20d). |
| `fieldConfig.defaults.color`, `thresholds`, `mappings`, `unit`, `decimals`, `min`, `max`, `links`, `displayName`, `noValue` and `custom.hideFrom` | same | mapped | `fields` (4.18) | The rows of the field properties apply. An absent `color` gives the default mode of the type, which is the Grafana default (9.14). |
| `fieldConfig` members of a `timeseries` (`drawStyle`, `lineInterpolation`, `showPoints`, `pointSize`, `stacking`, `axisLabel` and the others) in these panels | same | dropped | none | Metadata when the value is the default of Grafana. Else it is listed. |
| `E.vizConfig` type `heatmap`, `histogram` | `type` | mapped | `type` = `heatmap`, `histogram` (4.33c, 4.33d) | The same rows apply in the v2 forms. The query rows of the panel map as for a time series. |
| A panel `heatmap` of Grafana 7 to 9, with a member `cards`, `dataFormat`, `xBucketSize`, `xBucketNumber`, `yBucketSize`, `yBucketNumber`, `yBucketBound`, `hideZeroBuckets` or `reverseYBuckets` at the top level of the panel | same | unsupported | none | These panels have no `options.calculate`. Hune reads the form of Grafana 10 and later only. Fallback: panel removed. The report names the first member that it found. Grafana changes such a panel when it loads the dashboard, so an export from its page is in the new form. |
| `options.calculate` (`heatmap`) | same | mapped | `options.calculate` | `true` and `false` map. An absent member gives `false`. |
| `options.calculation.xBuckets.mode`, `.value` | same | mapped | `options.x_buckets` | The modes `count` and `size` map. An absent mode gives `size` (the Grafana rule), and an absent or empty value gives the mode `count` with `null`. With `count`, a whole number text from 1 to 1,000 maps. With `size`, a length in the form of 4.4 or a whole number of milliseconds maps, from 1 ms to 365 days. Another value is `unsupported`; fallback: `count` with `null`. |
| `options.calculation.xBuckets.scale` | same | unsupported | none | Grafana refuses a log scale on the x axis. Fallback: member removed. |
| `options.calculation.yBuckets.mode`, `.value`, `.scale` | same | mapped | `options.y_buckets` | `scale.type` `linear` gives `linear`. `log` with the base 2 or 10 gives `log2` or `log10`. The value is then the split: a whole number from 1 to 16, or `null` for an empty value. The mode is `changed` to `size` for a log scale. Any other scale (`symlog`, a base other than 2 and 10) is `unsupported`; fallback: `linear`. With a linear scale the rules of `xBuckets` apply, with 1 to 1,000 for `count` and a number above 0 for `size`. |
| `options.yAxis.axisPlacement` | same | mapped | `options.y_axis` | `left` and `hidden` map. An absent member gives `left`. `right` is `changed` to `left`. |
| `options.yAxis.reverse` | same | mapped | `options.y_reverse` | `true` and `false` map. An absent member gives `false`. |
| `options.yAxis.min`, `.max` | same | mapped | `options.y_min`, `options.y_max` | A number or a number text maps. An empty or absent member gives `null`. Another text is `unsupported`; fallback: `null`. With a log scale a value of 0 or less is `unsupported`; fallback: `null`. |
| `options.yAxis.unit`, `.decimals` | same | changed | `fields.defaults.unit`, `fields.defaults.decimals` | With `calculate` equal to `true`, they move to the defaults when the defaults have no such member. When the defaults have such a member, the item is `unsupported`; fallback: member removed. With `calculate` equal to `false`, Grafana formats the names of the rows with them, and Hune does not: the item is `unsupported`; fallback: member removed. |
| `options.yAxis.axisWidth`, `.axisLabel` | same | dropped | none | Metadata when absent or empty. Else it is listed. The engine sizes the axis (9.24). |
| `options.color.mode` | same | mapped | `options.color.mode` | `scheme` and `opacity` map. An absent member gives `scheme`. |
| `options.color.scheme` | same | mapped | `options.color.scheme` | The names `Oranges`, `Blues`, `Greens`, `Greys`, `Purples`, `Reds`, `YlOrRd`, `RdYlGn`, `Spectral` and `Viridis` map to the 10 values of 4.33c. Another name of the 38 schemes of Grafana is `changed` to `oranges`. A name that Grafana does not know is `changed` to `spectral`, which is the choice of Grafana. An absent member gives `oranges`. |
| `options.color.fill`, `.scale`, `.exponent`, `.steps`, `.reverse` | same | mapped | `options.color.fill`, `.scale`, `.exponent`, `.steps`, `.reverse` | The ranges of 4.33c apply. An `exponent` with more than one decimal is `changed` to the nearest value with one decimal. A value out of range or a colour that 4.19 refuses is `unsupported`; fallback: the default of the member. Absent members give the defaults. |
| `options.color.min`, `.max` | same | mapped | `options.color.min`, `.max` | A number maps. An absent member gives `null`. When both are numbers and `min` is not lower than `max`, both items are `unsupported`; fallback: both `null`. |
| `options.cellGap` | same | mapped | `options.cell_gap` | A whole number from 0 to 25 maps. An absent member gives 1. A value out of range is `unsupported`; fallback: 1. |
| `options.cellRadius`, `options.cellValues.unit`, `.decimals` | same | dropped | none | Metadata when absent, empty or 0. Else it is listed. Hune draws square cells and writes the count in the short form. |
| `options.filterValues.le`, `.ge` | same | mapped | `options.hide_le`, `options.hide_ge` | A number maps. The default of Grafana for `le`, `1e-9`, gives 0 (19.20g). An explicit `null` gives `null`. An absent `ge` gives `null`. |
| `options.legend.show` (`heatmap`) | same | mapped | `options.show_legend` | `true` and `false` map. An absent member gives `true`. |
| `options.tooltip` (`heatmap`) | same | mapped | `options.tooltip` | `mode` maps, and `none` gives `hidden`. `yHistogram` and `showColorScale` equal to `true` are `unsupported`; fallback: member removed. `maxHeight` and `maxWidth` are metadata when absent, else they are listed. |
| `options.rowsFrame.layout` | same | changed | none | `auto` and `unknown` are metadata, because Hune puts a label at the middle of its row. `le` and `ge` are listed: the label stays at the middle. |
| `options.rowsFrame.value`, `.yBucketScale` | same | unsupported | none | Fallback: member removed. The axis of a frame that is already bucketed is ordinal (19.20g). |
| `options.selectionMode`, `options.exemplars`, `options.showValue` | same | dropped | none | Metadata when the value is the default of Grafana. Else it is listed. Hune has no selection by drag, no exemplar and no value in the cell. |
| `options.bucketCount` (`histogram`) | same | mapped | `options.bucket_count` | A whole number from 1 to 1,000 maps. An absent member gives 30. A value out of range, 0 included, is `unsupported`; fallback: 30. |
| `options.bucketSize` | same | mapped | `options.bucket_size` | A number above 0 maps. An absent member, 0 and the empty text give `null`. Another value is `unsupported`; fallback: `null`. |
| `options.bucketOffset` | same | mapped | `options.bucket_offset` | A finite number maps. An absent member gives 0. Another value is `unsupported`; fallback: 0. |
| `options.combine` | same | mapped | `options.combine` | `true` and `false` map. An absent member gives `false`. |
| `options.legend`, `options.tooltip` (`histogram`) | same | mapped | `options.legend`, `options.tooltip` | As in the rows of a `barchart`. |
| `fieldConfig.defaults.custom.stacking.mode` (`histogram`) | same | mapped | `options.stacking` | `none`, `normal` and `percent` map. An absent member gives `none`. `group` is metadata. |
| `fieldConfig.defaults.custom.lineWidth`, `.fillOpacity` (`histogram`) | same | mapped | `options.line_width`, `options.fill_opacity` | The ranges of 4.33d apply. A value out of range is `unsupported`; fallback: the default. Absent members give 1 and 80. In an override the member is `unsupported`; fallback: property removed. |
| `fieldConfig.defaults.custom.gradientMode` (`histogram`) | same | unsupported | none | `none` is metadata. Another value is `unsupported`; fallback: member removed. The bars have no gradient. |
| `fieldConfig.defaults.color`, `thresholds`, `mappings`, `unit`, `decimals`, `min`, `max`, `links`, `displayName`, `noValue` and `custom.hideFrom` of these panels | same | mapped | `fields` (4.18) | The rows of the field properties apply. An absent `color` gives the default mode of the type (9.14). |
| `fieldConfig.defaults.custom` members of a `timeseries` or of the axes (`drawStyle`, `showPoints`, `axisPlacement`, `axisLabel`, `axisWidth`, `axisSoftMin`, `axisSoftMax`, `scaleDistribution` and the others) in these panels | same | dropped | none | Metadata when the value is the default of Grafana. Else it is listed. |
| `E.vizConfig` type `gauge` | `type` | mapped | `type` = `gauge` (4.33e) | The classic gauge of Grafana 12.4.2. The same rows apply in the v2 forms. The query rows of the panel map as for a `stat`. |
| `options.reduceOptions` (`gauge`) | same | mapped | `options.reduce` | As in the rows of a `stat`. |
| `options.orientation` (`gauge`) | same | mapped | `options.orientation` | `auto`, `horizontal` and `vertical` map. An absent member gives `auto`. |
| `options.showThresholdLabels`, `.showThresholdMarkers` | same | mapped | `options.show_threshold_labels`, `options.show_threshold_markers` | `true` and `false` map. Absent members give `false` and `true`. |
| `options.text.titleSize`, `.valueSize` (`gauge`) | same | mapped | `options.title_size`, `options.value_size` | A whole number from 1 to 200 maps. An absent member or `null` gives the automatic size. Another value is `unsupported`; fallback: member removed. Another member of `text` is listed. |
| `options.sizing` (`gauge`) | same | mapped | `options.sizing` | `auto` and `manual` map. An absent member gives `auto`. Another value is `unsupported`; fallback: `auto`. |
| `options.minVizWidth`, `.minVizHeight` (`gauge`) | same | mapped | `options.min_width`, `options.min_height` | With `sizing` = `manual`, a whole number from 0 to 600 maps. Another value is `unsupported`; fallback: 75. With `sizing` absent or `auto`, the members are metadata, because Grafana uses 75 px. |
| `fieldConfig.defaults.custom.neutral` (`gauge`) | same | mapped | `options.neutral` | A finite number maps. Another value is `unsupported`; fallback: member removed. In an override the member is `unsupported`; fallback: property removed. |
| Another member of `options` of a `gauge`, for example a member of the new gauge of Grafana (`shape`, `barWidthFactor`, `segmentCount`, `effects`) | same | unsupported | none | Metadata when the value is trivial (15.5). Else it is `unsupported`; fallback: member removed. |
| `E.vizConfig` type `logs` | `type` | mapped | `type` = `logs` (4.33f) | The same rows apply in the v2 forms. The query rows of the panel map as for a time series. The format 2 of the plugin gives `logs` (see the rows of the format). A logs panel keeps its first query: each other target is `unsupported`; fallback: the item is removed (4.15a, rule 7). |
| `options.showTime`, `options.wrapLogMessage`, `options.enableLogDetails` (`logs`) | same | mapped | `options.show_time`, `options.wrap`, `options.details` | `true` and `false` map. An absent member gives the default of 4.33f: `false`, `false` and `true`. Another value is `unsupported`; fallback: the Grafana default of that option. |
| `options.sortOrder` (`logs`) | same | mapped | `options.order` | `Descending` gives `newest_first`, and `Ascending` gives `oldest_first`. An absent member gives `newest_first`. Another value is `unsupported`; fallback: the Grafana default of that option. |
| `options.dedupStrategy`, `options.fontSize`, `options.detailsMode`, `options.timestampResolution` (`logs`) | same | unsupported | none | `none`, `default`, `inline` and `ms` are metadata. Another value is `unsupported`. Fallbacks: each line shows (no deduplication), the default size, the details row under the line, and the time with milliseconds. |
| `options.showLabels`, `options.showCommonLabels`, `options.prettifyLogMessage`, `options.enableInfiniteScrolling`, `options.showControls`, `options.unwrappedColumns`, `options.syntaxHighlighting` (`logs`) | same | unsupported | none | `false` is metadata. Another value is `unsupported`; fallback: member removed. Hune has no labels, no JSON format, no infinite scroll, no controls, no columns of displayed fields and no highlight of the text (9.19h). |
| `E.vizConfig` type `geomap` | `type` | mapped | `type` = `geomap` (4.33g) | The same rows apply in the v2 forms. A map draws the frame of one query: the importer keeps the first query, as for a `barchart`. |
| `options.basemap` (`geomap`) | same | changed | none | The document holds no tile source. Each base layer of Grafana, the default layer of CARTO included, leaves the document. The tile source of Hune is the block `geomap` of the configuration (13.12c). Without it, the map has a plain background. |
| `options.layers` (`geomap`) | same | mapped | `options.layer` (4.33g) | The first layer of the type `markers` maps. A layer `xyz` is a tile source: it is `changed` and leaves the document. Each other layer, and a second layer of markers, is `unsupported`; fallback: layer removed. No layer of markers gives the default layer of 4.33g, with a `changed` item. |
| `layers[i].location` (`geomap`) | same | mapped | `layer.lat_field`, `layer.lon_field` | `auto` gives `null` and `null`. `coords` gives the two names. `geohash` and `lookup` are `unsupported`; fallback: the fields `lat` and `lon`. |
| `layers[i].config.style.size`, `.color`, `.opacity` (`geomap`) | same | mapped | `layer.size`, `size_min`, `size_max`, `size_field`, `color`, `color_field`, `opacity` | The ranges of 4.33g apply. A size with a fraction is rounded, with a `changed` item. A largest size below the smallest size becomes the smallest size, with a `changed` item. The opacity is times 100. A value out of range is `unsupported`; fallback: the default of Grafana (5, 2, 15, `dark-green`, 0.4). A size mode `quad` is `unsupported`; fallback: linear. |
| `layers[i].config.style.symbol`, `.text`, `.rotation`, `.symbolAlign` (`geomap`) | same | unsupported | none | A circle, no text, no rotation and the centre on the position are metadata. Another value is `unsupported`: Hune draws each marker as a circle. |
| `layers[i].config.showLegend` (`geomap`) | same | changed | none | `true` is `changed`: Hune draws no legend of the layer, and the card of a marker gives its values. `false` is metadata. |
| `layers[i].tooltip` (`geomap`) | same | changed | `options.tooltip` | `false` gives `none`, with a `changed` item: the card shows after a click. `true` maps. |
| `layers[i].filterData` (`geomap`) | same | unsupported | none | Fallback: member removed. The layer takes the frame of the first query. |
| `options.view` (`geomap`) | same | mapped | `options.view` | `zero`, `coords` and each preset of Grafana 12.4.2 give `fixed` with the centre of the view or of the preset. `fit` gives `fit`, with `zoom`, else `maxZoom`, else 15 as the highest zoom. A zoom with a fraction is rounded, with a `changed` item. An absent zoom gives 1. A padding other than 5 is `changed`. `shared` and `minZoom` are `unsupported`; fallback: member removed. `allLayers`, `lastOnly`, `layer` and `noRepeat` are metadata. |
| `options.controls` (`geomap`) | same | mapped | `options.show_zoom`, `options.mouse_wheel_zoom` | `showZoom` and `mouseWheelZoom` map; absent gives `true`. `showAttribution` = `false` is `changed`: the attribution of the tile source always shows (9.19i). `showScale`, `showMeasure` and `showDebug` = `true` are `unsupported`; fallback: member removed. `scaleUnits` is metadata. |
| `options.tooltip.mode` (`geomap`) | same | mapped | `options.tooltip` | `details` and `none` map. An absent member gives `details`. |
| `E.transparent` | `transparent` | mapped | `transparent` (4.13) | Absent or `null` gives `false`. The number 0 or 1 gives `false` or `true`. Another value is `unsupported`; fallback: `false`. |
| `E.data.spec.queryOptions.interval` | `interval` | mapped | `data.query.min_interval` (4.15) | Grafana text `<n><unit>` or `><n><unit>`, with a unit of 4.4, from `1ms` to `1d`. Empty gives `null`. Other text is `unsupported`; fallback: `null`. |
| `E.data.spec.queryOptions.maxDataPoints` | `maxDataPoints` | mapped | `data.query.max_data_points` (4.15) | Absent gives `null`. |
| `E.data.spec.queryOptions.timeFrom` | `timeFrom` | mapped | `data.query.time_from` (4.15) | `<count><unit>` with a unit of `s`, `m`, `h`, `d`, `w`, `M` or `y` gives `now-<count><unit>`. A text of the form `now-<count><unit>`, `now-<count><unit>/<unit>` or `now/<unit>` maps as it is. An empty text gives no member. Another text (`now`, `now+1h`, a variable) is `unsupported`; fallback: member removed. |
| `E.data.spec.queryOptions.timeShift` | `timeShift` | mapped | `data.query.time_shift` (4.15) | `<count><unit>` maps. An empty text gives no member. Another text (a text with `/`, with `now`, with a sign or with a variable) is `unsupported`; fallback: member removed. |
| `E.data.spec.queryOptions.hideTimeOverride` | `hideTimeOverride` | mapped | `data.query.hide_time_override` (4.15) | `true` maps. `false` and an absent member are metadata. Another value is `unsupported`; fallback: member removed. |
| the three members above in a panel with `-- Dashboard --` | same | unsupported | none | A `reuse` has no time member (4.15). Fallback: member removed. The panel uses the range of its source. |
| `E.data.spec.queryOptions.cacheTimeout`, `.queryCachingTTL` | `cacheTimeout`, `queryCachingTTL` | dropped | none | Metadata. Hune has no result cache. |
| element of kind `LibraryPanel` | `libraryPanel` | unsupported | none | Library panels are out of scope. Fallback: panel removed. |
| `E.data.spec.transformations` | `transformations` | see below | `transforms` (4.25) | |

The constructs of the sixth row of the table are these. The scan is a text scan of the lines, so it can list a construct inside a code block. That is a known limit of the report.

- A `<` followed by a letter, `/` or `!`: HTML shows as text.
- `![`: no image shows. The page shows `!` and a link.
- `&` followed by letters or `#`, and then `;`: an entity shows as written.
- `~~`: the marks of a strike show.
- An item that starts with `[ ]` or `[x]`: the marks of a task show.
- A line of `=` or `-` under a line of text: a setext heading shows as text or as a rule.
- A line that starts with 4 spaces or a tab, after a blank line: an indented code block shows as text.
- A line that starts with `[name]:`: a link reference shows as text. `[text][name]` and `[^name]` show as written.
- A link whose destination breaks the rule of 9.19b. Examples are a scheme other than `http` and `https`, a start with `//`, a parenthesis and a character that is not printable ASCII. A `www.` start and a link title are in this item too.
- A list or a quote nested more than 6 levels, or a table with more than 20 columns. This is `unsupported`; fallback: panel removed.
- A placeholder with a format other than `text`, `csv`, `pipe` or `glob`. Also `[[name]]`, a name that starts with `__`, and a placeholder in a destination.

Value tables of the options. A row gives a Grafana value and the value of format 2. A Grafana value that a table does not
name follows the rule of the `options` row above. Some values have the same text in Grafana and in format 2, for example
`drawStyle` `line`, `bars` and `points`. Such a value maps as it is and has no row here.

| Member of format 2 (Grafana source) | Grafana value | Value of format 2 | Class |
|---|---|---|---|
| `legend.show`, `legend.mode` (`legend.showLegend`, `legend.displayMode`) | `showLegend` `true` with `displayMode` `list` or `table` | `show` = `true`, `mode` = `list` or `table` | mapped |
| same | `displayMode` `hidden`, or `showLegend` `false` | `show` = `false`, `mode` = `list` | mapped |
| `legend.placement` (`legend.placement`) | `bottom`, `right` | `bottom`, `right` | mapped (19.20) |
| `legend.values` (`legend.calcs`) | `sum`, `min`, `max`, `mean`, `last`, `lastNotNull`, `first`, `firstNotNull`, `count`, `median`, `variance`, `range`, `diff`, `delta` | `total`, `min`, `max`, `mean`, `last`, `last_not_null`, `first`, `first_not_null`, `count`, `median`, `variance`, `range`, `diff`, `delta` | mapped |
| same | `stdDev`, `diffperc`, `distinctCount`, `changeCount` | `std_dev`, `diff_percent`, `distinct_count`, `change_count` | mapped |
| same | another reducer id | the item is removed | unsupported |
| `legend.sort_by` (`legend.sortBy`) | the title of a column: `Name`, `Last *`, `Last`, `First *`, `First`, `Min`, `Max`, `Mean`, `Median`, `Variance`, `StdDev`, `Total`, `Count`, `Range`, `Delta`, `Difference`, `Difference percent`, `Change Count`, `Distinct Count`. In a `piechart`: `Name`, `Value`, `Percent`. | `name`, `last_not_null`, `last`, `first_not_null`, `first`, `min`, `max`, `mean`, `median`, `variance`, `std_dev`, `total`, `count`, `range`, `delta`, `diff`, `diff_percent`, `change_count`, `distinct_count`. In a `piechart`: `name`, `value`, `percent`. | mapped |
| same | another text | the member is removed | unsupported |
| `legend.sort_desc` (`legend.sortDesc`) | `true`, `false` | `true`, `false` | mapped |
| `tooltip.sort` (`tooltip.sort`) | `none`, `asc`, `desc` | `none`, `asc`, `desc` | mapped |
| `tooltip.hide_zeros` (`tooltip.hideZeros`) | `true`, `false` | `true`, `false` | mapped |
| `legend.values` of a `piechart` (`legend.values`) | `value`, `percent` | `value`, `percent` | mapped |
| `tooltip.mode` (`tooltip.mode`) | `single`, `multi`, `none` | `single`, `multi`, `hidden` | mapped |
| `reduce.calc` (`reduceOptions.calcs`, an array) | one item: `lastNotNull`, `last`, `firstNotNull`, `first`, `min`, `max`, `mean`, `sum`, `count`, `median`, `stdDev`, `variance`, `range`, `diff`, `diffperc`, `delta`, `distinctCount`, `changeCount` | `last_not_null`, `last`, `first_not_null`, `first`, `min`, `max`, `mean`, `total`, `count`, `median`, `std_dev`, `variance`, `range`, `diff`, `diff_percent`, `delta`, `distinct_count`, `change_count` | mapped |
| same | 0 items | `last_not_null`, the Grafana default | changed |
| same | 2 items or more | the first item, by the row above | unsupported; fallback: the first item |
| same | one item with another reducer id | `last_not_null` | unsupported; fallback: `last_not_null` |
| `reduce.fields` (`reduceOptions.fields`) | empty or absent | `null` | mapped |
| same | a text | the text as it is, in the form of 4.24 that 4.26 gives (19.29) | mapped |
| `reduce.values` (`reduceOptions.values`) | `true`, `false` | `true`, `false` | mapped |
| `interpolation` (`custom.lineInterpolation`) | `linear`, `smooth`, `stepBefore`, `stepAfter` | `linear`, `smooth`, `step_before`, `step_after` | mapped |
| `span_nulls` (`custom.spanNulls`) | `true`, `false` | `true`, `false` | mapped |
| same | a number of ms from 1 to 10^12 | `false`, and `span_nulls_ms` = the number rounded up | mapped. Grafana joins a run when the gap is less than the number, so a fraction rounds up for times in whole ms. |
| same | a number of 0 or below | `false` | mapped: Grafana joins no run |
| same | a number above 10^12 | `true` | changed: the threshold joins each run |
| `gradient_mode` (`custom.gradientMode`) of a `timeseries` and a `barchart` | `none`, `opacity`, `hue`, `scheme` | `none`, `opacity`, `hue`, `scheme` | mapped. `null` is the default `none`. |
| same | another value | `none` | unsupported; fallback: `none` |
| `bar_alignment` (`custom.barAlignment`) of a `timeseries` | -1, 0, 1 | `before`, `center`, `after` | mapped. `null` is the default `center`. |
| same | another value | `center` | unsupported; fallback: `center` |
| `color_mode` of `stat` (`colorMode`) | `none`, `value`, `background` | `none`, `value`, `background` | mapped |
| same | `background_solid` | `background` | changed |
| `justify` of `stat` (`justifyMode`) | `auto`, `center` | `auto`, `center` | mapped |
| `percent_change` of `stat` (`showPercentChange`) | `true` | `true` | mapped |
| same | `false` | none | metadata |
| same | another value | none | unsupported; fallback: member removed |
| `percent_change_color` of `stat` (`percentChangeColorMode`) | `inverted`, `same_as_value` | `inverted`, `same_as_value` | mapped |
| same | `standard` | none | metadata |
| same | another value | none | unsupported; fallback: `standard` |
| `title_size`, `value_size` of `stat` (`text.titleSize`, `text.valueSize`) | an integer from 1 to 200 (also written as `20.0`) | the integer | mapped |
| same | `0`, `null`, or a `text` object with only such members (`{}`) | none | metadata. Grafana fits the size. |
| same | another value | none | unsupported; fallback: member removed |
| none (`text.percentSize` of `stat`) | a value | none | unsupported; fallback: member removed. The size of the change follows the value (9.19). |
| none (`wideLayout` of `stat`) | `true` | none | metadata |
| same | `false` | none | unsupported; fallback: member removed. A wide tile of Hune has the name at the left of the value (9.19). |
| none (`sizing`, `minVizHeight`, `minVizWidth`, `maxVizHeight` of `bargauge`) | `sizing` absent or `auto`, and each size | none | metadata. Grafana reads the three sizes only with `sizing` = `manual`. |
| same | `sizing` = `manual`, and each size | none | unsupported; fallback: member removed |
| `thresholds_style` (`custom.thresholdsStyle.mode`) | `off`, `line`, `dashed`, `area` | `off`, `line`, `dashed`, `area` | mapped |
| same | `line+area`, `dashed+area` | `line_area`, `dashed_area` | mapped |
| same | `series` | `off` | unsupported; fallback: `off` |
| `orientation` of `barchart` (`orientation`) | `vertical`, `horizontal`, `auto` | `vertical`, `horizontal`, `auto` | mapped. An absent member gives `auto`, the default of the Grafana panel. |
| `orientation` of `bargauge` (`orientation`) | `horizontal`, `vertical` | `horizontal`, `vertical` | mapped |
| same | `auto` | `horizontal` | changed |
| `bar_width`, `group_width` (`barWidth`, `groupWidth`) | a number from 0.1 to 1 | the number times 100, rounded to the nearest integer | mapped |
| `sort` of `table` (`sortBy`) | items `{displayName, desc}` | items `{field, desc}` | mapped |
| `y_scale` (`custom.scaleDistribution.type`) of a `timeseries` | `linear`, `log` | `linear`, `log` | mapped |
| same | `log` with `log` = 10 | `log` | mapped (the member `log` is metadata) |
| same | `log` with `log` = 2 | `log`, and `log_base` = 2 | mapped |
| same | `log` with another base | `log` | unsupported; fallback: base 10 |
| same | `symlog` | `linear` | unsupported; fallback: `linear`. Hune has no symmetric log scale. |
| `x_field`, `y_fields` of an `xychart` (`options.mapping`, also `seriesMapping`) | `auto` | `x_field` = `null` and `y_fields` = `[]` | mapped |
| same | `manual` or absent: the members `options.series[].x` and `.y` | the first x field and the y fields (4.32) | mapped |
| same | another value of `mapping` | the rule of `manual` | unsupported; fallback: the Grafana default of that option |
| `x_field`, `y_fields` (`options.series[].x`, `.y`) | a text, or the matcher `byName` with a field name | the text | mapped |
| same | another matcher, or a second x field that differs from the first | none | unsupported; fallback: series removed |
| none (`options.series[].frame`) | the matcher `byIndex` with 0 | none | metadata |
| same | another frame | none | unsupported; fallback: series removed |
| `draw` of an `xychart` (`options.series[].show`, `custom.show`) | `points`, `lines`, `points+lines` | `points`, `lines`, `points_and_lines` | mapped. The `show` of a series wins over `custom.show`. |
| `point_size` of an `xychart` (`custom.pointSize`) | a number, or an object with `fixed` | the number, or `fixed` | mapped |
| same | the members `min` = 1 and `max` = 100 of the object | none | metadata |
| same | other values of `min` or `max` | none | unsupported; fallback: the fixed size |

Query members:

| v2 | v1 | Class | Hune | Reason or fallback |
|---|---|---|---|---|
| `E.data.spec.queries` with one item | `targets` with one item | mapped | `data` with the member `query` or `reuse` (4.15) | |
| `E.data.spec.queries` with 2 to 8 items | `targets` with 2 to 8 targets | mapped | `data.query` (the first item) and `data.extra_queries` (the other items in order) (4.15a) | The rows below map each item. The type of the panel MUST be a type of 4.15a, rule 7. For a `barchart` or an `xychart`, the first item stays and the others are `unsupported`; fallback: items removed. |
| more than 8 items | more than 8 targets | unsupported | none | 4.15a, rule 5. Fallback: the items from the 9th are removed. |
| `Q.hidden` = `true` | `hide` = `true` | mapped | `hide` = `true` of the query | The query stays in the document and does not run (4.15a, rule 3). The member `hide` = `false` or an absent member gives no member. The numbers 1 and 0 are the same as `true` and `false`, and `null` is the same as an absent member. Another value, for example the text `"true"` or the number 2, is `unsupported`. Fallback: the query keeps `hide` = `true` and does not run, because Grafana hides such a target. A target of a panel that reuses another panel has no `hide`: the value `true` is `changed`, and the panel shows the frame of its source. |
| `Q.refId` | `refId` | mapped | `ref` | A refId of the grammar of 4.3 maps as it is. The ref `A` of the first query gives no member. A refId that breaks the grammar, or that two queries share, or an absent refId, is `changed`: the importer writes the first free letter of `A` to `Z` and lists the item. A panel with one query writes no `ref`, whatever its refId, so a document that the import makes with one query per panel has the same bytes as before (4.42). The refId of such a panel is metadata. |
| data source of type `grafana-clickhouse-datasource` with a uid in `--source-map` | same | mapped | `data.query.source` (4.15) | 15.9. |
| data source of type `grafana-clickhouse-datasource` with another uid | same | unsupported | none | Fallback: panel removed. |
| data source `$name`, `${name}` or `[[name]]`, where `name` is a data source variable or an item of `__inputs` that `--source-map` names | same | mapped | `data.query.source` (4.15) | 15.9. The kind check of 15.9 applies to the mapped source. |
| data source `$name`, `${name}` or `[[name]]` that `--source-map` does not name | same | unsupported | none | The reason is "the data source is a variable that the source map does not name". Fallback: panel removed. |
| data source of type `grafana-postgresql-datasource` or `postgres` with a uid in `--source-map` | same | mapped | `data.query.source` (4.15) | 15.9. The dialect of the query is `postgres` (7.19). In v2, the type is the `group` or the `kind` of the query. |
| data source of that type with another uid | same | unsupported | none | Fallback: panel removed. |
| `rawSql` of a PostgreSQL query | `rawSql` | mapped | `data.query.sql` (4.16) | The same rules as the row for `rawSql` of a ClickHouse query. |
| `format` = `table` (a text) of a PostgreSQL query | same | mapped | `data.query.format` = `table` | The PostgreSQL plugin writes the format as a text. |
| `format` = `time_series` (a text) of a PostgreSQL query | same | mapped | `data.query.format` = `time_series` | The time column follows 7.16b. An absent `format` is `changed` to `table`. |
| another `format` text of a PostgreSQL query | same | unsupported | none | Fallback: panel removed. |
| `editorMode` = `builder`, with `sql`, `table`, `dataset` or `columns` | same | unsupported | none | Hune has no query builder for PostgreSQL. Fallback: a non-empty `rawSql` stays as `data.query.sql`, and the builder members are removed. Else the panel is removed. |
| `editorMode` = `code`, `rawQuery`, `refId`, `table`, `dataset`, `columns`, `sql` of a code query | same | dropped | none | Metadata. |
| the macros of 7.22 in code: `$__time`, `$__timeEpoch`, `$__timeFilter`, `$__timeFrom`, `$__timeTo`, `$__timeGroup`, `$__timeGroupAlias`, `$__unixEpochFilter`, `$__unixEpochFrom`, `$__unixEpochTo`, `$__unixEpochNanoFilter`, `$__unixEpochNanoFrom`, `$__unixEpochNanoTo`, `$__unixEpochGroup`, `$__unixEpochGroupAlias` | same | mapped | kept as text | Hune writes the text of Grafana (7.22). A macro with empty parentheses stays as written. |
| `$__timeGroup`, `$__timeGroupAlias`, `$__unixEpochGroup` or `$__unixEpochGroupAlias` with a third argument | same | unsupported | none | Hune does not fill gaps (7.22). The report has one item for each query, with the count of the calls. Fallback: the third argument is removed, and the panel shows the gaps. The importer matches the parentheses of the text in one pass, so a call with no `)` does not make it read the text again. |
| the interval argument of a group macro: `'<n><unit>'` of 7.23, or `$__interval` | same | mapped | kept as text | 7.23. |
| an interval argument that 7.23 refuses (two units, a decimal, a unit outside the list) | same | unsupported | none | Fallback: panel removed. |
| a macro of the dialect `clickhouse` in a PostgreSQL query: `$__timeFilter_ms`, `$__timeInterval`, `$__dateFilter`, `$__fromTime` and the others | same | unsupported | none | Not a macro of the dialect `postgres` (7.22). Fallback: panel removed. |
| `rawSql` | `rawSql` | mapped | `data.query.sql` (4.16) | The text is cut into lines at `\n`. A text with `\r` is `changed`: `\r\n` and `\r` become line ends. |
| `[[name]]` in `rawSql` | same | changed | `${name}` | The old Grafana form. |
| a placeholder that 7.6 accepts, with a format that 7.8 permits in its context | same | mapped | kept as text | The formats of the last group of 7.9 are permitted in a string only. |
| a format of the last group of 7.9 in code or in a quoted name | same | unsupported | none | 7.8 refuses it, because free text there could leave its place. Fallback: panel removed. |
| `${name:date}` or another format that 7.9 does not list, in a string | same | changed | kept as text | The text stays in the literal. Grafana gives `NaN` for `date`. The item is listed. |
| another `${name:format}` form in code | same | unsupported | none | Fallback: panel removed. |
| `$__timeFilter`, `$__timeFilter_ms`, `$__timeInterval`, `$__timeInterval_ms`, `$__fromTime`, `$__fromTime_ms`, `$__toTime`, `$__toTime_ms`, `$__interval_s`, `$__dateFilter`, `$__dateTimeFilter`, `$__conditionalAll` | same | mapped | kept as text | The macros of 7.10. |
| `$__interval_ms`, `$__from`, `$__to` in code | same | mapped | kept as text | The built-in variables of 7.10. |
| a macro name, `$__interval`, `$__range`, `$__range_s` or `$__range_ms` in a string | same | changed | kept as text | Grafana replaces some of these names in a string. Hune keeps the text (7.10). The item is listed. |
| another `$__name` in code | same | unsupported | none | Examples: `$__range`, `$__range_s`, `$__range_ms`, `$__interval`. Fallback: panel removed. |
| `format` = 1 | same | mapped | `data.query.format` = `table` | |
| `format` = 0 | same | mapped | `data.query.format` = `time_series` | |
| `format` = 2 (the logs format of the plugin) | same | mapped or changed | `data.query.format` = `logs` in a `logs` panel, else `table` | On a logs panel the row is `mapped` (4.33f). On another panel the row is `changed`: Grafana draws the format 2 as the format 1 there (measured on Grafana 12.4.2). |
| another `format` | same | unsupported | none | Not in the measured set, for example 3 (traces). Fallback: panel removed. |
| `editorType` = `builder` with `builderOptions` | same | changed | `data.query.sql` | The SQL that the builder made stays. The builder form is removed. |
| `queryType`, `editorType` = `sql`, `meta`, `pluginVersion`, `builderOptions` of a SQL query | same | dropped | none | Metadata. |
| data source of type `haohanyang-mongodb-datasource` with a uid in `--source-map` | same | mapped | `data.query.source` (4.15) | 15.9. The query is a JSON text (7.27). In v2, the type is the `group` or the `kind` of the query. The rows below follow the plugin 0.5.0 as the author remembers it, and the capture of 15.14a decides. |
| data source of that type with another uid | same | unsupported | none | Fallback: panel removed. |
| `queryType` = `table` of a MongoDB query | same | mapped | `data.query.format` = `table` | |
| `queryType` = `timeseries` of a MongoDB query | same | mapped | `data.query.format` = `time_series` | The time column MUST be a Date (7.16d). An absent `queryType` is `changed` to `table`. |
| another `queryType` | same | unsupported | none | Fallback: panel removed. |
| `collection` of a MongoDB query | same | mapped | the member `collection` of the JSON text (7.29) | The name MUST be in the key `collections` of the block (7.33). With `-c`, another name is `unsupported`; fallback: panel removed. Without `-c`, the importer does not run item L8 of 7.36. The save checks it. |
| `database` of a MongoDB query | same | mapped | the member `database` of the JSON text | The same rule, with the key `databases`. An absent `database` gives no member. |
| `queryText` that is a JSON array of stages | same | mapped | the member `aggregate` of the JSON text | The importer builds the object of 7.29 and cuts it into lines. It runs the checks of 7.28 to 7.33 on it. A fault makes the item `unsupported`, and the item names the stage or the operator. Fallback: panel removed. |
| `queryText` in the shell syntax or with JavaScript: `db.getCollection(...)`, `db.<name>.aggregate(...)`, keys with no quote marks, comments, `ISODate(...)`, `ObjectId(...)`, `NumberLong(...)`, `new Date(...)` | same | unsupported | none | Hune reads no JavaScript (7.27). Fallback: panel removed. |
| a stage or an operator that 7.31 does not list, for example `$out`, `$merge`, `$function`, `$where`, `$graphLookup`, `$sample` | same | unsupported | none | 7.32. Fallback: panel removed. |
| a `$lookup` or `$unionWith` with a collection that the key `collections` does not list | same | unsupported | none | 7.33. Fallback: panel removed. |
| the wrapper `{"$date": {"$numberLong": "$__from"}}`, and the same with `$__to` | same | changed | `"$__timeFrom"` and `"$__timeTo"` | The same Date, written as the macro of 7.34. The item is listed. |
| the bare token `$__from` or `$__to` as a value, outside a string | same | changed | `"$__from"` and `"$__to"` | Grafana writes the number in the text. Hune writes the Int64 as a typed value (7.34). The item is listed. |
| `"$__timeFrom"`, `"$__timeTo"`, `"$__from"`, `"$__to"`, `"$__interval_ms"` as a whole string | same | mapped | kept as text | The macros of 7.34. |
| another `$__name` | same | unsupported | none | Examples: `$__interval`, `$__range`. Fallback: panel removed. |
| a placeholder (`$name`, `${name}`, `${name:format}`) in a string value, with a format that 7.34 lists for its place | same | mapped | kept as text | A value never becomes a key, an operator or a field path (7.34, 7.35). |
| `[[name]]` in a string value | same | changed | `${name}` | The old Grafana form. |
| a placeholder in a key, in a name place of 7.34, or with a format that 7.34 does not list | same | unsupported | none | Fallback: panel removed. |
| an Extended JSON form that 7.28 does not list, for example `$binary`, `$regularExpression`, `$timestamp` | same | unsupported | none | Fallback: panel removed. |
| a MongoDB query with more than one stage list, a `find` form that 7.29 refuses, or another member of the plugin that no row names | same | unsupported | none | The rule of 15.5 for an unknown member applies. Fallback: member removed, or panel removed when the query needs it. |
| a query variable of the MongoDB plugin | same | unsupported | none | A query variable of the MongoDB plugin is `unsupported`. Fallback: the dashboard is not imported. |
| data source `-- Dashboard --` with `panelId` | same | mapped | `data.reuse.panel` (4.15) | The source panel must have a ClickHouse query. A reuse of a reuse panel is `unsupported`; fallback: panel removed. |
| `withTransforms` | same | mapped | `data.reuse.with_transforms` | |
| data source `-- Mixed --` of a panel | `datasource` of the panel | mapped | none | The `datasource` of each target gives its `source` by the rows above and below. A target with no `datasource` is `unsupported`; fallback: the target is removed. |
| a target of a panel with several targets whose data source is `-- Dashboard --` or `-- Grafana --`, or of a type or a uid that no row maps | same | unsupported | none | A query of Hune is always a source and an SQL text (4.15a, rule 9). Fallback: the target is removed. When no target is left, the panel is removed. A panel with one target gives the fallback "panel removed". |
| a panel with the data source `-- Dashboard --` and other targets, or a panel with `repeat` and several targets | same | unsupported | none | Fallback: for `-- Dashboard --`, the targets other than the first are removed. For a repeat, the `repeat` is removed and all the queries stay (4.15a, rule 8). A section with `repeat` that holds such a panel loses the `repeat` of the section too. |
| a panel of the type `barchart` or `xychart` with the data source `-- Dashboard --` that names a panel with several queries | same | unsupported | none | 4.15, rule 6. Fallback: panel removed. |
| a panel with the data source `-- Mixed --` and one target | same | mapped | `data.query` | The source of the target gives `source`. |
| data source `-- Mixed --` or `-- Grafana --` of a panel with no usable target | same | unsupported | none | Fallback: panel removed. |

Field members (`fieldConfig` in v1, `E.vizConfig.spec.fieldConfig` in v2):

A Grafana property is a member of `defaults` or an item of `overrides[].properties` (its `id` and `value`). The same row
applies to both places. In `defaults`, the result goes to `fields.defaults`. In an override, it goes to
`fields.overrides[].set`. Both are property sets of 4.18.

| Grafana property (`defaults` member or override `id`) | Class | Hune member of the property set (4.18) | Reason or fallback |
|---|---|---|---|
| `unit` | mapped | `unit` (4.20) | A unit of 4.20 maps. The Grafana custom units `suffix:<text>` and `prefix:<text>` map as they are. Another unit is `unsupported` (19.16); fallback: property removed. |
| `decimals` | mapped | `decimals` | |
| `min`, `max` | mapped | `min`, `max` | |
| `displayName` | mapped | `display_name` | 19.19. |
| `noValue` | mapped | `no_value` | Only in `fields.defaults` (4.18). In an override, it is `unsupported`; fallback: property removed. |
| `thresholds` | mapped | `thresholds` (4.21) | Modes `absolute` and `percentage`. Grafana reads the first step as the base step, whatever its value. The first step of Hune has the value `null`, so an export with the value 0 in the first step gives `null`. |
| `color` | mapped | `color` (4.19) | Grafana `color.mode` `fixed` gives `mode` = `fixed`, `thresholds` gives `thresholds`, and `palette-classic` gives `palette`. The ids `palette-classic-by-name` and `palette-saturated` also give `palette`, each with one `changed` item. The ten ids `continuous-GrYlRd`, `continuous-RdYlGr`, `continuous-BlYlRd`, `continuous-YlRd`, `continuous-BlPu`, `continuous-YlBl`, `continuous-blues`, `continuous-reds`, `continuous-greens` and `continuous-purples` give the modes `continuous_gr_yl_rd`, `continuous_rd_yl_gr`, `continuous_bl_yl_rd`, `continuous_yl_rd`, `continuous_bl_pu`, `continuous_yl_bl`, `continuous_blues`, `continuous_reds`, `continuous_greens` and `continuous_purples`. Another mode (`shades`) is `unsupported`; fallback: property removed. Grafana `color.fixedColor` gives `fixed` with the mode `fixed`, and `null` with the other modes. A colour that is not a token, a hex colour or an `rgb` colour of 4.19 is `changed` to the nearest token. Grafana `color.seriesBy` is dropped (listed when it is not `last`). Hune has one series palette, the 18 colours of ClickHouse Dash (9.16). The mode `palette` draws it, not the classic palette of Grafana. This difference is wanted (19.20a), and the importer does not list it for `palette-classic`. The other palette modes lose their own rule (a colour by series name, a saturated list): the item text names the Grafana mode. The explicit colours, the thresholds, the mappings and the continuous modes do not change. |
| `mappings` | mapped | `mappings` (4.22) | The split and order rule below. |
| `links` | mapped | `links` (4.23) | `targetBlank` gives `target_blank`. A URL that breaks the prefix rule of 4.23, or a token outside 9.18, is `unsupported`; fallback: link removed. The importer removes each U+FEFF of the URL, as for a panel link. |
| `custom.width` | mapped | `width` | Only in a `table` panel (4.18). |
| `custom.align` | mapped | `align` | `auto`, `left`, `center`, `right`. Only in a `table` panel. |
| `custom.cellOptions`, older `custom.displayMode` | mapped | `cell` | Type `auto`, `color-text` and `color-background` give `auto`, `color_text` and `color_background`, with the mode `gradient` or `basic`. For `color-background`, an absent mode gives `gradient`, and another mode is `unsupported`; fallback: `gradient`. In the older `custom.displayMode`, `color-background` gives `color_background` with `gradient`, and `color-background-solid` gives `color_background` with `basic`. Type `gauge` with `mode` `basic`, `gradient` or `lcd` gives `gauge` with this mode. An absent mode gives `gradient`, and another mode is `unsupported`; fallback: `gradient`. In the older `custom.displayMode`, `basic`, `gradient-gauge` and `lcd-gauge` give `gauge` with `basic`, `gradient` and `lcd`. The type `json-view` (older and newer form) gives `json`. Another cell type is `unsupported` (19.15); fallback: `auto`. Only in a `table` panel. |
| `custom.cellOptions.valueDisplayMode` of a `gauge` | dropped | none | Metadata for `color` and `text`: the cell always shows the display text (9.20). The value `hidden`, and any other value, is `unsupported`; fallback: member removed. |
| `custom.footer` (`reducers`) | mapped | `footer` | An empty `reducers` is metadata. Else each id gives a value of `reduce.calc` (the table above). `countAll` gives `count`. Another id is `unsupported`; fallback: the item is removed. An item that repeats a calculation is `changed` (removed). More than 8 different items: each item after the eighth is `unsupported`; fallback: the item is removed, so the first 8 stay. A `footer` object with no `reducers`, or `custom.footer.reducers` as an array, are read in the same way. A value that is not an array is `unsupported`; fallback: property removed. Only in a `table` panel. |
| `custom.hideFrom` | mapped | `hide_from` | The members `legend`, `tooltip` and `viz` map as they are. In `fields.defaults`, a value with all three `false` is metadata. In a `table` panel, the member `viz` gives `hidden` (Grafana 12 hides a column with `hideFrom.viz`), and `legend` and `tooltip` have no equal. A flag that is not a boolean is `unsupported`; fallback: property removed. |
| `custom.axisPlacement` | mapped | `axis_placement` | `auto`, `left`, `right` and `hidden` map, only in a `timeseries`. The value `auto` in `defaults` is metadata. In another panel type, `auto` is metadata, and another value is `unsupported`; fallback: property removed. |
| `custom.axisLabel` | mapped | `axis_label` | A text of 1 to 256 characters, only in a `timeseries` and a `barchart`. An empty text is metadata. A longer text is `changed` (cut to 256 characters). In another type, a text is `unsupported`; fallback: property removed. |
| `custom.axisSoftMin`, `custom.axisSoftMax` | mapped | `soft_min`, `soft_max` | A number maps, only in a `timeseries` and a `barchart`. `null` and an absent value are metadata. When both are in one set and `axisSoftMin` is not below `axisSoftMax`, the pair is `unsupported`; fallback: property removed. In another type, a number is `unsupported`; fallback: property removed. |
| `custom.drawStyle`, `custom.lineWidth`, `custom.fillOpacity`, `custom.showPoints`, `custom.pointSize` in an override of a `timeseries` | mapped | `draw`, `line_width`, `fill_opacity`, `show_points`, `point_size` | The value tables of 4.28 apply, with the ranges of 4.18. A value out of range is `unsupported`; fallback: property removed. In `defaults`, these members map to the options of the panel (the row of 4.28 above). In an override of another type, they are `unsupported`; fallback: property removed. |
| `custom.lineStyle` | mapped | `line_dash`, `line_dash_array` | The `fill` values `solid`, `dash` and `dot` give `solid`, `dash` and `dot`. `{"fill": "solid"}` in `defaults` is metadata, and `dash` has no effect with `solid`. The member `dash` gives `line_dash_array` when it differs from the default of Hune: `[10, 10]` for `dash` and `[0, 10]` for `dot`. Grafana draws `[10, 10]` for a fill with no `dash`, also for `dot`, and dashes for a `dash` with no fill: the importer writes these arrays. An array that is not 1 to 8 integers from 0 to 100, one above 0, is `unsupported`; fallback: the default array of the fill. Only in a `timeseries`. |
| `custom.axisCenteredZero`, `custom.axisWidth`, `custom.axisBorderShow`, `custom.axisColorMode` | mapped | `axis_centered_zero`, `axis_width`, `axis_border`, `axis_color` | Only in a `timeseries` and a `barchart`. A boolean, an integer from 0 to 500, a boolean, and `text` or `series`. In `defaults`, `false` and `text` are metadata, and `null` is metadata. An override keeps an explicit value. Another value is `unsupported`; fallback: property removed. |
| `custom.transform` | mapped | `transform` | `negative-Y` gives `negative_y`. `constant` is `unsupported`; fallback: property removed. An absent value is metadata. Only in a `timeseries`. |
| `custom.filterable` | mapped | `filterable` | Only in a `table` panel. |
| `custom.hidden` | mapped | `hidden` | Only in a `table` panel (19.19). |
| `custom.inspect` | mapped | `inspect` | Only in a `table` panel. In `defaults`, `false` is metadata, because it is the Grafana default. In an override, `true` and `false` map. A value that is not a boolean is `unsupported`; fallback: dashboard not imported. In another panel type, the general rule of a `custom` member applies. |
| `custom.width`, `custom.align`, cell type, `custom.filterable`, `custom.hidden` in a panel that is not a `table` | unsupported | none | 4.18, rule 1. Fallback: property removed. |
| `custom.axisGridShow` = `false` in a `timeseries`, `histogram` or `xychart` | unsupported | none | The value hides the grid lines of the axis. Hune draws them for each axis that shows (9.19). Fallback: property removed. In an override, the same rule applies. A `barchart` maps the member to `axis_grid` (4.18). |
| `custom.axisGridShow` in a `barchart` | mapped | `axis_grid` | `true` and `false` map, in `defaults` and in an override. `null` is Auto: metadata. Another value is `unsupported`; fallback: property removed. |
| `custom.fillOpacity`, `custom.lineWidth` in a `barchart` | mapped | `options.fill_opacity`, `options.line_width` | The ranges of 4.30 apply: 0 to 100 and 0 to 10. A value out of range is `unsupported`; fallback: the default 80 or 1. `null` is the default. In an override the member is `unsupported`; fallback: property removed. |
| another member of `defaults.custom` that is not a source of 4.28 or 4.32 | unsupported | none | Fallback: property removed. |
| another property id | unsupported | none | Fallback: property removed. |

| Grafana override matcher | Class | Hune | Reason or fallback |
|---|---|---|---|
| `matcher.id` = `byName`, `matcher.options` | mapped | `fields.overrides[].match.by` = `name`, `match.value` (4.17) | |
| `matcher.id` = `byRegexp`, `matcher.options` | mapped | `fields.overrides[].match.by` = `regex`, `match.value` | The Grafana text as it is. Grafana reads it with `stringToJsRegex`, the Grafana form of 4.24; M0 and the spike of 18.3a confirm it on the pinned Grafana (19.29). A pattern that 4.24 refuses is `unsupported`; fallback: override removed. |
| `matcher.id` = `byNames`, `matcher.options` | mapped | `fields.overrides[].match.by` = `names`, `match.names`, `match.mode` (4.17) | `options.names` gives `names`. Two equal names give one name. The list MUST have 1 to 64 different names, each of 1 to 1024 bytes with no control character, else `unsupported`. `options.mode` gives `mode`: `include` or `exclude`; an absent mode gives `include`. Another mode is `unsupported`. `options.prefix` and `options.readOnly` are metadata: Grafana uses them for its own interface. Fallback for each `unsupported` case: override removed. |
| the member `matcher.scope` | mapped | none | The value `series` is mapped with no member: a frame of Hune has no nested frame, so the behaviour is the same. Grafana 13 writes it on a column width that the author sets. Another value (`nested`) is `unsupported`; fallback: override removed. |
| the override member `__systemRef` | dropped | none | Metadata. Grafana writes the value `hideSeriesFrom` when a viewer clicks a series in the legend. |
| matcher `byFrameRefID` with a text `options` | mapped | `match` with `by` = `query` and `value` (4.17) | The text MUST follow the grammar of 4.3. Else the item is `unsupported`; fallback: override removed. A refId that the importer changed (the row `Q.refId`) is changed in the matcher too, and the matcher of a panel with one query names the ref `A`. |
| another matcher (`byType`, `byValue`) | unsupported | none | Fallback: override removed. |

The split and order rule of the value mappings. A Grafana property `mappings` is an array of items. An item of `type`
`value` holds many values in `options`: one key for each value, each with a `text`, a `color` and an `index`. Items of
`type` `range`, `regex` and `special` hold one `options.result` with `text`, `color` and `index`.

1. Each key of a `value` item gives one mapping of kind `value` (4.22), with `value` equal to the key.
2. A `range` item gives one mapping of kind `range` (`options.from`, `options.to`). A `regex` item gives one mapping of
   kind `regex` (`options.pattern`). A `special` item gives one mapping of kind `special`.
3. The Grafana special matches `null`, `nan`, `null+nan`, `true`, `false` and `empty` give `null`, `nan`, `null_nan`,
   `true`, `false` and `empty`.
4. The importer puts the mappings in the order of `index`, from the lowest. Two equal `index` values keep the order of the
   input. This is the order in which Grafana tries them.
5. An empty `text` gives `null`. An absent `color` gives `null`. A mapping whose `text` and `color` are both `null` is
   dropped (listed).
6. More than `MAX_MAPPINGS` (4.35) mappings is `unsupported`; fallback: the first `MAX_MAPPINGS` mappings stay.

Transformations (each item of `transformations`). A v1 item is `{id, options, disabled, filter}`. A v2 item is
`{kind, spec}`: `kind` is the Grafana id, and `spec` holds `id`, `options`, `disabled` and `filter`. The table names the
id and the members without the prefix `spec.`. More than 16 items give `unsupported`; fallback: the first 16 items stay, as for `MAX_TRANSFORMS` (4.35).

| Grafana `id` | Class | Hune | Reason or fallback |
|---|---|---|---|
| `filterByValue` | mapped | `transforms` item of kind `filter_by_value` (4.25) | `options.type` (`include`, `exclude`) gives `mode`. `options.match` (`any`, `all`) gives `match`. Each filter gives `field` (from `fieldName`) and `matcher`. The Grafana matcher ids `equal`, `notEqual`, `greater`, `greaterOrEqual`, `lower`, `lowerOrEqual`, `between`, `regex`, `isNull` and `isNotNull` give the kinds `equal`, `not_equal`, `greater`, `greater_or_equal`, `lower`, `lower_or_equal`, `range`, `regex`, `is_null` and `is_not_null`. Another matcher (for example `substring`, `notSubstring`) is `unsupported`; fallback: transformation removed. M0 counts them (18.3). |
| `groupingToMatrix` | mapped | `transforms` item of kind `grouping_to_matrix` (4.25) | `columnField`, `rowField`, `valueField` and `emptyValue` give `column_field`, `row_field`, `value_field` and `empty_value`. |
| `prepareTimeSeries` | changed | `data.query.format` = `time_series` (4.25) | The multi-frame form is the long-to-wide result. Its format is `multi`, or `many` in older dashboards: Grafana 12.4.2 reads the two names the same. The formats `wide` and `long` are `unsupported`; fallback: transformation removed. |
| `labelsToFields` | changed | `data.query.format` = `table` (4.25) | After long-to-wide, the result is the table form. |
| `organize` | mapped | `transforms` item of kind `organize` (4.25a) | `excludeByName` (the names with `true`) gives `hide`. `indexByName` gives `order`, the names sorted by index. A tie of two indexes is `changed`: the names keep the order of the keys of the file. `renameByName` gives `rename` (an empty new name is `dropped`). A non-empty `includeByName` is `unsupported`; fallback: the member is dropped and listed, and the other members stay. More than 64 names in a list is `unsupported`; fallback: transformation removed. |
| `calculateField` | mapped | kind `calculate_field` | `mode` `binary`, `reduceRow`, `unary` and `index` are mapped (`reduceRow` gives `reduce_row`). `alias` (an empty text gives `null`) and `replaceFields` are mapped. For `binary`, an operand that is a number text, `{fixed: "2"}` or a `byName` matcher gives the operand `number` or `field`. Another operator than `+`, `-`, `*` and `/`, an operand with `$`, an empty operand, a `fixed` text that is not a number, a matcher `byType`, and two number operands are `unsupported`; fallback: transformation removed. The `reduce.reducer` ids map to the calculations of 4.26 (`sum` gives `total`, `lastNotNull` gives `last_not_null`, `stdDev` gives `std_dev`, `diffperc` gives `diff_percent`, `distinctCount` gives `distinct_count`, `changeCount` gives `change_count`). Another reducer (`allValues`, `p50`) is `unsupported`; fallback: transformation removed. `ignoreNulls` and `nullValueMode` with a value other than `null as zero` are `dropped` (no effect, measured). `nullValueMode` `null as zero` is `unsupported`; fallback: transformation removed. The modes `cumulativeFunctions` and `windowFunctions`, and an absent `mode`, are `unsupported`; fallback: transformation removed. The member `timeSeries` is `dropped`. |
| `sortBy` | mapped | kind `sort_by` | `sort` gives `sort`: `field` and `desc`. More than 8 keys is `unsupported`; fallback: the first 8 keys. `reverse` is `dropped` (no effect, measured). A key that equals an earlier key is `dropped`. An empty `sort` gives no transform; the item is `dropped`. |
| `limit` | mapped | kind `limit` | `limitField` as a number or a number text: the whole part of the absolute value gives `count`, and a negative value gives `end` = `last`. An absent, empty or non-number value is `dropped` (no effect, measured). A text with `$` is `unsupported`; fallback: transformation removed. A value above 1,000,000 is `unsupported`; fallback: `count` = 1,000,000. |
| `merge` | mapped | kind `merge` | No option. |
| `reduce` | mapped | kind `reduce` | `mode` (`seriesToRows` or absent gives `series_to_rows`, `reduceFields` gives `reduce_fields`) and `reducers` (absent gives `["max"]`) are mapped, with the calculation ids of the row `calculateField`. An empty list of reducers is `dropped` (no effect, measured). A reducer that has no calculation is `unsupported`; fallback: the item of the list is removed, and with no reducer left the transformation is removed. A non-empty `fields` is `unsupported`; fallback: transformation removed (Grafana 12.4.2 gives a panel error, measured). `includeTimeField` and `labelsToFields` are `dropped`. |
| `partitionByValues` | changed | kind `partition_by_values` | `fields` (1 to 4 names; another count is `unsupported`; fallback: transformation removed) and `keepFields` are mapped. Hune splits each frame and Grafana splits the first frame only (19.20j), so the transformation has one item with the class `changed`. `naming.asLabels` equal to `false` is `changed`: Hune always makes labels. `naming.append` and the separators are `dropped`. |
| `joinByField`, `seriesToColumns` | mapped | kind `join_by_field` | `byField` (an empty or absent value gives `null`) and `mode` (`outer` or absent, `inner`) are mapped. The mode `outerTabular` is `unsupported`; fallback: transformation removed. |
| `filterFieldsByName` | mapped | kind `filter_fields_by_name` | `include` and `exclude` give `names` and `pattern`. A pattern that 4.24 refuses is `unsupported`; fallback: transformation removed. The members `byVariable` and `variable` are `unsupported`; fallback: transformation removed. |
| `renameByRegex` | mapped | kind `rename_by_regex` | `regex` and `renamePattern` give `regex` and `rename_pattern`. A regex that 4.24 refuses is `unsupported`; fallback: transformation removed. An absent `regex` is `dropped` with the item. |
| `convertFieldType` | changed | kind `convert_field_type` | Each conversion gives `field` and `to` (`string` gives `text`). The destination types `enum` and `other`, and a conversion with `dateFormat`, `joinWith` or `enumConfig`, are `unsupported`; fallback: the conversion is removed, and with none left the transformation is removed. A second conversion of the same field, and a conversion after the 16th, are `unsupported`; fallback: the conversion is removed. The rules of the values differ (19.20j), so the class is `changed`. |
| another `id` | unsupported | none | Fallback: transformation removed. |
| `disabled` = `true` | dropped | none | Listed. The transformation is removed. |
| `filter` (the refId filter of an item) | unsupported | none | Fallback: member removed. |

Variables (each item of `spec.variables` or `templating.list`):

| v2 kind | v1 `type` | Class | Hune | Reason or fallback |
|---|---|---|---|---|
| `QueryVariable` | `query` | mapped | `kind` = `query` (4.8) | |
| `CustomVariable` | `custom` | mapped | `kind` = `custom` (4.9) | The text `a,b` or `text : value,...` gives `options` with `text` and `value`. `\,` is a comma in a value. |
| `TextVariable` | `textbox` | mapped | `kind` = `text` (4.10) | The Grafana `query` gives `default`. `value_type` is `text`, except in the next row. |
| `TextVariable` used in code (outside a literal) | `textbox` used in code | changed | `value_type` = `number` | Only when the default matches the number rule of 4.10: the value must then be a decimal number. Another default is `unsupported` (19.11); fallback: the dashboard is not imported. |
| `DatasourceVariable` that `--source-map` names | `datasource` that `--source-map` names | changed | none | The variable leaves the document. Each reference to it in a member `datasource` gets the mapped source (15.9). The item gives `regex`, `multi` and `includeAll`. |
| `DatasourceVariable` that `--source-map` does not name | `datasource` that `--source-map` does not name | unsupported | none | Hune has no data source variable (open question D10). Fallback: the dashboard is not imported. |
| `AdhocVariable` | `adhoc` | changed | `kind` = `adhoc` (4.10c) | The source comes from the source map and MUST be a ClickHouse source; else `unsupported`, fallback: the variable is removed. The keys are the static keys of `defaultKeys` (`value`, else `text`), then the keys of `filters` and of `baseFilters`, each one once. A key that is not a plain column name, a key that the list `adhoc_columns` of the source (with `-c`) does not hold, and a key after the 64th are `unsupported`; fallback: the key is removed. With no key left, the keys are the first 64 items of the list (`changed`). With no list, the variable is `unsupported`; fallback: the variable is removed. A stored filter and a base filter do not apply (`changed`). `allowCustomValue` is metadata. The variable has one more `changed` item: the filters apply only where a query has `AND $__adhocFilters`. Each panel query and extra query on the source of the variable gets a `changed` item that asks for `AND $__adhocFilters` in its WHERE clause. The importer never changes the SQL. |
| `GroupByVariable` | `groupby` | unsupported | none | Not in the measured set. Fallback: the dashboard is not imported. |
| `IntervalVariable` | `interval` | mapped | `kind` = `interval` (4.10a) | See the rows below. |
| `ConstantVariable` | `constant` | mapped | `kind` = `constant` (4.10b) | See the rows below. |
| `query` of an interval variable | `query` | mapped | `intervals` | The importer splits the text at each run of characters that are not a letter, a digit or `_`, as Grafana does (measured). It removes a repeated item and keeps the order. An item that breaks 4.10a (`abc`, `5x`, `0m`, `1M`) is `unsupported`; fallback: the item is removed and listed. With no item left, the variable is `unsupported`; fallback: the dashboard is not imported. An empty text gives the default list of Grafana (`1m`, `10m`, `30m`, `1h`, `6h`, `12h`, `1d`, `7d`, `14d`, `30d`), `changed`. More than 50 items give `unsupported`; fallback: the first 50 items. A value that is not a text is `unsupported`; fallback: the dashboard is not imported. |
| `auto`, `auto_count`, `auto_min` of an interval variable | `auto`, `auto_count`, `auto_min` | mapped | same | An absent `auto` gives `false`. An absent `auto_count` gives 30 and an absent `auto_min` gives `10s`, the defaults of Grafana, with no item. An `auto_count` from 1 to 1,000 is kept, else `unsupported` with the fallback 30. An `auto_min` that is a table text of 7.4a is kept. Another valid interval text gives the largest table text that is not above it (`changed`; `90s` gives `1m`, as Grafana does). A text of more than 365 days gives `1y` (`changed`). An invalid text gives `10s` (`changed`). |
| `current` of an interval variable | `current` | mapped | `default` | `$__auto` and `$__auto_interval_<name>` give `$__auto` when `auto` is `true`, else `null` (`changed`). An item gives that item. Another value gives `null` (`changed`). |
| `query` of a constant variable | `query` | mapped | `value` | A value of more than 1,024 bytes, or with a control character, is `unsupported`; fallback: the dashboard is not imported. An absent value gives an empty text. |
| `hide` of a constant variable | `hide` | changed | `hide` = `variable` | Grafana shows a constant with `hide` 0, and Hune never shows it. The row is listed when `hide` is 0 or 1. |
| `description`, `skipUrlSync`, `options` and `current` of a constant variable | same | dropped | none | Metadata. Listed when not empty. |
| `name` | `name` | mapped | `name` (4.3) | A name outside the rule of 4.3 (for example one that starts with `__`) is `unsupported`; fallback: the dashboard is not imported. |
| `label` | `label` | mapped | `label` (4.7) | Empty gives `null`. |
| `description` | `description` | dropped | none | Format 1 has no description of a variable (4.7). Listed when not empty. |
| `hide` | `hide` | mapped | `hide` (4.7) | `dontHide` or 0 gives `none`. `hideLabel` or 1 gives `label`. `hideVariable` or 2 gives `variable`. |
| `query` of a query variable | `query` or `definition` | mapped | `sql` (4.16) and `source` (4.8) | Same rules as a panel query. |
| `multi`, `includeAll`, `allValue` | same | mapped | `multi`, `include_all`, `all_value` (4.8, 4.9) | An empty `allValue`, or any `allValue` with `includeAll` = `false`, gives `null`. |
| `allowCustomValue` of a `QueryVariable` (v2beta1: `spec.variables[].spec.allowCustomValue`; v1: `templating.list[].allowCustomValue`) | same | mapped | `allow_custom_value` (4.8) | `true` and an absent member both give `true`: Grafana 12.4.2 reads an absent member as `true`. `false` gives no member (4.38). A value that is not a boolean is `unsupported` (19.18); fallback: no member. |
| `allowCustomValue` of a `CustomVariable` (same paths) | same | mapped | `allow_custom_value` (4.9) | `true` gives `true`. `false` gives no member (4.38). An absent member gives no member too, with the class `changed` and one report line: Grafana reads it as `true`, and Hune keeps the variable closed, so the server keeps its option check. The member stays absent, with the class `changed` and one report line, when a free value would be refused: a placeholder of the variable in a quoted name, a placeholder in code with the format none, `raw` or `csv`, or (MongoDB) an option value that starts with `$` and a query that uses the variable. A query uses the variable when a placeholder of 7.6 names it. Each form counts: `$name`, `${name}`, `${name:format}` and `[[name]]`, which the importer rewrites to `${name}`. The importer reads the placeholders with the scan of 7.6, so `$names` names another variable. A value that is not a boolean is `unsupported` (19.18); fallback: no member. |
| `allowCustomValue` of a `TextVariable` | same | dropped | none | Metadata. A text variable is always free (4.10). Not listed. The v2beta1 fixture `monitor-v2b` holds it with the value `true`. |
| `refresh` = `onDashboardLoad` or 1 | same | mapped | `refresh` = `load` (4.8) | |
| `refresh` = `onTimeRangeChanged` or 2 | same | mapped | `refresh` = `time_change` | |
| `refresh` = `never` or 0 | same | changed | `refresh` = `load` | Hune computes the options at load. Grafana keeps stored options. |
| `sort` | `sort` | mapped | `sort` (4.8) | v1 number, then v2 name: 0 or `disabled` gives `none`; 1 or `alphabeticalAsc` gives `alpha_asc`; 2 or `alphabeticalDesc` gives `alpha_desc`; 3 or `numericalAsc` gives `num_asc`; 4 or `numericalDesc` gives `num_desc`; 5 or `alphabeticalCaseInsensitiveAsc` gives `alpha_ci_asc`; 6 or `alphabeticalCaseInsensitiveDesc` gives `alpha_ci_desc`; 7 or `naturalAsc` gives `natural_asc`; 8 or `naturalDesc` gives `natural_desc`. A value that 4.8 does not hold is `unsupported` (19.18); fallback: `none`. |
| `regex` | `regex` | mapped | `regex` (4.8, Grafana form of 4.24) | A pattern or a flag that 4.24 refuses is `unsupported` (19.18); fallback: `null`. |
| `current` | `current` | mapped | `default` (4.11) | All gives `$__all`. A value of a `custom` variable that is not an option value is `unsupported` when the mapping makes the variable closed; fallback: an empty array. When the variable is open, the value is a default item (4.11, rule 5). |
| `options` of a query variable | same | dropped | none | Metadata. Hune computes the options. |
| `skipUrlSync` | same | dropped | none | Listed when `true`. |

Folders and objects outside the dashboards:

| Object | Class | Hune | Reason or fallback |
|---|---|---|---|
| folder with a uid that fits the folder id rule of 4.3 | mapped | folder id and the `title` of its folder file (4.41) | 15.7. |
| root of Grafana | mapped | no folder (4.3) | The dashboards of the root have no folder. |
| folder with a parent (`parentUid`) | unsupported | none | Folders have one level. Fallback: its dashboards are not imported. |
| version history (138 versions measured) | not imported | none | Not in the export. The report header says so. |
| public dashboard (1 measured) | not imported | none | Public dashboards are removed in v1 (section 18). |
| short links (7 measured) | not imported | none | The nginx map of 15.17 redirects them. |
| permissions, stars, playlists, snapshots, alert rules | not imported | none | Not in the export, and out of scope. |

### 15.7 The folder map

The file of `--folders` is JSON with a closed schema:

```json
{
  "folders": [
    { "grafana_uid": "fe1x0abc2kq9", "id": "transit", "title": "Transit desk" },
    { "grafana_uid": "ae7r4kd0pq1z", "id": "nodes" }
  ]
}
```

| Member | Type | Rule |
|---|---|---|
| `grafana_uid` | text | Necessary. A folder uid of `folders.json`. Unique in the file. |
| `id` | text | Necessary. The folder id rule of 4.3. |
| `title` | text | Optional. 1 to 256 characters (4.41). When absent, the importer takes the Grafana title. |

A folder without an entry gets its Grafana uid as its id when the uid fits the rule. Otherwise its dashboards are
`unsupported` with the fallback "the dashboard is not imported". Two Grafana folders that map to one id stop the command
with exit code 2. A file that breaks the schema stops the command with exit code 2.

### 15.8 The uid rule

The importer keeps a Grafana uid that fits the uid rule of 4.3. Otherwise it makes a new uid with these steps:

1. Change ASCII capital letters to small letters.
2. Change each character outside `[a-z0-9-]` to `-`.
3. Change each run of `-` to one `-`. Remove `-` at the start and at the end. Keep the first 40 characters.
4. Check the result. When it is empty, breaks the rule or equals the uid of another imported dashboard, replace it.
   The replacement is `g-` and the first 16 hex characters of the SHA-256 of the old uid.

Example: `Nodes_Prod` becomes `nodes-prod`. The new uid is an item of class `changed` with the old and the new uid.
The nginx map of 15.17 redirects the old uid to the new uid. The steps give the same result at each run.

### 15.9 The source map

Each `--source-map` gives a Grafana data source uid and a source id of section 8. Example: `--source-map clickhouse=prod`
maps the Grafana data source with the uid `clickhouse` to the source `prod`.

The importer reads the data source of a query here: `datasource.uid` in v1, `PanelQuery.spec.datasource.uid` in v2alpha1,
and `DataQuery.datasource.name` in v2beta1. An old v1 reference that is a text (a data source name) is compared with the
keys of `--source-map` too.

A key can also name a data source variable or an item of `__inputs` with `type` = `datasource`, for example
`--source-map datasource=prod`. Hune has no data source variable (open question D10). Thus the importer replaces each
reference `$name`, `${name}` and `[[name]]` in a member `datasource` with the source id. This applies to the panels, the
targets, the query variables and the annotations.

The variable and the item of `__inputs` leave the document, each with one item of class `changed`. The text of the item of
a variable gives its `regex`, `multi` and `includeAll`. A reference with no type takes the plugin of the variable (`query`
or `pluginId`) or of the item (`pluginId`) as its type.

A reference that the map does not name is `unsupported`; fallback: panel removed. Its reason is "the data source is a
variable that the source map does not name". A data source variable that the map does not name is `unsupported`;
fallback: the dashboard is not imported. A SQL text that names such a variable or item (`$datasource` in the SQL) is
`unsupported`, because the importer cannot give it a value. Fallback: panel removed, item removed for an annotation, or
the dashboard is not imported for a query variable.

The source id must fit the label rule of a source block (13.12, 13.12a, 13.12b). The kind of the block follows the type of the
Grafana data source. The type `grafana-clickhouse-datasource` gives a `clickhouse` block. The types
`grafana-postgresql-datasource` and `postgres` give a `postgres` block.

The type `haohanyang-mongodb-datasource` gives a `mongodb` block. With `-c`, the id must name a block of that
kind, else exit code 2. Without `-c`, the importer takes the dialect of 7.19 from the type.

The importer checks the kind of the block after the conversion. This check runs for each source id that a panel or a variable uses with a typed data source. A block of another kind stops the command with exit code 2, before any write. A reference with no type, such as an old data source name, takes the kind of the block. Without `-c`, such a reference has the dialect `clickhouse`. With `-c`, each source id of the map MUST name a block of one of the three kinds, else exit code 2.

### 15.10 The acceptance file

The file of `--accept` lists the losses that the operator accepts. A person with the right to accept the losses signs the
file. The file stays with the confidential exports and never enters the repository (15.18).

```text
# Losses accepted for the import of 2026-11-20. Signed: the operator.
abc123:/spec/elements/panel-7/spec/vizConfig/spec/options/legend/sortBy   # legend sort: not used
k9x2mm:/panels/3/targets/1                                                 # second query: a copy of the first
```

The rules:

- One item id on each line. A `#` after a space starts a comment. Blank lines and comment lines are permitted.
- An item id is `<grafana uid>:<JSON pointer>`. The JSON pointer (RFC 6901) names the member in the input file of that
  dashboard. An item of `folders.json` uses `folder:<folder uid>:<JSON pointer>`.
- A duplicate item id stops the command with exit code 2.
- An item id that matches no item of the run is stale. A stale line gives exit code 1, the same as an item without
  acceptance. So the file always describes the current export.

### 15.11 The report

`--report <path>` writes `<path>.json` for programs and `<path>.md` for the operator. The two files hold the same facts.
Dashboards are in the order of their Grafana uid, and items in the order of their JSON pointer.

```json
{
  "report_version": 1,
  "hune_version": "0.2.0",
  "created_at": "2026-11-20T09:12:44Z",
  "mode": "dry_run",
  "input": {
    "dir": "/srv/grafana-export-20261120T090000Z",
    "grafana_version": "12.2.1",
    "clickhouse_plugin_version": "4.5.1",
    "files": [ { "name": "dashboards/abc123.json", "sha256": "9f2c...", "form": "v2beta1" } ],
    "ignored_files": []
  },
  "options": {
    "source_map": { "clickhouse": "prod" },
    "link_prefix": { "old": "/grafana/", "new": "/dashboards/" },
    "folders_file": "folders.map.json",
    "accept_file": "accept.txt",
    "write": false,
    "replace": false
  },
  "totals": {
    "dashboards": 10, "panels": 161,
    "mapped": 4210, "changed": 12, "dropped": 4, "unsupported": 0, "metadata": 380,
    "accepted": 16, "unaccepted": 0, "stale_accept_lines": 0
  },
  "not_imported": { "history": "not in the export", "public_dashboards": 1, "short_links": 7 },
  "folders": [ { "grafana_uid": "fe1x0abc2kq9", "id": "transit", "title": "Transit desk", "result": "would_create" } ],
  "dashboards": [
    {
      "grafana_uid": "abc123", "uid": "abc123", "folder": "transit", "title": "Nodes",
      "panels": 45, "metadata": 41, "result": "would_write", "etag": "4be1...",
      "items": [
        {
          "id": "abc123:/spec/elements/panel-7/spec/vizConfig/spec/options/legend/sortBy",
          "class": "unsupported", "panel_id": 7, "panel_title": "Score by day",
          "member": "options.legend.sortBy",
          "reason": "not in the option set of the panel type",
          "fallback": "member removed",
          "value": "\"Max\"", "truncated": false, "accepted": true
        }
      ]
    }
  ],
  "redirects": [
    { "from": "/grafana/d/abc123", "to": "/dashboards/d/abc123" },
    { "from": "/grafana/goto/xYz12AB", "to": "/dashboards/d/abc123/nodes?from=now-7d&to=now" }
  ],
  "exit_code": 0
}
```

| Member | Rule |
|---|---|
| `mode` | `dry_run` or `write`. |
| `etag` | The etag of section 4 of the document that the importer makes. |
| `value` | The original value as compact JSON, cut to 200 characters at a character boundary. `truncated` tells if it was cut. An `unsupported` item of a MongoDB query gives the JSON of its member, cut by the same rule. |
| `panel_id`, `panel_title` | `null` for a member outside a panel. |
| `folders[].result` | `exists`, `would_create`, `created`, `would_rename`, `renamed`, `refused_title` (15.13). |
| `dashboards[].result` | One value of the table below. |

| Result | Meaning |
|---|---|
| `would_write` | Dry run: the document is new, or differs from the store in its bytes or its folder. |
| `would_refuse` | Dry run with `-c`: the uid exists with other bytes or in another folder, and `--replace` is absent. |
| `unchanged` | The store holds the same bytes in the same folder. No write, no history version. |
| `written` | The importer created the document. |
| `replaced` | The importer saved a new version over other bytes (`--replace`). |
| `moved` | The importer moved the document to its new folder; the bytes were equal (`--replace`). |
| `moved_replaced` | The importer moved the document, then saved a new version over other bytes (`--replace`). |
| `refused_exists` | The uid exists with other bytes or in another folder and `--replace` is absent, or the uid is deleted. |
| `blocked` | An item needs acceptance and has none. |
| `not_imported` | An accepted item has the fallback "the dashboard is not imported". |
| `store_error` | The store refused or failed the write. |

The Markdown report has these parts, in this order:

1. The input directory, the Grafana version and the plugin version.
2. The totals, as a table.
3. The objects that are not imported.
4. One table of items for each dashboard: id, class, panel, member, reason, fallback, accepted.
5. The redirects, as an nginx block.
6. The exit code and its reason.

### 15.12 Exit codes

| Code | Meaning | Documents written |
|---|---|---|
| 0 | Each item that needs acceptance is accepted, no line is stale, and each write succeeded. | yes with `--write` |
| 1 | An item needs acceptance and has none, or an acceptance line is stale. | none |
| 2 | Usage or input error: an option, a file, a schema version, a map file, the configuration. | none |
| 3 | Write refused or failed: a uid with other bytes or in another folder, or a folder with another title, without `--replace`; a deleted uid; a held volume lock; a store error. | 15.13 |

### 15.13 The write and the idempotent final import

With `--write`, the importer does these steps in this order:

1. Read and convert every input file. Check the acceptance file. On exit code 1 or 2, stop here.
2. Open the store of `-c`. For the volume, take the writer lock of 5.16. A held lock gives exit code 3.
3. Read the folder, its title, and the etag of each target uid. Without `--replace`, stop with exit code 3 for each of
   these cases:
   - a uid has other bytes;
   - a uid is in another folder;
   - a folder has another title.

   Also stop with exit code 3 when a uid is deleted, with or without `--replace`. Nothing is written yet.
4. Create the folders that do not exist. With `--replace`, call `Store::rename_folder` for each folder whose title
   differs (folder result `renamed`).
5. Write each dashboard in the order of its Grafana uid, by the rules below.

| State in the store | Action | Result |
|---|---|---|
| uid absent, with no version | `Store::create` with the kind `import` | `written` |
| same bytes, same folder | none | `unchanged` |
| other bytes, same folder, `--replace` | `Store::save` with the kind `import` and `If-Match` equal to the stored etag | `replaced` |
| same bytes, other folder, `--replace` | `Store::move_to` to the new folder, with `If-Match` equal to the stored etag | `moved` |
| other bytes, other folder, `--replace` | `Store::move_to`, then `Store::save` with the kind `import` | `moved_replaced` |
| uid deleted (versions only) | none; step 3 stops with exit code 3 | `refused_exists` |

Only `hune restore` (5.40) restores a deleted uid, because `create` gives `Exists` for a uid with versions (5.4).
A current file that is not valid (4.39, rule 3) has the etag of its raw bytes. A save over it with that `If-Match` first
records a version of kind `external` without bytes (5.14), then the version of kind `import`.

Each write records a history version of the kind `import` (5.2), or of the kind `move` for a move. Its `saved_by` is
`import-grafana`, and its message is `import-grafana <file> <sha12>`. `<sha12>` is the first 12 hex characters of the
SHA-256 of the input file.

The `save_id` of each write is a UUID version 8 (RFC 9562). Its 16 bytes are the first 16 bytes of a SHA-256, with the
version and variant bits of RFC 9562 set. The input of the SHA-256 names one transition:

| Write | Input of the SHA-256 |
|---|---|
| create | `import-grafana:<uid>:none:<new etag>` |
| save | `import-grafana:<uid>:<If-Match>:<new etag>` |
| move | `import-grafana:<uid>:<If-Match>:move:<new folder>` |

So each transition has its own `save_id`. Example: a dashboard changes from A to B, then to A again. The second A gets a
new `save_id`, because its `If-Match` is the etag of B. A repeated write after a crash or a lost answer sends the same
`save_id`. The store then answers `replayed` (5.5), and the result is the result of the first write: `written`,
`replaced`, `moved` or `moved_replaced`.

A store error in step 5 stops the command with exit code 3. The dashboards before it stay written. A new run with the same
input completes the work, because a written dashboard gives `unchanged`.

The same input and options give the same documents, byte for byte. The documents hold no time and no random value. A
second run of the same command writes nothing and exits with code 0. The final import of the cutover (15.15) is the same
command on the final export, with `--write --replace`.

A dry run with `-c` opens the store read-only and takes no lock. It reports `would_write`, `would_refuse` and `unchanged`
for the dashboards, and `would_create`, `would_rename` and `exists` for the folders.

### 15.14 Verification on real dashboards

The verification proves that Hune shows the same data as Grafana. It runs on a trusted machine or on a private runner,
never in a public CI. It reads the real Grafana and the real ClickHouse. So each run needs the authorisation of the person who
is responsible for them.

It runs outside the peak hours. It sends only read queries through Grafana and through Hune.
Its inputs and outputs stay outside the repository (15.18).

Each tool has one access method:

| Tool | Access | Credential |
|---|---|---|
| Export of 15.3 | SSH to the host of the Grafana instance, then `docker exec` in the Grafana container | The SSH key of the operator |
| `import-grafana` and the test `parity_private` | None: they read local files only | None |
| `capture.mjs` and `compare.mjs` | HTTPS through the reverse proxy of the instance (nginx in this runbook): `/grafana/` for Grafana and `/dashboards/` for Hune (15.15) | A login session of a person with access to both prefixes. This person logs in one time in the browser of the tool. The tool keeps the session state in the evidence directory (mode 0600), never the password, and removes it at the end of the run. |

The tools need no change on the host of the instance: no SSH tunnel to the Grafana port and no new nginx location.

The verification has six steps. Each step has a pass criterion.

| Step | Action | Pass criterion |
|---|---|---|
| V1 | Run the importer in a dry run on a fresh export, with the signed acceptance file. | Exit code 0; `unaccepted` = 0; no dashboard is `blocked` or `not_imported`. |
| V2 | Run `--write` two times into an empty temporary volume. | The second run gives `unchanged` for each dashboard. The two JSON reports are equal except `created_at`. The volume tree is equal after each run. |
| V3 | Compare the SQL (15.14a). | Each text is equal, or its difference has a cause that the operator signs. |
| V4 | Run the parity harness (15.14b). | Each panel is equal, or its difference has a cause that the operator signs. |
| V5 | Visual check (15.14c). | The operator signs the review. |
| V6 | Record the counts and the signatures in the evidence directory. | The record exists and names the Grafana and plugin versions. |

#### 15.14a SQL parity

The capture tool is the Playwright script `tools/parity/capture.mjs`. It runs in the image `tools/parity/Dockerfile`, which
starts from the pinned Playwright image. It opens each dashboard in Grafana at a fixed URL: absolute `from` and `to`,
and the `var-` values of one set. The window is 1920 by 1080 pixels. The tool
waits until no request is open.

The tool records each `POST /grafana/api/ds/query` request and its answer, for panels and for variables. It reads the
dashboard uid and the panel id from the request headers `X-Dashboard-Uid` and `X-Panel-Id`.

A request without `X-Panel-Id` is a variable query. The compare program pairs it by three keys: the dashboard uid, the
`refId` of the query, and its text. For the text key, the program expands the SQL of each query variable of that
dashboard with the values of the set. It normalises the result by the rule below. The pair is the variable whose text
equals the normalised `rawSql` of the request. A request that pairs with no variable or with two variables gets the cause
`other`.

The time ranges are fixed and in the past. D is 00:00 UTC of the day of the run.

| Range | `from` | `to` |
|---|---|---|
| R1 | D minus 1 hour | D |
| R2 | D minus 24 hours | D |
| R3 | D minus 7 days | D |

The variable sets:

| Set | Values |
|---|---|
| S1 | The saved values of each variable (`current`). |
| S2 | "All" for each variable with `include_all`; S1 for the others. |
| S3 | The first two options for each multi-value variable; S1 for the others. |

So each dashboard opens 9 times: 3 ranges by 3 sets.

The compare program is the ignored test `parity_private` of the crate `hune-grafana`. The operator runs it with
`cargo test -p hune-grafana --test parity_private -- --ignored`. It reads the capture directory from the variable
`HUNE_PARITY_DIR`, because libtest reads each free argument as a test filter. For each recorded query, it expands the SQL
of the imported document with the expander of section 7. The inputs of the expansion are the variable values, and `from`, `to` and `maxDataPoints` of the request. The
`min_interval` of the panel is also an input.

The program compares the result with `results.<refId>.frames[0].schema.meta.executedQueryString` of the answer. It also
compares the interval of section 7 with the `intervalMs` of the request. It normalises both texts first. The lexer of
section 7 reads each text. Each run of white space outside literals and comments becomes one space. The program removes the
white space at both ends.

A difference gets one cause:

| Cause | Meaning | Action |
|---|---|---|
| `escape` | A value holds `'` or `\`. Hune escapes it; Grafana inserts it raw. | Expected by section 7. The operator signs the list. |
| `interval` | The interval differs. | Fix Hune, or the operator signs the cause. |
| `format` | A variable format gives other text. | Fix Hune, or the operator signs the cause. |
| `other` | Any other difference. | Fix Hune before the cutover. |

The compare program also reads the panels of a PostgreSQL source. The Grafana PostgreSQL data source gives `executedQueryString` in the same member. The macros of 7.22 have the text of Grafana, so they give no difference. The causes `escape` and `format` apply, with the rules of 7.21. A macro of the dialect `postgres` in a string gives the cause `other`, and the report lists it as `changed` (15.6).

The compare program does not compare the query text of a MongoDB panel. The plugin gives no executed query in the form of 7.35, and the text that Hune sends is BSON. The check is by result: the disposable MongoDB of 16.9b holds synthetic data, and the harness runs the pipeline in the plugin and in Hune. It compares the frames with the types of 7.16c. The difference list is in `tools/parity/vector-differences.json` (16.13).

#### 15.14b The parity harness

The harness is a second Playwright script, `tools/parity/compare.mjs`. It opens each dashboard in Grafana and in the
Hune instance of the coexistence (15.15). Both get the same URL state and the same window. That Hune instance reads the
imported documents and the same ClickHouse. The harness opens a panel in the two tools within one minute. Thus the data of
a past range does not move.

For each panel, the harness compares these items:

- For a table panel: the column headers in order, after the field configuration (display names, hidden columns).
- For the other six types: the series names (the display names of 9.13).
- For every panel: the data after the transformations, as display texts. Grafana gives them in "Inspect, Data", with
  the options "Data with transformations" and "Formatted data", as CSV. Hune gives them in the "View data" dialog of
  the panel (9.11). The harness reads the table of that dialog. The button "Download CSV" of the dialog gives the same data as a file (9.11).
- The `max_data_points` that Hune sends for the panel (the body of 6.20), against the `maxDataPoints` of the Grafana
  request for the same panel and window. A difference gets the cause `interval` (19.31).
- These rules of Grafana, which the project took from the source code only:
  - the field order of long-to-wide: the order of first appearance (9.22);
  - a regex mapping with `$1` in its result text (9.17);
  - the order of the slices of a pie chart (9.19);
  - the unit tables: the text of each unit in use (9.15);
  - `now/w` starts on Monday at 00:00 (9.9).

The cells compare as texts, after the removal of white space at both ends. A difference gets one cause:

| Cause | Meaning | Action |
|---|---|---|
| `precision` | An integer above 2^53 that Grafana rounds. | Expected (7.16). The operator signs the list. |
| `format` | A unit, decimal or mapping rule gives another text. | Fix Hune, or the operator signs the cause. |
| `data_moved` | A new run gives no difference. | None. |
| `other` | Any other difference. | Fix Hune before the cutover, or the operator signs the cause. |

The converter `tools/parity/to_grafana.py` writes `allowCustomValue` for each `query` and `custom` variable: `true` for an open variable and `false` for a closed one. Grafana reads an absent member as `true`, so the converter never leaves it out. The capture then compares the same control on both sides. The fixture `tests/fixtures/grafana/open_variables.json` has one open and one closed variable of the kinds `query` and `custom`, and one `text` variable. It is the output of the converter for `tests/fixtures/dashboards/open_variables.json`.

The converter writes an `interval` variable as the type `interval` and a `constant` variable as the type `constant`. The fixture `tests/fixtures/grafana/variable_kinds.json` is its output for `tests/fixtures/dashboards/variable_kinds.json`. It leaves out the stored `current` and `options` of a constant, which only repeat the value. It drops the `placement` of the interval variable with a warning, as for any variable (4.7a).

#### 15.14c Visual check

The capture tool takes a full-page screenshot of each dashboard in Grafana and in Hune. It uses range R2, set S1 and a
window of 1920 by 1080 pixels. Hune gives one screenshot in the light theme and one in the dark theme. The operator
reviews them side by side and writes OK or a defect for each panel. A defect needs a fix and a new review, or a signature
of the operator.

The parity harness has three fixture dashboards for the repeat: `tests/fixtures/parity/repeat-h.json`, `repeat-v.json` and `repeat-section.json`. The first has a panel with `h` and `max_per_row` 3. The second has a panel with `v` and panels around it. The third has a section with a repeat. The converter `tools/parity/to_grafana.py` writes them with `repeat` (4.13a, 15.6).

The capture finds a panel of Grafana by `data-viz-panel-key` (`panel-<id>` and `panel-<id>-clone-<n - 1>`). It finds a panel of Hune by `data-testid` (`panel-<id>` and `panel-<id>-r<n>`). It compares the box of each copy, the title and the number of queries. The order of the copies is the order of the options in both, so the fixtures select all the values. A difference that 19.20b lists is not a fault.

The tool `tools/parity/visual.mjs` makes these comparisons. It takes one Grafana dashboard file and the uid of the same dashboard in Hune. It saves the file in the folder `hune-parity` of Grafana under the uid `hpar-<uid>`. It deletes the dashboard at the end. It deletes the folder too when it made the folder and the folder holds no other dashboard. Another run can use the folder at the same time, and the removal of a folder removes its dashboards.

Grafana 12.4.2 ignores milliseconds in `from` and `to` on a dashboard URL. So the tool writes the time of the Grafana URL as an ISO time in UTC.

The tool sets the window of Hune so that the widest panel without repeat has the same width in both products. The canvases then have the same size.

The tool pairs the copies by the id and the number of the copy. The copy k of Grafana (`panel-<id>-clone-<k>`) is the copy k + 1 of Hune (`panel-<id>-r<k + 1>`). In Grafana, a panel of a repeated section has the same key in each copy. The n-th element with this key is the copy n. The number of queries of a copy is the number of queries in its requests in Grafana. In Hune it is the number of requests that carry the value of the copy.

The option `--replay DIR` compares the captures that an earlier run saved in `DIR/capture.json`. It opens no browser and writes nothing to Grafana.

The option `--grafana-datasource-map FROM=TO` saves the dashboard with another data source uid. Use it for a source that the disposable Grafana does not have. The option `--grafana-quote-placeholders` writes each `${name}` of a SQL text as `${name:singlequote}` in Grafana. Hune quotes a value in a code context, and Grafana gives the raw text.

The hook `tools/parity/canvas-hook.js` records the calls of each canvas since its last clear. These are `fill`, `fillRect`, `stroke`, `fillText`, the rectangles of the paths and the clip rectangle.

The rules of `tools/parity/visual-rules.mjs` give each difference a class. A difference that a section of 19.20 lists has the class `documented` and the id of the section. Any other difference is a `fault`.

The file `tests/fixtures/parity/visual-open-faults.json` lists the faults that are open, each with its cause. A case fails on a fault that this file does not list. The cases B101p, B137 and B148 run the tool when the Grafana of 127.0.0.1:3010 answers.

The tolerance of a mark on a canvas is 1.5 px. This is the 1 px of 15.14c and 0.5 px for the snap of a cell of Hune to a device pixel. A plot that moves or scales by more than 2 px is a fact of its own, and the check `plot` reports it. The marks of Hune map into the plot of Grafana in x when the plots differ. They map in y when the heights differ by more than 4 px.

The library uPlot of Grafana clips a series to the plot and half its line width. Thus the plot of Grafana is its clip less half the widest line in the clip.

The bars of a histogram pair by their x. With a `bucketOffset` that is not a multiple of `bucketSize`, they pair by their x value, from the labels of each x axis (19.20g). The height of a bar compares as a share of the tallest bar. A time series whose y axes are all hidden has the check `plot:top` (E27, 9.24). It compares the top of the plot of the lines, within 2 px.

The parity harness has three fixture dashboards for the text panel. The first has each construct of 9.19b with no variable. The second has placeholders with plain values. The third has links, a table and a code block.

The converter `tools/parity/to_grafana.py` writes a text panel as the panel of Grafana. It writes `options.mode` = `markdown`, the joined `content` and the default `options.code`, and no `targets`.

The capture finds the root of Grafana by `data-testid="TextPanel-converted-content"`. It finds the root of Hune by `data-testid="text-<id>"`. It compares the box, the title, the text of the body and the sequence of element names. A difference that 19.20c lists is not a fault. The values of the variables in the fixtures hold no Markdown character, because Grafana replaces them before it parses.

The parity harness has the fixture dashboards of the two types. They cover the value mappings (value, range, regex and special). They cover a numeric field in the colour modes `thresholds`, `fixed` and `continuous_gr_yl_rd`, a text field, a bool field and several rows.

They also cover the null values and the gaps, and each option of 4.33b. They cover both legend places. The fixtures are 8 dashboards that the project measured on Grafana, saved as `tests/fixtures/grafana/timelines_*.json`.

The converter `tools/parity/to_grafana.py` writes the two types as the panels `state-timeline` and `status-history`, with the options and the `custom` members of 15.6. The capture records the canvas calls of both pages with one hook script (`fill`, `stroke`, `fillText` and the rectangles of the paths). It compares the boxes with a tolerance of 1 px, the colours, the texts and the tooltips. A difference that 19.20d lists is not a fault.

The parity harness has 4 fixture dashboards for the annotations. They are the files `tests/fixtures/grafana/annotations_*.json`: `colours`, `filter`, `texts` and `many`. Each one is the output of the converter for the document `tests/fixtures/parity/annotations-*.json`. The first has points and regions in 3 colours on 2 time series. It has a region that starts before the range and a region that ends after it.

The second has the filter `panels` and an annotation with `enabled` equal to `false`. The third has texts with a tag, a long text and a text with a line end. The fourth has 1,000 points.

The converter `tools/parity/to_grafana.py` writes the member `annotations`. Each annotation becomes an item of `annotations.list`. The item has `datasource`, `enable`, `hide` equal to `false`, `iconColor` (the text of the colour) and `name`.

It has `target` (made by `target_out`, with the format `table` and the ref id `Anno`). It has `filter` (`exclude` equal to `false` and `ids`) or no `filter`. The converter writes the built-in item too, with `hide` equal to `true`. The importer reads the output back to the same members, and it lists two items for each annotation: `hide` equal to `false` and the warning of 15.6.

The capture records the canvas calls of Grafana (the lines with `setLineDash`, the fills and the DOM markers `annotation-marker`) and the data `data-annotations` of Hune. It compares the x of each mark (1 px), the width of each region (1 px) and the colour of each line. It compares the number of marks. It also counts the requests: 1 annotation request for each annotation in both products. The differences of 19.20e are not faults.

Several queries (4.15a). The capture compares the data of a panel for each ref. The key of a record is the panel and the ref, as it was the panel and the `refId` before. Grafana sends one request for each panel and each data source, and Hune sends one for each query, so the count of requests differs. The report lists this difference once (19.20f). The capture pairs the request of Hune with the query of Grafana that has the same ref.

The visual capture of 15.14c reads the display names of the combined frame (`A a`, `B b`). It reads the legend items, the headers of a table and the select of the frames.

The converter `tools/parity/to_grafana.py` writes one target for each query, with `refId`, `hide` and the `datasource` of the source. It writes the data source `-- Mixed --` for a panel whose queries have more than one source. It writes the matcher `byFrameRefID` for the matcher `query`. The 6 fixtures `tests/fixtures/grafana/multi_query_*.json` are the output of the converter for the documents `tests/fixtures/parity/multi-query-*.json`. They hold these panels:

- a time series with 3 queries, and a table with 2 queries;
- a stat with a hidden query, and a panel with Mixed sources;
- a panel with an error in one query, and a panel with all queries hidden.

The parity harness has the fixture dashboards of the heatmap and of the histogram. The heatmap fixtures cover the shape that is already bucketed. They cover names that are numbers and names that are not, null cells, zero cells, one time row and irregular times. They also cover the shape of raw values, with each rule of the time size and of the value size and the three scales.

The fixtures also cover each colour option, each cell option, each place of the axis, the three modes of the card and the legend. The histogram fixtures cover each bucket option, `combine`, the three modes of `stacking` and the options of the line and the fill. They cover these cases of the data. They are negative values, two clusters with a gap, equal values, one value, null values and a very small bucket size.

The fixtures are 8 dashboards that the project measured on Grafana. They are in `tests/fixtures/grafana/` as `heatmap_*.json` and `histogram_*.json`: the files `heatmap_a`, `heatmap_b`, `heatmap_c`, `heatmap_d`, `heatmap_e`, `histogram_f`, `heatmap_g` and `histogram_h`. Each panel has its own row (4.14), and the range is two epoch times in milliseconds (4.4).

The fixture `tests/fixtures/grafana/parity_axes.json` holds three time series with all their y axes hidden and one time series with an axis. It also holds the histograms of the measure of the y ticks, with the heights 5 and 16. The tool runs it by hand.

The converter `tools/parity/to_grafana.py` writes the two types as the panels `heatmap` and `histogram`, with the options and the `custom` members of 15.6. The capture records the canvas calls of both pages with one hook script (`fill` and the rectangles of the paths, `fillText`). It compares the boxes of the cells and of the bars with a tolerance of 1 px, the colours, the texts and the cards. A colour of the 9 exact schemes has a tolerance of 2 in each channel. A colour of `viridis` has a tolerance of 8. A difference that 19.20g lists is not a fault.

### 15.15 Coexistence, cutover and rollback

The cutover happens after the exit criteria of M4 (section 18). Hune then runs in volume storage and in mode `open`,
behind the login of the reverse proxy that guards `/grafana/` today. T is the day of the proxy switch. All dates below are
durations from T. This runbook uses nginx and Docker Compose as examples. Adapt the commands to your set-up.

Preconditions:

- The steps V1 to V5 of 15.14 passed. The export was at most 7 days old at T minus 14 days.
- Each server action of the plan has a written recap before it runs, and the person who is responsible for the instance
  authorised it.
- Nobody builds the Grafana image again from the start of V3 until T plus 30 days. When the image has no pinned version,
  a new build can change the Grafana and plugin versions that V3 measured.
- A test environment ran the same plan first, with its T at least 7 days before the T of the production environment.

| When | Step |
|---|---|
| T − 14 d | Add the service `hune` to the compose file of the host, with the volume as a read-only store: no `primary = true`, no `history_keep`, and a `title`. Add the nginx location `/dashboards/` (below). Run the first import with `--write`. The importer uses a second file, `import.hcl`, equal to the file of `serve` except that the volume has `primary = true` and `history_keep`, and has no `title`. A read-only `serve` takes no lock (5.19), so the importer takes it (5.16). |
| T − 14 d to T − 2 d | Parallel run. Grafana stays the editor and the source of truth. After each Grafana change, and at least one time each day, export and import again with `--write --replace`. Users compare the two tools. Hune fixes come as new releases. The operator reads the nginx log for `/grafana/public-dashboards/` and `/grafana/goto/` (19.2). |
| T − 2 d | Edit freeze. Give the role Viewer to each Grafana user. When an environment variable of the Grafana service sets the role, create the container again without a build (`docker compose up -d --no-deps grafana`). Grafana then refuses saves. Tell the users about the freeze. |
| T | Final export and final import with `--write --replace`. Run V3 again for each dashboard that changed after the last V3. Give the volume of Hune `primary = true` and `history_keep`, remove its `title`, and start it again. Set `GF_SERVER_ROOT_URL=/grafana-old/` on Grafana, create it again without a build, and route `/grafana-old/` to it. Replace the nginx location `/grafana/` with the redirects of 15.17. |
| T to T + 14 d | Grafana stays reachable read-only at `/grafana-old/`, behind the same login, for comparison. |
| T + 14 d | Stop the Grafana container (`docker compose stop grafana`). Do not touch its data volume. |
| T + 30 d | End of the rollback window. Remove the Grafana service and archive `grafana.db` (19.10). |
| T + 6 months | Remove the redirect locations of 15.17. |

The nginx location of Hune copies the access rule of the current `/grafana/` location. With `server.base_path` set to
`/dashboards/` (section 13), nginx passes the path without a change:

```nginx
location /dashboards/ {
    # Copy the access lines of the current location /grafana/ here, unchanged.
    auth_request /auth/verify;
    proxy_intercept_errors off;

    proxy_pass http://hune:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_read_timeout 120s;
}
```

From T to T + 14 d, the old Grafana answers under `/grafana-old/` with the same access rule:

```nginx
location /grafana-old/ {
    # Copy the access lines of the current location /grafana/ here, unchanged.
    auth_request /auth/verify;
    proxy_intercept_errors off;

    proxy_pass http://grafana:3000/grafana-old/;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

Rollback is possible from T to T + 30 d. These are its triggers:

- A defect that stops the daily work, with no fix within 1 business day.
- A panel that shows wrong data, with no fix within 1 business day.
- A decision of the operator.

Rollback procedure:

1. Remove `primary = true` and `history_keep` from the store of Hune, give it a `title`, and start it again. This stops new edits.
2. Run `hune export -c <file> --to <dir>` (5.38) to a private directory. It keeps the edits that people made in Hune
   after T.
3. Give the Grafana users their roles of before the freeze again. Remove `GF_SERVER_ROOT_URL` from the Grafana service. Start
   it again without a build.
4. Restore the old nginx location `/grafana/`. Remove the redirect locations. Run `nginx -t`, then reload nginx.
5. Tell the users about the rollback. People apply their Hune edits in Grafana by hand. Hune never writes to Grafana.

### 15.16 Changes after the cutover

After T, Hune is the source of truth. The importer is not used again for these dashboards. A dashboard that someone
exports from another Grafana later uses the same command, with a new acceptance file.

### 15.17 The nginx redirect map

The redirects keep the old links usable for 6 months: bookmarks, chat links and the short links. `--nginx-map <file>`
writes the two maps below. The operator includes the file in the `http` context of nginx.

```nginx
# Written by hune import-grafana. Include in the http context.
map $hune_grafana_uid $hune_new_uid {
    default "";
    "abc123" "abc123";
    "Nodes_Prod" "nodes-prod";
}
map $hune_grafana_short $hune_short_target {
    default "";
    "xYz12AB" "/dashboards/d/abc123/nodes?from=now-7d&to=now&var-node=%24__all";
}
```

The operator writes these locations in the `server` context once. They replace the location `/grafana/` at T:

```nginx
location ~ ^/grafana/d/(?<hune_grafana_uid>[A-Za-z0-9_-]+)(/[^?]*)?$ {
    if ($hune_new_uid = "") { return 302 /dashboards/; }
    return 301 /dashboards/d/$hune_new_uid$is_args$args;
}
location ~ ^/grafana/goto/(?<hune_grafana_short>[A-Za-z0-9_-]+)$ {
    if ($hune_short_target = "") { return 302 /dashboards/; }
    return 302 $hune_short_target;
}
location /grafana/ {
    return 302 /dashboards/;
}
```

The rules of the map:

- A dashboard line maps each imported Grafana uid to its Hune uid (15.8). The redirect keeps the query string. Hune
  uses the Grafana query names `from`, `to`, `var-<name>`, `refresh` and `viewPanel`, and drops `orgId` (9.5).
- A short-link line maps the short uid to its target. The importer reads `path` of `short-urls.json`. A path
  `d/<grafana uid>[/<slug>][?<query>]` becomes `<new>d/<hune uid>[/<slug>][?<query>]`, with `<new>` from
  `--link-prefix`. Another path becomes `<new>` and an item of class `changed`.
- The importer percent-encodes seven characters in a target: `$`, `"`, `\`, space, `;`, `{` and `}`. Their codes are
  `%24`, `%22`, `%5C`, `%20`, `%3B`, `%7B` and `%7D`. Reason: nginx reads `$` in a map value as a variable.
- A Grafana uid or a short uid with a character outside `[A-Za-z0-9_-]` gets no line. It gets an item of class `changed`.

nginx runs `return` in its rewrite phase, before `auth_request`. So the redirects answer without the login check. They give
no data: the target location `/dashboards/` checks the login.

The importer also rewrites data links of the documents with `--link-prefix`. A link that starts with `<old>d/<grafana uid>`
becomes `<new>d/<hune uid>` with the rest of the link unchanged. Without `--link-prefix`, such a link stays as it is and
gets an item of class `changed`.

### 15.18 Confidentiality

Real dashboards hold the business SQL of the instance. These things never enter the repository of Hune, an issue, a pull
request or a CI log:

- the exports of 15.3 and the copies of `grafana.db`;
- the reports of real runs, the acceptance files and the nginx maps of real runs;
- the requests, answers, CSV files and screenshots of 15.14;
- any SQL text of a real dashboard.

They stay on a trusted machine or a private runner, in a directory outside every git checkout, with mode 0700. The
importer and the tools of `tools/parity/` take every path from their command line, and the test `parity_private` from
`HUNE_PARITY_DIR`. None of them has a default path. They refuse an output path inside a git checkout (15.2).

Synthetic fixtures in `tests/fixtures/grafana/` cover each row of 15.6 (16.13). The policy check of 16.18 refuses a fixture
that names a production database or a host of a real deployment.

## 16. Tests

### 16.1 Rules for every test

- Every test runs in Docker. The CI runner needs only git, Docker Engine and the Docker Compose plugin.
- Each image that a test uses has a tag and a digest in `tests/images.lock`. One line holds `<name> <image>@sha256:<digest>`.
  The test scripts take images from this file only.
- No test connects to a server of a real deployment. No test holds a credential, an address or the data of a real deployment. The CI holds no
  credential of a real deployment. The verification of 15.14 is not a test. It is a manual step.
- A test is deterministic, or it uses a seed and prints the seed when it fails.
- A failed test prints the last 200 log lines of each container of its compose project.
- Each test project has the compose project name `hune-<job>-<run id>`. It removes only its own containers, networks and
  volumes.
- A case that expects an error checks the HTTP status and the error code of section 6 for the situation. The tables below
  name the situation.
- The builder image `hune-builder` comes from `build/Dockerfile.build` (17.2). The jobs run `cargo` only inside it.

### 16.2 Test layers

| Layer | What it checks | Where it runs | CI job (16.17) |
|---|---|---|---|
| Unit | The pure functions of each crate. | builder container | `rust` |
| Seeded random | The lexer and the substitution (16.4). | builder container | `rust`, `nightly` |
| Configuration | The key table, the forms, the contradictions (16.5). The environment case K14 needs ClickHouse and `fake-pdp`. | builder container and the binary; K14 in `tests/compose/api.yaml` | `rust`; K14 in `api` |
| Store conformance | The `Store` trait on the volume and on PostgreSQL 15 and 18 (16.6, 16.7). | `tests/compose/store.yaml` | `store` |
| Restore drill | Export, import and byte equality (16.14). | `tests/compose/store.yaml` | `store` |
| API black-box | The HTTP behaviour of the binary, both backends, both modes (16.8). | `tests/compose/api.yaml` | `api` |
| ClickHouse integration | The client, the limits, the cancellation, the types, the account (16.9). | `tests/compose/source.yaml` | `source` |
| PostgreSQL source integration | The client, the grammar, the limits, the cancellation, the types, the role (16.9a). | `tests/compose/pgsource.yaml` | `pgsource` |
| MongoDB source integration | The client, the grammar, the limits, the cancellation, the types, the user (16.9b). The cases with no server run in `rust`. | `tests/compose/mongosource.yaml` | `mongosource` |
| Front-end units | The pure functions of the browser, with golden vectors (16.10a). | the browser test image, with Node | `frontend` |
| Browser | The front end and the editor (16.10). | `tests/compose/browser.yaml` | `browser` |
| PostgreSQL failover | Patroni with three nodes (16.11). | `tests/failover/compose.yaml` | `failover` |
| ProcurAuth series U | The PEP (16.12). | `tests/conformance/run.sh` | `procurauth` |
| Importer | The mapping, the report, the exit codes, the expansion vectors (16.13). | builder container and `tests/compose/grafana.yaml` | `import` |
| Image and deployment | The static binary, the image, the compose and Kubernetes examples (16.15). | `tests/ci/image.sh` | `image` |
| Fuzz | The lexer, the expander and the parsers (16.16). | `fuzz/` | `nightly` |
| Load | 52 viewers on the largest dashboard (16.16). | `tests/load/compose.yaml` | `load` |

### 16.3 Unit tests

`cargo test --workspace --locked` runs the unit tests of each crate. They hold at least these groups:

| Crate | Groups |
|---|---|
| `hune-core` | Closed parse of the document: unknown member, duplicate member, wrong type, byte order mark, data after the value. Each error has its JSON pointer. Each format limit at its maximum and one above. Canonical bytes and etag of fixed documents (golden files). The id rules. The lexer on the shared vectors (16.4). Each macro and the interval rule on the golden expansion vectors (16.13). The statement check. The variable rules of section 7 by context. The frame types. For the dialect `postgres`: the lexer vectors of 7.20, the escape vectors of 7.21, the golden expansion vectors of each macro of 7.22 and 7.23, the statement check of 7.25, the types of 7.16a and the rule of 7.16b. For the kind `mongodb`: the parser of 7.28 on the golden vectors of `tests/golden/pipeline-mongo/`, the grammar of 7.30 to 7.33 with each stage and each operator of 7.31 and each name of 7.32, the expansion and the property PM of 7.34 and 7.35, the conversion of each BSON type to a frame value (7.16c), the mixed columns, and the checks at save of 7.36. The members of format 3 (4.5a, 4.6, 4.15, 4.18, 4.19) have tests in `document_f2_members.rs`. Each member has a test at its limit and one above, with the code and the pointer of each error. The grammar of the time override and of the zone names has its own tests. The migration `migrate_2_to_3` has its tests in `document_migrate.rs`. Each golden pair of `tests/fixtures/dashboards/format2/` gives the file of `tests/fixtures/dashboards/`. A second call gives `format_version`. A top-level `links` gives `unknown_member`. The value `palette_by_name` of `color.mode` gives `bad_format`, and the message names the mode `palette`. A layout that is not canonical gives the same bytes. The worked example of 4.40 has 8,397 bytes. The chain 1 to 2 to 3 gives, for each file of `tests/fixtures/dashboards/format1/`, the same bytes as the file of format 3. The member `placement` of a variable (4.7a): `document_placements.rs` has 21 tests. They read the fixture `placements.json`, which is valid and canonical. Each of the 20 files `invalid/k_place_*.json` gives its one error with its code and pointer. The tests also check the canonical order, the absent member that leaves the bytes of a document unchanged, the rules 2 to 5, 7 and 10 of 4.7a, the section index against `sections`, and the limit of 64 variables with a place. The repeat of a panel and of a section (4.13a): the valid fixture `tests/fixtures/dashboards/repeats.json` has a golden canonical form, with `repeat` as the last member of a panel and of a section and `max_per_row` as the last member of its object. 20 invalid fixtures `tests/fixtures/dashboards/invalid/f23_*.json` have one error each, with the code and the pointer of 4.34. The constants `MAX_REPEAT_PER_PANEL`, `MAX_REPEAT_PER_SECTION` and `MAX_REPEAT_PER_DASHBOARD` equal the defaults in the table of 4.35. A document with no `repeat` keeps its bytes. The text panel (4.33a): the valid fixture `tests/fixtures/dashboards/text_panel.json` has a golden canonical form with the members in the order of 4.38. It has an empty content, a content with a tab, U+2028 and a character outside the plane 0. The boundaries are valid: 2,000 lines, a line of 8,192 bytes (with a multi-byte character at the end) and a text of 65,536 bytes. 22 invalid fixtures `tests/fixtures/dashboards/invalid/f29_*.json` have one error each, with the code and the pointer of 4.33a. They cover `mode` = `html`, `mode` = `code`, `mode` absent and `mode` of a wrong type, a `content` that is not an array and an item that is not text, a line with U+000A, U+000D, U+0000, U+0001 and U+007F, a line of 8,193 bytes, 2,001 lines and 65,537 bytes, each of the members `data`, `transforms`, `fields` and `repeat` in a text panel, an unknown member and a duplicate member in `options`, a `reuse` that names a text panel, and a panel of another type that has the `options` of a text panel. A panel with `type` = `texts` gives `bad_format` at `type`: this is the answer of a binary that does not know the type. The constants `MAX_TEXT_LINES`, `MAX_TEXT_LINE_BYTES` and `MAX_TEXT_BYTES` equal the table of 4.35. A document with no text panel keeps its bytes, and the golden files of 4.40 do not change. The two types (4.33b). The valid fixtures `tests/fixtures/dashboards/state_timeline.json` and `status_history.json` have a golden canonical form. The members of `options` come in the order of 4.33b. The boundaries are valid: `row_height` 10 and 100, `col_width` 10 and 100, `line_width` 0 and 10, and `fill_opacity` 0 and 100. 30 invalid fixtures `tests/fixtures/dashboards/invalid/f30_*.json` have one error each, with the code and the pointer of 4.33b. They cover each member out of range by one and a member of the other type. They cover an absent member, an unknown member and a duplicate member. They cover `legend.mode` equal to `table`, a `legend.values` that is not empty, `sort_by` and `sort_desc`. They cover `tooltip.sort` equal to `asc`, `tooltip.hide_zeros` equal to `true`, and a style member of a series in a property set. A panel with `type` equal to `state_timelines` gives `bad_format` at `type`. This is the answer of a binary that does not know the type. A document with no such panel keeps its bytes. The golden files of 4.40 do not change. The heatmap and the histogram (4.33c, 4.33d). The valid fixtures `tests/fixtures/dashboards/heatmap.json` and `histogram.json` have a golden canonical form. The members of `options`, `x_buckets`, `y_buckets` and `color` come in the order of the tables. The boundaries are valid. They are each integer at its lowest and its highest value, `exponent` 0.1 and 2, `value` of `x_buckets` 1 and 31,536,000,000, and a negative `bucket_offset`. 60 invalid fixtures `tests/fixtures/dashboards/invalid/f31_*.json` have one error each, with the code and the pointer of 4.33c and 4.33d. They cover each member out of range by one and a member of the other type. They cover an absent member, an unknown member and a duplicate member. They cover the log scale with the mode `count`, and a log `value` of 0, 17 and 1.5. They cover `color.min` equal to `color.max`, and `y_min` of 0 with a log scale. They cover `tooltip.sort` equal to `asc`, `tooltip.hide_zeros` equal to `true`, and a style member of a series in a property set. A panel with `type` equal to `heatmaps` gives `bad_format` at `type`. This is the answer of a binary that does not know the type. A document with no such panel keeps its bytes. The golden files of 4.40 do not change. A migration of a document of format 1 with 20,000 panels that have `transparent` gives `MAX_ERRORS` errors, and the last error is `too_many_errors` (4.42). The members of a `barchart` (4.18, 4.30): `document_barchart_p03.rs` has 8 tests. An absent `fill_opacity` is 80, an absent `line_width` is 1, and the canonical form leaves out these defaults. The boundaries 0 and 100, and 0 and 10, are valid, and one above or below gives `out_of_range`. A number with a fraction, `null` and a text give `wrong_type`. The value `auto` of `orientation` is valid in a `barchart` and gives `bad_format` in a `bargauge`. `axis_grid` is valid in `defaults` and in an override of a `barchart`, and gives `not_permitted` in a `timeseries`. |
| `hclconf` | The tests of the engine, in `src/` and `tests/` of the crate. |
| `hune-store` | The path rules of the volume. The history file names. The SQL text of each migration step. |
| `hune-source` | The decoder of `JSONCompactEachRowWithNamesAndTypes` for each ClickHouse type of section 8. The map from ClickHouse error codes to the codes of section 6. The settings of each query. For PostgreSQL: the decoder of each type of 7.16a on binary samples, the map from SQLSTATE to the codes of section 6 (8a.8), the startup parameters (8a.4), the attribution comment (8a.11), the constant texts of 8a.5 and 8a.12, and the byte count (8a.6). For MongoDB, with the fake client of 8b.16: the steps of 8b.5, the row and byte caps and the stage `$limit` of `R + 1` (8b.6), the deadline and the kill (8b.7), the map from server codes to the codes of section 6 (8b.8), the wire guard, the attribution comment (8b.11), the check functions M0 to M4 on plain structures (8b.12), and the byte count |
| `hune-grafana` | One test for each row of 15.6 in each input form. The uid rule of 15.8. The source map. The redirect lines and the percent-encoding. The converter `tools/parity/to_grafana.py` drops the `placement` of a variable (4.7a) with a warning on the standard error and keeps the variable. Its 6 tests are the class `VariablePlacement` in `tests/tools/test_to_grafana.py`. One test for each row of the repeat in 15.6, in the form v1 and in the form v2 (`GridLayoutItem.spec.repeat` and `RowsLayoutRow.spec.repeat`). Separate tests cover the default direction, `maxPerRow` with `v`, the shared row, the nested repeat, a variable with `multi` = `false` and the stored copy with `repeatPanelId`. The text panel: one test for each new row of 15.6, in the form v1 and in the forms v2. Separate tests cover the fence of a code block for runs of 0 to 5 backticks, the line ends `\r\n` and `\r`, the removal of a control character, and each construct of the list under the table. A test checks that a content with hostile text goes to the document unchanged, and that the document passes the validation. The file `tests/fixtures/grafana/text_panels.json` gives the rows of the case B109. The state timeline and the status history: one test for each new row of 15.6, in the form v1 and in the forms v2. A test covers each value of `rowHeight` and `colWidth` at the boundaries. A test checks that `perPage`, `insertNulls` and a numeric `spanNulls` make the items of the rows. The converter `tools/parity/to_grafana.py` writes both types, and the importer reads the output back to the same members. The 8 dashboards of 15.14c are the files `tests/fixtures/grafana/timelines_*.json`. The heatmap and the histogram: One test for each new row of 15.6, in the form v1 and in the forms v2. A test covers each range at its boundaries. A test checks that each scheme name of Grafana maps or is `changed` as the row says. A test checks the old form of Grafana 7 to 9: the panel is removed, and the report names the first member. The converter `tools/parity/to_grafana.py` writes both types, and the importer reads the output back to the same members. |
| `hune` | The router: each route of section 6 and its method. The write guard. The security headers. The time model: `from < to`, epoch milliseconds. The key `server.query_inspect` (6.20): 11 server tests in `crates/hune/src/server/tests/inspect.rs`. They check that the answer has `sql` only with the key `true` and that the session member says the same. They check that an error answer, the route of 6.21 and the preview of a variable have no `sql`. They check that a variable value is in the SQL and that no configuration secret is in an answer or in a log line. In the mode `authzen`, they check the extra question of 11.21 and the cases with a subject that has only `dashboard.read`. The parameter `tag` of 6.12: 4 server tests. They cover a tag that matches, a tag that does not match, `tag` with `q`, and a tag of 65 bytes that gives 400. The test `a24_preview_ignores_the_placement_of_the_draft_variables` checks that a draft variable with a `placement` (4.7a, 6.22) gives no error in the preview. The query rule of a repeat (4.13a): 9 server tests in `crates/hune/src/server/tests/repeat.rs`. A save with a `repeat` gives the canonical bytes with the member last, and three invalid documents give `422 invalid_document` with the codes of 4.34. A panel query of a copy has the repeat variable with `all` equal to `false` and one value, and with `server.query_inspect` the expanded SQL holds that value only. A value that is not an option of a `custom` variable gives `422 variable_value_refused`. A hostile free value gives the same code and no query at the source, for ClickHouse, PostgreSQL and MongoDB (the properties P, PG and PM). 24 requests of copies at the same time, with a small `max_concurrent_per_subject`, give permits, queue and `503 source_busy` as 8.8 says. The log line `query.run` has the panel id and no variable value, and the route table (6.9) has no new route. The three keys `server.repeat_max_per_*` and the member `repeat_limits` of the session: see the case K19. The text panel (4.33a, 6.20, 6.22): 6 tests in `crates/hune/src/server/tests/text_panel.rs`. A save with a text panel gives the canonical bytes, and a read gives the same bytes. Three invalid documents give `422 invalid_document` with the codes of 4.33a. A panel query and a preview of a text panel give `400 bad_request`, and the route table (6.9) has no new route. A `reuse` of a text panel gives `422 invalid_document` with `bad_reference`. In the mode `authzen`, a subject that has `dashboard.read` and no `source.query` reads a dashboard that has a text panel only. The logs of the run hold no line of the content: a marker text in a content is not in any log line. The state timeline and the status history (4.33b, 6.20): 4 tests in `crates/hune/src/server/tests/timeline_panels.rs`. A save with each type gives the canonical bytes, and a read gives the same bytes. Three invalid documents give `422 invalid_document` with the codes of 4.33b. A panel query of each type gives `200` as for a table. A `reuse` of such a panel works as for a time series. The heatmap and the histogram (4.33c, 4.33d, 6.20): 5 tests in `crates/hune/src/server/tests/bucket_panels.rs`. A save with each type gives the canonical bytes, and a read gives the same bytes. Four invalid documents give `422 invalid_document` with the codes of 4.33c and 4.33d. A panel query of each type gives `200` as for a table, and a `reuse` of such a panel works as for a time series. A panel with `repeat` and `extra_queries` of these types follows the rules of 4.13a and 4.15a. |


The annotations (F25) add these tests to the groups of the table:

- `hune-core`: the member `annotations` (4.5b) and the function `annotations_from_frame` (7.37).
  - The valid fixture `tests/fixtures/dashboards/annotations.json` has a golden canonical form. The member comes after `variables`. The boundaries are valid: 8 annotations, a name of 256 characters, and a colour of each form of 4.19. A test builds a document with 200 panels and 200 items in `panels`.
  - 24 invalid fixtures `tests/fixtures/dashboards/invalid/f25_*.json` have one error each. 19 of them are errors of the format (4.5b and 4.34). They cover 9 annotations, a repeated name, a repeated panel id, an unknown panel id, an empty `panels` and 201 items. They also cover an unknown member, an absent `color` and a colour that is not valid. 5 of them are errors of the save checks of 7.18: an unknown source and each interval macro. `hune-core` reads these 5 as valid documents, and the server tests refuse them.
  - A document with no annotation keeps its bytes. The golden files of 4.40 do not change. A document with `"annotations": []` is valid, and its canonical form has no member `annotations`.
  - `annotations_from_frame`: 66 vectors in `tests/golden/annotations/frames.json`. They cover the roles of the columns and their case, and the names `time_end` and `end` (no role). They cover a duplicate role, a time of each kind and a time at the two bounds. They cover the end as `null`, 0, equal and below the time. They cover the three forms of the tags and the limits of the text.
  - 42 hostile texts give `bad_chars`, in a title, a text and a tag. They hold control characters, the bidirectional controls, and the forms `<b>`, `</i>`, `<!--` and `<script>`. 12 texts with `<`, `&`, quotes and `javascript:` that are not an HTML tag pass.
  - A test checks that no message of `annotation_invalid` holds a cell value. A test checks that the marks come in the order of the time, then of the row.
- `hune-grafana`: one test for each new row of 15.6, in the form v1 and in a synthetic form v2. A test covers the 8 items and the 9th, the filter with `exclude` equal to `true`, and an `iconColor` that is not a colour. Other tests cover each unsupported case. The converter `tools/parity/to_grafana.py` writes the member, and the importer reads the output back to the same members.
- `hune` (the server): 16 tests in `crates/hune/src/server/tests/annotation_query.rs` (k01 to k16). They cover the answer of 6.21a, an index that is not valid, a disabled annotation and a body with `max_data_points`. They cover a stale etag (`409`) and a variable value that breaks the property P.
  - They cover 1,000 and 1,001 rows, a ceiling `max_rows` of 10 (`403 ceiling_exceeded`) and `annotation_invalid` for each reason. They cover a viewer with `dashboard.read` only. A test checks that the log line holds no title, no text and no tag. A test checks that the route is in the table of 11.21.
  - The save tests: a save with an annotation asks `source.query` for its source (11.21). A save with an interval macro in the SQL gives `invalid_document` with `not_permitted`. The three kinds of 8.2 are valid for an annotation. A unit test checks the closed list of `annotation_kind`, which the save and the route use.

The annotations from PostgreSQL and MongoDB sources add these tests:

- `hune-core`: rule 13 of 7.37 (a frame with no column and no row gives no mark). The frames of the client of 8b give marks.
  - The documents hold a Date, an absent field, an array of strings and a string with commas.
  - Int32, Int64, Double and Decimal128 are whole epoch milliseconds. A fraction, NaN, a negative value and a value above the bound give `bad_time`.
  - A String, a Boolean, a Document and an Array are not a time. A number and a Boolean are not a text.
  - 5 hostile texts give `bad_chars` or `too_long`. The check L9 of 7.36 names the annotation in its message.
- `hune-source`: the frames of the PostgreSQL types give the marks or the reasons of the table of the kinds of 7.37.
  - The types are `timestamptz` (with microseconds and `infinity`), `varchar`, `text[]` (with a NULL element), `int4`, `int8`, `numeric`, `float8` and `date`.
  - The types `time`, `bool`, `uuid` and `json` give their reasons or their texts. 5 hostile texts give their reasons.
  - The attribution of an annotation query has the kind `annotation` and the member `annotation` (8a.11, 8b.11).
- `hune-grafana`: the rows of 15.6 for the two plugins.
  - An item of the PostgreSQL plugin maps with the dialect `postgres`. An interval macro, a refused function and the visual builder give their rows.
  - An item of the MongoDB plugin maps to the object of 7.29. `queryType` = `timeseries` and a member of a SQL plugin give their rows.
  - An interval macro, a refused stage, a text that is not JSON, an empty `queryText` and an absent `collection` give their rows.
- `hune` (the server): the tests k13, k15 and k17 to k20 of `annotation_query.rs`.
  - A save on a `postgres` source runs the checks of 7.26. The route reaches the source with the cap of 1,000 rows.
  - On a `mongodb` source, the fake client of 8b.16 gives the documents. The route gives the marks, the closed answer, the log line and the comment of 8b.11.
  - An empty result gives no mark and no fault. 1,000 rows give 1,000 marks, and 1,001 rows give `422 query_limit`.
  - The route refuses a value that breaks PM2 or the options of a closed variable before the source. A hostile text gives no cell value.
  - The route asks one `dashboard.read` question and takes the ceiling `max_rows` (`403 ceiling_exceeded`). The save runs the checks L3, L8 and L9 of 7.36.
- Container suites: the cases PS17 (16.17) on PostgreSQL 15 and 18, and MS20 (16.9b) on MongoDB 7.0 and 8.0. The browser case B217.
- Fuzz: the seed corpus of `document_parse` holds the fixture `annotations.json` and the invalid files `f25_*.json`. A new target `annotation_frame` feeds `annotations_from_frame` with frames made from random bytes. It checks that the function never panics and that an accepted frame gives marks that obey 7.37. The seed corpus of `annotation_frame` holds the 66 vectors.

Several queries in one panel (F24) add these tests to the groups of the table:

- `hune-core`: the several queries of a panel (4.15a).
  - The valid fixture `tests/fixtures/dashboards/queries.json` has a golden canonical form. `extra_queries` comes after `query`. `ref` and `hide` are the last members of `data.query`, and an item has the order `ref`, `source`, `format`, `sql`, `hide`.
  - 30 invalid fixtures `tests/fixtures/dashboards/invalid/f24_*.json` have one error each, with the code and the pointer of 4.34. They cover an empty array and 8 items. They cover a ref used twice, as the default `A` and an item, and as two items. They cover an item with no `ref`.
  - They cover a ref that breaks the grammar: a digit first, 17 characters, a wrong type and the empty text. They cover a `hide` of a wrong type in `data.query` and in an item.
  - They cover an item with one of the members of the panel (4.15a, rule 4). A separate test checks the five members. They cover an unknown member and a duplicate member in an item, a wrong `format`, a wrong `source` and a `sql` with a line end.
  - They cover `extra_queries` with `reuse`, in a `barchart`, in an `xychart` and in a text panel. They cover it in a panel with `repeat` and in a panel of a section with `repeat`.
  - They cover a `reuse` panel of the type `barchart` or `xychart` whose source has `extra_queries`. They cover 401 queries in the document.
  - They cover the matcher `query` with no `value`, with a `value` of 17 characters, with `names`, and with a `value` that breaks the grammar.
  - The boundaries are valid: 8 queries in a panel, 400 queries in a document, a ref of 16 characters, and a first query that has `ref` and `hide`.
  - The constants `MAX_QUERIES_PER_PANEL` and `MAX_QUERIES` equal the table of 4.35. A document with no extra query keeps its bytes. The golden files of 4.40 do not change.
  - A property test builds 1,000 random documents with 1 to 8 queries. It checks that the canonical form is stable: a second parse gives the same bytes.
- `hune-source`: the member `ref` of `log_comment` for the three kinds of source (8.10, 8a.11, 8b.11), after `panel`.
- `hune-grafana`: one test for each row of 15.6 for several queries, in the form v1 and in the form v2. Separate tests cover a hidden first target, a refId that breaks the grammar, two targets with one refId and 9 targets. Others cover a Mixed panel with a target of each kind of source, and a target of `-- Dashboard --` in a Mixed panel. The last ones cover a `repeat` with two targets and the matcher `byFrameRefID`. The 19 tests are in `crates/hune-grafana/tests/queries.rs`. One of them imports the 6 fixtures of 15.14c.
- `hune` (the server): 14 tests in `crates/hune/src/server/tests/queries.rs`.
  - A save with `extra_queries` gives the canonical bytes and the checks of 7.18 for each query. The pointer of a fault is the SQL line of the extra query. A source that the configuration does not declare gives `bad_reference` at its `source`.
  - A panel has 3 queries. A request with `ref` for each query gives 3 answers. Each answer has its `ref`, its own source and its own SQL (with `server.query_inspect`). A request with no `ref` runs the first query, and its answer has no `ref`.
  - A request with a `ref` that no query has gives `404`. A request for a hidden query gives `400`, with and without `ref`. The source gets no byte in both cases.
  - The panel members of `data.query` apply to a query of `extra_queries`. These are `min_interval` and the time shift. The expanded SQL of the second query holds the interval of the first.
  - A hostile free value for each kind of source gives `422 variable_value_refused` for the second query and no query at the source. These are the properties P, PG and PM.
  - A panel with a query on `prod` and a query on `conf` runs each on its own source. The admission of each source counts its own query. The key `max_concurrent_per_subject` bounds the queries of one subject on one source, not the panel.
  - 8 requests of one panel at the same time, with a small `max_concurrent_per_subject`, give permits, queue and `503 source_busy` as 8.8 says. A failed query does not change the answer of the others.
  - A preview with `extra_queries` gives `400`. The preview of a query that the editor sends as `data.query` works.
  - The log line `query.run` and the comment `log_comment` have the `ref` of the request, and no `ref` in the comment for a request with none. The field `ref` of the log line is `null` then. The route table (6.9) has no new route.
- `tools`: the converter `tools/parity/to_grafana.py` writes `targets` for each query, `hide`, `refId`, the Mixed data source and the matcher `byFrameRefID`. Its tests are the class `MultiQuery` in `tests/tools/test_to_grafana.py`, and the test `test_several_queries` of the class `RoundTrip`. The importer reads the output back to the same members. The test `tests/tools/parity_refs.test.mjs` checks how the capture pairs the queries by ref (15.14c).
- Fuzz: the seed corpus of `document_parse` holds a document with each form of `extra_queries`, `ref` and `hide`. The seed corpus of `grafana_import` holds the 6 fixtures of 15.14c.

The tools of the text panel: the converter `tools/parity/to_grafana.py` writes a text panel. Its 6 tests are the class `TextPanel` in `tests/tools/test_to_grafana.py`, and the importer reads the output back to the same members.

The seed corpus of `document_parse` holds a document with a text panel for each boundary of 4.33a. It also holds a content with hostile text. The seed corpus of `grafana_import` holds a text panel in each mode. The parser of the Markdown is page code, so it has no target of `fuzz/`. 16.10a gives its seeded tests and its tool.

The seed corpus of `document_parse` holds a document with each type and each boundary of 4.33b. The seed corpus of `grafana_import` holds the 8 dashboards of 15.14c.

The seed corpus of `document_parse` holds a document with each type and each boundary of 4.33c and 4.33d. The seed corpus of `grafana_import` holds the 8 fixtures of 15.14c for the heatmap and the histogram.

The tools of the repeat: the converter `tools/parity/to_grafana.py` writes `repeat`, `repeatDirection` and `maxPerRow` of a panel and `repeat` of a row (the v1 form). Its tests are the class `Repeat` in `tests/tools/test_to_grafana.py`, and the importer reads the output back to the same members. The seed corpus of the fuzz target `document_parse` holds a document with each form of `repeat`.

The lexer vectors come from ClickHouse Dash: `tests/frontend/model/sql-lex-vectors.json` at the commit of the chart engine
copy. The repository holds a copy at `tests/fixtures/sql-lex-vectors.json`. One line of `tests/images.lock` holds its
SHA-256 and the ClickHouse Dash commit of 7.5, and the policy check of 16.18 compares them.
A text marked `"differs": true` uses the answer of the ClickHouse Dash backend lexer `src/sql_lexer.hpp`, because Hune
follows the backend rules.

The closed or free value of a variable (F41) adds these tests to the groups of the table:

- `hune-core`: the member `allow_custom_value` (4.8, 4.9) and the values of 7.7.
  - The valid fixture `tests/fixtures/dashboards/open_variables.json` has a golden canonical form. It has an open and a closed variable of each kind. The member comes after `default` and before `placement`. A `false` is not in the canonical bytes. A document with the member `false` has the same etag as one with no member.
  - 10 invalid fixtures `tests/fixtures/dashboards/invalid/f41_*.json` have one error each. They cover the member on a `text` variable (`unknown_member`), a duplicate member, and a text, a number and a null for the member (`wrong_type`). They cover an open `custom` default of 1,025 bytes (`out_of_range`), an empty one (`out_of_range`) and one with a control character (`bad_format`). They cover a closed `custom` default that is not an option (`not_permitted`), with the member absent and with `false`.
  - The differential test for a `query` variable runs the engine for ClickHouse, PostgreSQL and MongoDB. The routes 6.20, 6.21 as a parent, 6.21a and the preview share this engine. It runs each case with the member `true`, with `false`, and with the variable replaced by a `text` variable. The expansion is the same byte for byte, or the error is the same. The API suite runs the four routes (A96).
  - The tests of a closed `custom` variable run with the member absent and with `false`. They send a value outside the options, and a list with one such item. They send the text of an option in place of its value, and variants with another case, a blank and another Unicode form. Each gives `422 variable_value_refused` and no source query, on the four routes.
  - The tests of an open `custom` variable send a hostile set (quote marks, backslashes, comment marks, `$` and Unicode spaces). Each value becomes a literal or gives `422`, never raw code. A placeholder in a quoted name gives `not_permitted` at save and `422` at run. A MongoDB value that starts with `$` gives `422` (PM2).
  - The bounds of a typed value for an open `custom` variable: 0 bytes (`422`), 1 byte, 1,024 bytes, 1,025 bytes (`422`) and a control character (`422`). The list "All" of an open `custom` variable stays author text.
  - The time test: 10,000 values against 10,000 options give an answer in 250 ms or less. The test fails with the linear `contains`.
  - The questions to the PDP are the same with and without the member (11.21).
- `hune-grafana`: one test for each new row of 15.6, in the form v1 and in the forms v2. The cases are the member `true`, `false` and absent, for a `query`, a `custom` and a `text` variable. They cover a `custom` variable in a quoted name, in code and with a `$` option on MongoDB. The golden files of `tests/golden/import/` change as 15.6 says. The five lines of `tests/fixtures/grafana/accept.txt` for `allowCustomValue` leave the file, and one line for each absent member of a `custom` variable joins it.
- `hune` (the server): the case "an open `custom` variable in a quoted name" for the save, the restore and `validate_only`. The preview gets a body whose `dashboard_variables` have the member.

The variable kinds `interval` and `constant` (F27) add these tests to the groups of the table:

- `hune-core`: the kinds (4.10a, 4.10b), the set check of 7.7 and the text of "Auto" (7.4a).
  - The valid fixture `tests/fixtures/dashboards/variable_kinds.json` has two `interval` variables and two `constant` variables. It holds the bounds of 4.10a and 4.10b. They are 1 item and 50 items, the longest text of an item and `auto_count` 1 and 1,000. They are also a `constant` of 0 and of 1,024 bytes. It has an `interval` with a `placement`. The tests set each text of 7.4a as `auto_min`.
  - 30 invalid fixtures `tests/fixtures/dashboards/invalid/f27_*.json` have one error each, with the code and the pointer of 4.34.
  - They cover an item with `1M`, `1.5h`, `0m`, `-1m`, a space, an empty text and 366 days. They also cover 0 items, 51 items and a repeated item.
  - They cover `auto_count` 0 and 1,001, and an `auto_min` that is not a table text. They cover a `default` that is not an option, and `$__auto` with `auto` = `false`.
  - They cover a `constant` with `hide` = `none`, with a `placement`, with 1,025 bytes and with a control character. A member of another kind gives `unknown_member`. A `repeat` that names these kinds gives `not_permitted`.
  - The function of 7.4a gives the 30 texts for the 59 bound ranges of the measure. It also gives the worked examples of 7.4a and the extremes of the range.
  - A test checks the set check of the `interval` values with 10,000 values and 50 items. A hostile set of 40 texts gives `422 variable_value_refused` and no expansion. The case variants `1M` and `1H` and the text `$__auto` with `auto` = `false` give the same error.
  - A `constant` takes the document value with no request member. The equal member gives the same result. Each different member, `all: true`, no value and two values give `422`. A value with `$x` or `$__from` stays text.
  - The seeded random tests of 16.4 for these kinds are in `crates/hune-core/tests/f27_interval_constant.rs`, with 32 seeds of 200 cases. Each case has an `interval`, a `constant`, a `text` and a `query` variable. P holds for the `query` and `text` variables.
- `hune-grafana`: one test for each row of 15.6 for the two kinds, in the form v1 and in the forms v2. The cases are the list `1m,1.5h,1M`, an empty list, 60 items, each `auto_min` rule and each `current` rule. They also cover a constant with `hide` 0, 1 and 2, and a value of 1,025 bytes.
- `hune` (the server): the tests of 16.8, A103 to A110, in `crates/hune/src/server/tests/f27_interval_constant.rs`. The fake source records each query, so the tests check that a refusal runs no query.
- Converter: `tests/tools/test_to_grafana.py` converts the fixture to `tests/fixtures/grafana/variable_kinds.json`. Its round trip through the importer gives the same document, with no item, apart from the placement.
- Fuzz: the seed corpus of `document_parse` holds the fixture and the invalid files `f27_*.json`, with the bounds of the items. The seed corpus of `grafana_import` holds the lists and the values of the table of 15.6. No new target.

The eleven transforms (F28) add these tests to the groups of the table:

- `hune-core`: the kinds (4.25a), in `crates/hune-core/tests/document_f28_more_transforms.rs`.
  - The valid fixture `tests/fixtures/dashboards/transforms_more.json` has one transform of each of the eleven kinds. It holds the bounds of 4.25a: the arrays at 0 and at the bound, the longest names, and `count` 0 and 1,000,000.
  - 45 invalid fixtures `tests/fixtures/dashboards/invalid/f28_*.json` have one error each, with the code and the pointer of 4.34. One file has two errors: a `calculate_field` with the object of another mode gives `missing_member` and `unknown_member`.
  - They cover a member that is absent, a member of another kind, a wrong type and a value out of range. They also cover a duplicate in each array and a bad pattern.
  - They cover a `calculate_field` with two objects of the mode, or the wrong one, and an operand that has both members. A `limit` has -1 or 1,000,001. A `sort_by` has 9 keys. A `reduce` has 19 reducers.
  - They cover a `partition_by_values` with an empty `fields` or 5 fields. A `convert_field_type` has 17 conversions or a `to` that does not exist.
  - The round trip of the canonical form (4.38) holds for all valid files. A document that has no new kind keeps its bytes.
- `hune-grafana`: one test for each row of 15.6 for the eleven ids, in `crates/hune-grafana/tests/more_transforms.rs`, in the form v1 and in the forms v2. The cases come from the measure of 19.20j. The goldens of the import (`tests/golden/import/`) hold the mapped `organize` of the fixtures.
- `hune` (the server): the tests of 16.8, A100 to A102, in `crates/hune/src/server/tests/f28_more_transforms.rs`.
- Converter: `tests/tools/test_to_grafana.py` converts the fixture to the Grafana ids (`transforms_out`). Its round trip through the importer gives the same document.
- Fuzz: the seed corpus of `document_parse` holds the fixture and the invalid files `f28_*.json`, with the bounds, hostile names and patterns. The seed corpus of `grafana_import` holds a dashboard with each id and each option of 15.6.

The logs panel (F33) adds these tests to the groups of the table:

- `hune-core`: `crates/hune-core/tests/document_logs.rs` has 9 tests (4.33f). The empty options and the defaults stay out of the canonical form, and the other values keep the order of the table. A bad option gives one error at its member. The format `logs` in another type gives `not_permitted`, also in an extra query. A logs panel has no extra query, and its reuse source has one query.
- `hune-grafana`: `crates/hune-grafana/tests/logs.rs` has 7 tests in the forms v1, v2alpha1 and v2beta1 (I40).
- Front end: FE-81 in `tests/frontend/logs_panel.test.mjs` (16.10a).
- Browser: the cases B220 and B221.

The map (F37) adds these tests to the groups of the table:

- `hune-core`: `crates/hune-core/tests/document_geomap.rs` has 8 tests (4.33g). They cover the canonical order, each range, the closed options with no tile source, the rule of the sizes and the frame of one query.
- `hune`: `crates/hune/src/config/tests/geomap.rs` has 7 tests (13.12c). They cover each refusal of the shared list FE-85, the attribution, the zoom, the closed block, the derived line of `print-config` and W9. `crates/hune/src/server/tests/geomap.rs` has 3 tests: the header of 12.8 byte for byte with no block, the header from the parts, and the session member (6.10).
- `hune-grafana`: `crates/hune-grafana/tests/geomap.rs` has 7 tests (I42).
- Front end: FE-85 in `tests/frontend/geomap.test.mjs` and `tests/frontend/geomap_frame.test.mjs` (16.10a). The second file drives the real panel frame through refreshes: a failed tile is asked one time in a view.
- Browser: the cases B226 and B227.

The card view of the list page (9.7) adds these tests to the groups of the table:

- `hune`: `crates/hune/src/config/tests/list_filters.rs` tests the key `server.list_filters_min_dashboards` (13.8). It checks the default, the bounds, the refusal of `_env` and `_file`, and the line of `print-config`. `crates/hune/src/server/tests/core.rs` checks that the session gives the number (6.10).
- Front end: `tests/frontend/list_page.test.mjs` tests the tag filter, the short age, the threshold, the rights of a card and the structure of the card. `tests/frontend/theme_k.test.mjs` merges `look.css` and checks 4.5:1 for the 29 tag colours in both themes and 3:1 for the logo.
- Browser: the case B228. The cases B01, B34, B50 to B52 and B91 follow the new page.

The JSON view of the edit mode (10.4) adds these tests to the groups of the table:

- Front end: `tests/frontend/app_shell.test.mjs` tests the layout of 9.6. It checks the body, the container, the header and the sticky row. It also checks the measures of `main.js` and the scroll step of a drag.
- Front end: `tests/frontend/json_area.test.mjs` tests the colour of each kind of token, a text that is not valid and the cap of 2,000 parts for a row. It tests the line diff for the marks of the gutter. The cases are an added, a modified and a deleted line. Other cases are the two ends of the text, a long text and the compare by place above the cap. It also reads the CSS: the colour tokens, the metrics of the two layers and the rule of the height.
- Browser: the cases B240, B270, B271 and B272.

The stores in the page (9.6, 9.7, 9.8) add these tests to the groups of the table:

- Front end: `tests/frontend/stores.test.mjs` tests the groups by store, the marks of a card and the items of the selector. It tests the texts and the message of `read_only_store` too.
- Browser: the cases B241 to B244. The case B01 and the entry case B33 use a dashboard that becomes not valid after the start.

The dashboards with no folder (9.6, 9.7, 10.7, 10.12, 10.13) add these tests to the groups of the table:

- Front end: `tests/frontend/root_dashboards.test.mjs` tests the value of the root entry, the right of the root and the entries of the folder select. It tests the root groups with their order and their title rule. It tests the items of the selector, the conflict of a move to the root, and the bodies with the folder `null`.
- Browser: the cases B300 to B305. The harness adds a file at the root of a volume with `addRootDashboard` (options `prepare` and `labPrepare`).

The select and copy rules (9.8e) add these tests to the groups of the table:

- Front end: `tests/frontend/copy_text.test.mjs` tests the drag threshold and the place of a text: alignment, baseline, pixel ratio and turn. It tests the order of the texts, the signature and the cap of 400 texts. It tests the wrapper of `fillText` and `clearRect` on a fake context. It tests the text of a copy.
- Front end: `tests/frontend/row_copy.test.mjs` tests the text of a copy of rows on a fake selection. The cases are whole rows, a partial cell, one cell and hidden cells. It tests the cases that use the copy of the browser, the marks of the logs panel and the cursor rules.
- Browser: the cases B260 to B266. They use real mouse drags and clicks. A spy on the event `copy` reads the copied text. Chrome and Firefox run them.

The default range and the default refresh (10.9) add these tests to the groups of the table:

- Front end: `tests/frontend/time_defaults.test.mjs` tests what differs and what the saved document gets. It tests the refusal of a range that the settings form refuses. It also tests the Save dialog, the button "Save" and the end of a view.
- Browser: the cases B231 and B232.

The ad hoc filters (F36) add these tests to the groups of the table:

- `hune-core`: `crates/hune-core/tests/adhoc_filters.rs` has 10 tests (4.10c, 6.14, 7.7a, 7.10a, 7.18). They cover the fixture `tests/fixtures/dashboards/adhoc.json` and its canonical form, and one refusal for each rule of the kind. They also cover the names of 6.14, the macro and each refusal of its place.
- `hune-core`, seeded: a test runs hostile values of 64 seeds through the macro. Each value stays in one string literal and decodes to itself, and only a control character is refused. A test shows that the identifier comes from the memory of the configured list, never from the request key. The module `sql/adhoc.rs` has 7 unit tests.
- `hune`: `crates/hune/src/config/tests/adhoc.rs` has 4 tests (13.12, 13.13): the absent key, the list in `print-config` and the start log, each refusal of a list, and the reference.
- `hune`, API: `crates/hune/src/server/tests/adhoc_filters.rs` has 4 tests on the fake source. The first checks the filters in the SQL, the member `adhoc` of 6.14 and the log line. The second sends bad filters: an unknown key, an unknown operator, 17 filters, a control character and a long value. Each gives `422 variable_value_refused` with no source query.
- `hune`, API: the third test shows that a change of the configured list fails closed. The fourth runs the checks at save.
- `hune-grafana`: `crates/hune-grafana/tests/adhoc_import.rs` has 5 tests (I39).
- ClickHouse suite: `crates/hune-source/tests/adhoc_clickhouse.rs` (16.9).
- Front end: FE-80 in `tests/frontend/adhoc.test.mjs` (16.10a).
- Browser: the cases B218 and B219.

### 16.4 Seeded random tests of the lexer and the substitution

The generator is a dev-dependency only. It uses a seeded pseudo-random generator, so a seed gives the same cases on each
platform. The tests are in `crates/hune-core/tests/random.rs`, so the environment check of 16.18 does not apply to them.

The generator makes SQL templates from these parts:

- keywords, plain identifiers, and identifiers in backquotes and in double quotes, with escapes;
- numbers, and string literals with `\'`, `''` and `\\`;
- line comments (`--` and `#`) and nested block comments;
- heredoc text (`$tag$ ... $tag$`);
- the macros of section 7.

The generator puts variable references in code, in string literals, in comments, in identifiers and in heredoc text. The
references are `$v`, `${v}` and each format form of section 7.

The generator makes values from an alphabet that favours dangerous characters: `'`, `"`, `` ` ``, `\`, `$`, `{`, `}`, `-`,
`/`, `*`, `#`, `;`, line ends, tabulators, NUL, U+2028 and characters outside the Basic Multilingual Plane. A value has 0 to
1024 bytes. A multi-value list has 0 to 50 values.

| Property | Statement |
|---|---|
| R1 | A value never leaves its literal. The lexer reads the output again. At each place of a reference, the output holds one string literal (or a list of literals and commas for several values). The decoded literal equals the value, byte for byte. |
| R2 | The tokens outside the places of references are equal to the tokens of the template: same kinds and same text. |
| R3 | A value that the rules of section 7 refuse gives an error and no SQL: free text in an identifier, NUL, a raw value outside its closed list. |
| R4 | A reference in a comment stays as text. |
| R5 | The same inputs give the same bytes. |
| R6 | The output passes the statement check of section 7 when the template passes it, and fails when the template fails. |
| R7 | The lexer never panics. The concatenation of the token texts equals the input. |
| R8 | A format of the last group of 7.9 never puts free text outside its literal. In a string, the output holds one string literal at the place of the reference, and the decoded literal equals the text of the format with the value. In code and in a quoted name, the same reference gives an error, at save and at run. |
| R9 | `$__conditionalAll`: with "All", the output holds `1=1` and none of the first argument. Else property R1 holds for each free value of the first argument. |

The properties R1 to R9 run for each dialect. The generator of the dialect `postgres` adds these pieces: `E` strings, dollar quotes, `U&` forms, a backslash before a quote mark, `#`, Unicode spaces and runs of operator bytes. For this dialect, R3 also puts a free value in an `E` string, in a prefixed string and in a dollar quote. R6 uses the statement check of 7.25.

The kind `mongodb` has its own generator and the property R10 in place of R1 to R9. The generator makes query trees from the stages and the operators of 7.31. It puts whole-leaf and embedded placeholders at all value places, in keys and in name places.

The values are hostile: `$`, `$$ROOT`, `$secret`, `.`, `{"$out": "x"}`, a JSON text, quote marks, NUL, U+2028 and long texts. The property R10 has three parts:

- The expansion gives an error. Else it gives a tree T1 with the keys, the stage names and the operator names of T0 (the property PM of 7.35).
- Each leaf that holds a free value does not start with `$`.
- A placeholder in a key or in a name place always gives an error.

The same inputs give the same bytes. The parser of 7.28 never panics, and it gives the same tree for the same text.

Each seeded test runs two times for a `custom` variable: once closed and once open, with the same values. For a `query` variable it runs two times: with `allow_custom_value` true and false. The property P, PG or PM holds in each run, and the output of the two runs of a `query` variable is the same.

The `rust` job runs the seeds 1 to 64 with 5,000 cases each. The `nightly` job runs 1,000 seeds with 20,000 cases each, and
the seeds come from the date. A failure prints the seed and the smallest failed case that the shrink step finds. The
command below runs one seed again:

```sh
docker run --rm -v "$PWD:/src" -w /src -e HUNE_TEST_SEED=4242 hune-builder \
  cargo test -p hune-core --locked random_
```

The ClickHouse case C03 (16.9) adds a check against the real ClickHouse lexer: ClickHouse returns each value of a
sample, unchanged.

### 16.5 Configuration tests

| Case | Check |
|---|---|
| K01 | For each key of the table of section 13: the minimum and the maximum are accepted. A value below or above stops the start. The error names the key, the expected type or range, and the value. |
| K02 | A wrong type stops the start, with no coercion: `"8080"` for an integer, `1.0` for an integer, `"true"` for a boolean. |
| K03 | An unknown block, an unknown attribute, a duplicate attribute and a duplicate block label each stop the start. |
| K04 | `<name>`, `<name>_env` and `<name>_file`: exactly one form is accepted. Two forms stop the start. A variable that does not exist, an empty value, or a file that the program cannot read stop the start, and the error names the attribute and the variable or the path. A `_file` value loses its last line end only. |
| K05 | A secret in clear text in the file stops the start. `print-config` and the start log hide each secret. |
| K06 | Each contradiction of section 13 stops the start. The error names both keys and both values. |
| K07 | A key of the mode `authzen` with the mode `open` stops the start. A necessary key that is absent stops the start. |
| K08 | An expression, a function call, an interpolation `"${x}"` and a heredoc each stop the start. |
| K09 | A duration with a fraction, a sign, two units or no unit stops the start. |
| K10 | `check-config` on a file with 5 errors prints 5 errors and exits 1. On a good file it prints nothing and exits 0. |
| K11 | `print-config` prints the value and the source (`file`, `env:VAR`, `file:/path`) of each key and exits 0. It runs in a container with `--network none`, so it contacts no network. |
| K12 | The start log line of section 14 holds the same values as `print-config`. |
| K13 | The output of `hune print-reference --format markdown` equals the committed `docs/configuration-reference.md` (13.21). |
| K14 | These variables, set to bad values, change nothing: `RUST_LOG`, `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SDK_DISABLED`, `OTEL_SERVICE_NAME`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY`, `SSL_CERT_FILE`, `SSL_CERT_DIR`. The test compares the output of `print-config` and the behaviour of a panel query. It is the test `environment_has_no_effect` of 13.24, and it runs in the compose project of 16.8 (job `api`). It has three parts: K14a in the mode `open` (from M2), K14b in the mode `authzen` with `fake-pdp` (from M6), K14c with `telemetry.enabled = true` (from M7, a build with the OTLP exporter). |
| K15 | The output of `print-reference` passes `check-config` when its secret files exist: the `local` output in every build, the `distributed` output from M7. A build before M7 gives the error `telemetry.enabled: this build has no OTLP exporter` on the `distributed` output, and the test checks this error text. |
| K16 | The block `postgres "<id>"`: each key at its minimum and its maximum; each form of 13.5 for `user` and `password`; a secret in clear text; the contradictions C14 to C20; the warning W7; the output of `print-config` for the example of 13.12a; the tables of 13.12a in the reference document. |
| K17 | The key `server.query_inspect` (13.8): 5 tests in `crates/hune/src/config/tests/inspect.rs`. An absent key is `false` with the source `default`. The key in the file gives its value with the source `file`. The key is in the key table, in the reference and in the order of `print-config`. A wrong form or value stops the start. The contradiction C21 stops the start. One test in `crates/hune/tests/config_cli.rs` checks the line of `print-config` with the sources `default` and `file`. |
| K18 | The block `mongodb "<id>"`: each key at its minimum and its maximum; each list at 1 item and at its maximum, and one more item; each form of 13.5 for `user` and `password`; a secret in clear text; each mode of `topology`, with the keys that the mode forbids; the contradictions C14, C15 and C22 to C27; the warning W8; a `host` with `://`, `@` or `mongodb+srv`; the output of `print-config` for the example of 13.12b; the tables of 13.12b in the reference document. |
| K19 | The keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` (13.8): 8 tests in `crates/hune/src/config/tests/repeat.rs`. An absent key has its default with the source `default`. A key in the file gives its value with the source `file`. The keys are in the key table, in the reference and in the order of `print-config`. A value below 1 or above its bound (200, 50 and 200), a wrong type, a form `_env` or `_file`, a duplicate key and an unknown key in the block stop the start, and the error names the key, the range and the value. The contradictions C28 and C29 stop the start, also when the key that is more than the dashboard limit is absent. The member `repeat_limits` of `GET /api/session` (6.10) holds the three values: 2 tests in `crates/hune/src/server/tests/core.rs`. The reference document equals the output of `print-reference`. |
| K20 | The key `server.compress_static` (13.8, 9.3): 4 tests in `crates/hune/src/config/tests/static_gzip.rs`. An absent key is `true` with the source `default`, and a key in the file has the source `file`. A text, an integer, a form `_env` or `_file`, a duplicate key and an unknown near name stop the start, and the error names the key, the type and the value. The answers of the static files: 7 tests in `crates/hune/src/server/tests/static_gzip.rs`. The encoder: 17 tests in `crates/hune/src/gzip/`, with round trips through `gzip -d` and Node.js. The parse of `Accept-Encoding`: 3 tests in `crates/hune/src/server/encoding.rs`. |
| K21 | The key `server.transform_max_cells` (13.8, 4.35): 8 tests in `crates/hune/src/config/tests/transform_cells.rs`. An absent key is 10,000,000 with the source `default`. The values 1, 2,500,000 and 100,000,000 have the source `file`. The values 0 and 100,000,001, a wrong type, `_env` or `_file`, a duplicate and a near name stop the start, and the error names the key, the range and the value. No environment variable gives the value. The member `transform_max_cells` of `GET /api/session` (6.10) has 2 tests in `crates/hune/src/server/tests/core.rs`. |
| K22 | The key `server.list_filters_min_dashboards` (13.8, 9.7): 6 tests in `crates/hune/src/config/tests/list_filters.rs`. An absent key is 6 with the source `default`. The values 0, 1, 25 and 1000 have the source `file`. The values 1001 and 4294967296, a wrong type, `_env` or `_file`, a duplicate and a near name stop the start, and the error names the key, the range and the value. No environment variable gives the value. The member `list_filters_min_dashboards` of `GET /api/session` (6.10) has 2 tests in `crates/hune/src/server/tests/core.rs`. |
| K23 | The stores of 13.10: 14 tests in `crates/hune/src/config/tests/stores.rs`. One test for each of C2, C10 and C30 to C34, the labels, the limit of 16 volumes, the removed keys (`unknown attribute`), a file with no primary store, and a read-only PostgreSQL store next to a primary volume. |

### 16.6 Store conformance suite

The store conformance suite is the suite of 5.33 to 5.36. Section 5 owns its cases: the common tests of 5.34 (SC-), the
volume tests of 5.35 (SV-) and the PostgreSQL tests of 5.36 (SP-). Its rules are those of 5.33.

The `store` job (16.17) runs the common tests on a volume in a temporary directory, and from M5 on PostgreSQL 15 and 18
in containers. It runs the volume tests on a tmpfs, with a tmpfs of 1 MiB for the test of a full disk. It runs the
PostgreSQL tests with a TCP proxy in front of PostgreSQL for the tests that break a connection. Each PostgreSQL database of
the job has the two roles of 5.25: `migrate-store` uses the owner role, and the suite uses the application role.

### 16.7 Cases added to the store suite

| Case | Backend | Check |
|---|---|---|
| SX-01 | volume | An `fsync` error (fault injection in the test build): the save fails with `Unavailable`, the temporary file is removed, and the backend calls `fsync` no second time (5.13). |
| SX-02 | stores | `Stores` (5.42 to 5.44): 14 tests in `crates/hune-store/tests/stores.rs`. The first load and the merge, the refusal of each write into a read-only store, the ids that two stores cannot share, each start rule, the primary store with no dashboard, the last valid state of a file that becomes not valid, a file that is new and not valid, the period of each store, and a write in the primary store that the catalog sees at once. |

### 16.8 API black-box suite

The suite is the integration test `api` of the crate `hune`. It sends HTTP requests to the binary on the compose network
and reads nothing inside the process. The compose project holds these services:

- the binary of the build;
- a ClickHouse of the pinned LTS version, with seed data. At its first start it runs `tests/fixtures/clickhouse/init.sql`.
  This file is the SQL of 8.12 that creates the profile and the user `hune`. It names the test database `analytics`, not
  the production database. The test configuration names this user, with `password_file`;
- PostgreSQL 18, with the two roles of 5.25 (from M5, 16.17);
- a second PostgreSQL 18 database `analytics` for a `postgres` source, with the role `hune_ro` of 8a.13 (from the milestone PG). It runs `tests/fixtures/postgres-source/init.sql` (16.9a);
- `fake-pdp` of the pinned ProcurAuth commit.

The cases A01 to A38 run four times: volume in mode `open`, PostgreSQL in mode `open`, and the same two with
no primary store. The last two runs check the cases that name the primary store. The runs on PostgreSQL start at
M5. The cases A39 to A45 run on the volume in mode `authzen` with `fake-pdp`, from M6. Their configuration is
`tests/api/authzen.hcl`: the audience `hune-test`, the sources `prod` and `conf`, and `cache_ttl = "30s"`.

| Case | Check |
|---|---|
| A01 | `GET /api/dashboards` lists each valid document, and each invalid document with its errors. |
| A02 | `GET /api/dashboards/{uid}?download=true` gives the canonical bytes, whose SHA-256 is the etag. The plain `GET /api/dashboards/{uid}` gives the answer of 6.14 with the member `document`. Both have the etag in the `ETag` header. |
| A03 | `GET` of an unknown uid in mode `open`: 404. |
| A04 | `POST /api/dashboards` creates the document in its folder. A uid that exists: 409. |
| A05 | `PUT /api/dashboards/{uid}` with the current `If-Match` and a new `Idempotency-Key`: a new etag and one history version. |
| A06 | `PUT` without `If-Match`: refused, no change. |
| A07 | `PUT` with an old `If-Match`: 409 with the current etag, no change. |
| A08 | `PUT` repeated with the same `Idempotency-Key` and body: the same answer, one version. |
| A09 | `PUT` with a used `Idempotency-Key` and another body: refused, no change. |
| A10 | `PUT` of a document whose `uid` differs from the path: refused, with the JSON pointer `/uid`. |
| A11 | `PUT` of a document with three errors: the answer lists the three errors with their JSON pointers. |
| A12 | `PUT` of a document with a SQL text that starts with another keyword than `SELECT` or `WITH`, or with two statements: refused, with the pointer of the SQL. |
| A13 | `DELETE /api/dashboards/{uid}`: a tombstone. The next `GET` gives 404. The history stays. |
| A14 | `GET /api/dashboards/{uid}/history` gives the versions, newest first. `GET .../history/{n}` gives the bytes of version n. |
| A15 | `POST /api/dashboards/{uid}/restore`: a new version with the bytes of the chosen version. |
| A16 | `POST /api/dashboards/{uid}/move`: same uid, new folder. A folder that does not exist: refused. |
| A17 | `GET` and `POST /api/folders`, `PUT` and `DELETE /api/folders/{id}`: create, title change, delete of an empty folder. A folder with documents: refused. |
| A18 | `POST /api/dashboards/{uid}/panels/{id}/query` with the current etag, an absolute range, `max_data_points` and variable values: a frame of section 7 with the expected columns and kinds. |
| A19 | The same query with an old etag: 409. |
| A20 | A query with `from >= to`, with `max_data_points` out of range, or for an unknown panel: refused. A query for a panel of kind reuse: refused. |
| A21 | A variable value that section 7 refuses: refused, and `system.query_log` of the test ClickHouse shows no new query. |
| A22 | `POST /api/dashboards/{uid}/variables/{name}/options`: the options. A variable that depends on another variable gets its options with the value of the other. |
| A23 | `GET /api/sources`: ids and labels, and no URL, user or secret. |
| A24 | `POST /api/sources/{id}/preview` runs draft SQL. With no primary store: refused. |
| A25 | `GET /api/session`: the mode, the subject (`-` in mode `open`) and the editing flag. |
| A26 | With no primary store, each write route is refused and each read route works. A write into a read-only store gives `403 read_only_store` (11.35). |
| A27 | Write guard: `Sec-Fetch-Site: cross-site` is refused. No `Sec-Fetch-Site` and an `Origin` of another host is refused. A body that is not `application/json` is refused. An `OPTIONS` request gets no CORS header. |
| A28 | A body above `server.max_request_bytes` is refused before the server reads all of it. A header block above 16 KiB is refused. A connection above `server.max_connections` is refused or waits, as section 14 says. |
| A29 | Each class of answer (shell, static file, API, error) has the security headers of section 12, with the exact CSP text. |
| A30 | `/static/{build-hash}/...` has the immutable cache header. Another build hash gives 404. The shell has `no-cache`. |
| A31 | With `server.base_path = "/dashboards/"`, each route answers under the prefix, and the shell holds relative links only. |
| A32 | Mode `open`: `/.well-known/procurauth/actions` and `/.well-known/procurauth/resources` give 404. |
| A33 | Ops listener: `/livez` gives 200. `/readyz` gives 503 before the first load of the catalog and 200 after it. `/metrics` gives the text format with the prefix `hune_`. These paths give 404 on the main listener. The API paths give 404 on the ops listener. |
| A34 | After the first load, stop PostgreSQL and ClickHouse: `/readyz` stays 200, `GET /api/dashboards/{uid}` gives 200 from the catalog, and a save fails with a retryable error. |
| A35 | SIGTERM during a slow query: `/readyz` gives 503 at once. During `SHUTDOWN_DRAIN` (5 s), new connections are served as usual (6.28, 14.13). After it, new connections are refused, and a request on an open connection gets `503 server_stopping`. The process exits with 0 within `server.shutdown_grace` plus `SHUTDOWN_DRAIN`. When this time ends first, the exit code is 1. |
| A36 | Each log line on the standard output is one JSON object with `event`. No line holds a SQL text, a secret or a token: the test searches the whole log for the secret values and for a marker in the SQL. |
| A37 | No metric label holds a uid, a subject or a SQL text: the test compares the label values with the fixture uids. |
| A38 | The variables of K14, set to bad values, change no answer of the suite. |
| A39 | Mode `authzen`: a request without a token gets 401 on each route, except `/.well-known/procurauth/actions`. |
| A40 | An unknown uid: the PDP answers `false` for `dashboard.read` on the service, so the answer is 403. The PDP answers `true`, so the answer is 404. |
| A41 | The PDP does not answer: 503 `pdp_unavailable`, retryable. A kept answer that is younger than `cache_ttl` still applies. |
| A42 | The ceilings `max_rows` and `max_duration` of the answer lower the settings of the query. `system.query_log` shows the lower `max_result_rows` and `max_execution_time` for the `query_id`. |
| A43 | A save asks `source.query` on each source of the new document, with no usage. A `false` answer refuses the save. |
| A44 | The list shows only the folders and dashboards that get `true` for `dashboard.read` with `subtree`. |
| A45 | `GET /.well-known/procurauth/actions` gives a JSON value equal to the golden file `tests/golden/api/action-document.json`: the document of 11.18 for `tests/api/authzen.hcl`. The test compares JSON values, not bytes. |
| A46 | Bodies in flight (6.27, 12.16): while `MAX_INFLIGHT_DOC_BODIES` (8) large bodies are in flight, one more create or save gets `503 server_busy`. While `MAX_INFLIGHT_SMALL_BODIES` (256) small bodies are in flight, one more small request with a body gets `503 server_busy`. When the bodies end, the next request succeeds. The test holds the bodies open with slow uploads. |
| A47 | The milestone PG. A panel query on a `postgres` source gives a frame with the expected columns and kinds (7.16a). A dashboard with one panel on each kind of source works. |
| A48 | `GET /api/sources` lists the `postgres` source with `kind` `postgres` and its state, and no host, user or secret. |
| A49 | A save, a restore and a preview with SQL for a `postgres` source that breaks 7.25 (a `DELETE`, two statements, `pg_sleep`, `$1`, `FOR UPDATE`) are refused, with the pointer of the SQL. The same text on a `clickhouse` source follows 7.12. |
| A50 | Mode `authzen`: the ceilings `max_rows` and `max_duration` lower the limits of a PostgreSQL query. A result above the ceiling gives `403 ceiling_exceeded`. |
| A51 | The log holds no SQL text and no value of a PostgreSQL query. `query.run` has `source_kind` `postgres` and the field `sqlstate`. The action document holds the `postgres` source in `resources`. |
| A52 | The milestone MMG. A panel query on a `mongodb` source gives a frame with the expected columns and kinds (7.16c). A dashboard with one panel on each of the three kinds of source works. |
| A53 | `GET /api/sources` lists the `mongodb` source with `kind` `mongodb` and its state, and no host, user, database, collection or secret. |
| A54 | A save, a restore and a preview with a query for a `mongodb` source that breaks 7.28 to 7.33 are refused, with the pointer of the line (7.36). The set holds `$out`, `$merge`, `$where`, `$function`, a `$lookup` outside the list, a duplicate key, JavaScript text, and a placeholder in a key. The same text on a `clickhouse` source follows 7.12. |
| A55 | Mode `authzen`: the ceilings `max_rows` and `max_duration` lower the limits of a MongoDB query. A result above the ceiling gives `403 ceiling_exceeded`. |
| A56 | The log holds no query text and no value of a MongoDB query. `query.run` has `source_kind` `mongodb` and the field `mongo_code`. The action document holds the `mongodb` source in `resources`. |
| A60 | The parameter `tag` of 6.12. One dashboard has the tag `chain` and one has not. `tag=chain` lists the first. `tag=other` lists none. A tag of 65 bytes, an empty `tag` and two `tag` give `400 bad_request`. `tag` with `q` and `folder` keeps the dashboards that pass all. Mode `authzen`: a dashboard that the subject cannot see is not in the list, with or without `tag`. |
| A61 | The stores (5.42 to 5.44, 6.10 to 6.14, 11.35). 15 tests in `crates/hune/src/server/tests/stores.rs`. The session lists the stores. The list, the folders and the read give `read_only`, `store` and `stale`. Each write into a read-only store gives `403 read_only_store` and changes nothing, in the mode `open` and in the mode `authzen`. Hune never asks `dashboard.write` for it. A subject with no right gets the same bytes for a target of a read-only store and for one that does not exist. Each start rule stops the start with the label, the file and the reason. A file that becomes not valid keeps its last valid state, with the warning, the log line and the gauge. |
| A70 | Several queries (6.20, 4.15a). A panel with 3 queries answers 3 requests with `ref`, one for each query, each with its own `query_id`. A request with no `ref` runs the first query. A `ref` that no query has gives `404 not_found`. A body with an SQL member, a `source` member or a `ref` of a wrong type gives `400`. |
| A71 | Hidden queries (6.20, T65). A request for a hidden query gives `400 bad_request`, with the `ref` and for a hidden first query with no `ref`. The log has no `query.run` line, and no query reaches the source. A panel with all queries hidden is a valid document. |
| A72 | The save (7.18, 11.21). A save, a restore, an import and a `validate_only` check each query of a panel. A fault in the third query gives the pointer `/sections/0/panels/<n>/data/extra_queries/1/sql/<line>`. 8 queries are valid and 9 give `out_of_range`. A document with 401 queries gives `too_large`. A panel with `repeat` and `extra_queries` gives `not_permitted`. |
| A73 | Mode `authzen`, with `fake-pdp`. A save of a panel whose second query names the source `conf` asks `source.query` on `conf`, also when the query is hidden. A subject with `source.query` on `prod` only gets `403 forbidden`. A viewer with `dashboard.read` runs each query of the panel, and each request asks one question with the same key, so the library makes one call with `cache_ttl = "30s"`. With `cache_ttl = "0s"` each request asks its own question. A subject without `dashboard.read` gets `403` for a hidden ref and for an unknown ref. |
| A74 | Mixed sources. A panel with a ClickHouse query and a PostgreSQL query, and a panel with a ClickHouse query and a MongoDB query, run each query with the rules of its kind (7.19, 7.35). A hostile free value gives `422 variable_value_refused` for the second query, and the source gets no byte (T66). A query with a refused statement gives `422 sql_invalid` and the other queries of the panel answer. A query above `max_query_size` of its own source gives `422 sql_too_large` for that query only. |
| A75 | Limits (8.8). A panel with queries on two sources, with `max_concurrent_per_subject` = 1 and a short `queue_wait`: each source counts its own queries, and the query that waits too long gives `503 source_busy` alone. 8 requests of one panel at the same time, with `max_concurrent_per_subject` = 2, give 2 answers and 6 `503 source_busy`. A refused query never reaches its source. |
| A90 | A closed `custom` variable (the member absent, then `false`): a value outside the options, sent to 6.20, 6.21 (as a parent), 6.21a and 6.22, gives `422 variable_value_refused`. `system.query_log` of the test ClickHouse shows no new query. |
| A91 | The same for a list with one item outside the options, for the text of an option in place of its value, and for variants of case, blank and Unicode form. |
| A92 | An open `custom` variable: a hostile value gives a literal in a string, a number in code, or `422`. The log of the source shows the escaped text. |
| A93 | A save with an open `custom` variable in a quoted name gives `422 invalid_document` with `not_permitted` at the line of the SQL. The same text with a closed variable is saved. |
| A94 | A request for an open `custom` variable with 0 bytes, 1 byte, 1,024 bytes, 1,025 bytes and a control character: `422` for the first, the fourth and the fifth. |
| A95 | An open `custom` variable on a MongoDB source: the value `$secret` gives `422`, and the value `secret` runs. |
| A96 | A `query` variable with the member `true` and with `false`: a value outside the options gives the same answer, byte for byte, on 6.20, 6.21 as a parent, 6.21a and 6.22. |
| A97 | The time of the check of a closed `custom` variable with 10,000 values and 10,000 options is 250 ms or less. |
| A98 | A document with `allow_custom_value` on a `text` variable, with a text as value, and with a number as value: `PUT ...?validate_only=true` gives `unknown_member` and `wrong_type` at `/variables/<i>/allow_custom_value`. |
| A76 | The heatmap and the histogram (4.33c, 4.33d, T69). A save of a document with each type gives `200` and the canonical bytes. The options at their boundaries are valid, and each one out of range by one gives `422 invalid_document` with its pointer. A panel query of each type answers as for a table, and the server never buckets. A result of 100,000 rows of counts passes without a cut. The server of the previous version, which the suite starts as a second binary, refuses a document with each type with `bad_format` at `type`. |
| A103 | A document with the two kinds is saved and read back with the same canonical bytes (4.38). The etag is the same for the same bytes. |
| A104 | Each of 20 invalid variables of 4.10a and 4.10b gives `422 invalid_document` with the code and the pointer of 4.37. |
| A105 | A panel query with an `interval` value that is an item gives 200 on the four routes. The values `$__auto` with `auto` false, an item with a space, `7m` that is not an item, an empty list, 2 values and `all: true` give `422 variable_value_refused`. A hostile set of 40 texts gives 422 and no source query. |
| A106 | A panel query for a `constant`: no member gives 200, the equal member gives 200, a different member and `all: true` give 422, and no source query runs for a refusal. |
| A107 | The text of "Auto" for a range of 6 h, 5 min and 30 d, with `auto_count` 30 and `auto_min` `10s`, is `10m`, `10s` and `1d`. The same text goes to all the routes of the request. |
| A108 | The route of 6.21 with a `query` variable whose SQL names an `interval` and a `constant`. A `$__interval_s` in that SQL keeps its error (7.18, item 5). |
| A109 | The annotation route (6.21a) with a SQL that names a `constant` and an `interval` gives the marks. The test covers it with the four routes of A105 and A106. |
| A110 | The preview (6.22) with `dashboard_variables` that holds the two kinds runs a draft panel and a draft variable. A `constant` is read from the body and not from `variables`. |
| A111 | The import of a Grafana dashboard with an `interval` and a `constant` gives the rows of 15.6. A dashboard with `1M` in the list gives the report row and the saved document has the other items. |
| A140 | The query parameters of ClickHouse (7.6a). A panel query, an options query, an annotation query and a preview with `{name:Type}` give 200. The source receives the SQL with the forms and no value, and the pairs `param_<name>` after `log_comment`, in the byte order of the names. Other values give the same SQL (PQ). The member `names` of 6.14 names the variables of the forms. |
| A141 | The refusals of 7.6a send no byte to the source. A value outside the grammar of its type, 1,001 items and pairs above 4,096 bytes give `422 variable_value_refused`, with a reason that names the type and holds no value. A form that a constant makes in the final text gives `422 sql_invalid`. A save with each fault of 7.18, items 10 to 15, gives `422 invalid_document` with the code and the line. A status 414 of a proxy gives `422 query_error` with `clickhouse_code` = `null`, and the source stays up. |
| A100 | A document with one transform of each of the eleven kinds is saved and read back with the same canonical bytes (4.38). The etag is the same for the same bytes. |
| A101 | Each of 20 invalid transforms of 4.25a gives `422 invalid_document` with the code and the pointer of 4.37 (`unknown_member`, `missing_member`, `wrong_type`, `out_of_range`, `too_large`, `not_unique`, `bad_format`, `not_permitted`). No other error stops the list. |
| A102 | The import of a Grafana dashboard with the 11 ids gives the rows of 15.6: the classes, the fallbacks and the report. The saved document has the new kinds and the same bytes again after a second save. |

### 16.9 ClickHouse integration cases

The `source` job runs these cases on two ClickHouse versions: the version that 19.4 names and the latest LTS release.
Each version has a tag and a digest in `tests/images.lock`. Each test ClickHouse runs `tests/fixtures/clickhouse/init.sql`
at its first start (16.8): the profile and the user of 8.12, with the test database `analytics`.

| Case | Check |
|---|---|
| C01 | Each macro of section 7, expanded and run on a seed table with known times, gives the expected rows. |
| C02 | `$__timeInterval` gives the expected bucket boundaries for each step of the interval table. |
| C03 | For 2,000 random values of each CI run (16.4), `SELECT <value as a substituted literal>` returns the value, unchanged. |
| C04 | A result above `max_result_rows`, a result above `max_result_bytes`, a run above `max_execution_time` and a SQL text above `max_query_size` each give their error code of section 6. A cut result is never shown as complete. |
| C05 | Cancellation: the test starts a panel query that runs 30 s, then closes its client connection. `system.processes` has no row for the `query_id` within 2 s. |
| C06 | The read-only guard of section 8: an account with `readonly = 0`, `1` and `2` each gives the behaviour of section 8. |
| C07 | Each ClickHouse type of section 8 gives its frame kind and value: `UInt64` maximum and `Int128` as exact text, `Decimal(38, 9)` as exact text, `DateTime64(3)` and `Date` as Unix seconds, `Nullable`, `LowCardinality`, `Bool`, NaN and infinities. |
| C08 | `system.query_log` holds the `query_id` and the `log_comment` of section 8 for each query. |
| C09 | Account contract: each `CHECK GRANT` line and the `SHOW GRANTS` line of 8.12 give the value that 8.12 writes beside them, with the test database `analytics` in place of the production database. |
| C10 | TLS with the test CA succeeds. A wrong `tls.server_name` stops the connection. No system CA store is read. |
| C11 | A ClickHouse that is down: the panel query fails with a retryable error, `/readyz` stays 200, and the source state is down. |
| C12 | A `SETTINGS` clause in the SQL gives an error of ClickHouse, and the query does not run, in each of these cases: a value above a constraint of the profile; `result_overflow_mode = 'break'`; `timeout_overflow_mode = 'break'`; `read_overflow_mode = 'break'` (the `CONST` constraints of 8.12). |
| C13 | After 15 s without a query, the next query succeeds on the first attempt: the client never sends on a connection that ClickHouse closed (its `keep_alive_timeout` is 10 s). |
| C14 | Two queries of one panel. The test sends two requests with `ref`. `system.query_log` holds two rows with two `query_id`, and the `log_comment` of each has its `ref` (8.10). A client that closes the connection of the first request leaves the second query running, and the first query leaves `system.processes` within 2 s. |
| C20 | An open `custom` variable with the value `x' OR 1=1 --` in a string: ClickHouse receives one query with the escaped literal, and the result has no row. |
| C21 | A closed `custom` variable with a value outside the options: no query reaches ClickHouse. The options query of a `query` variable keeps 10,000 rows (a result of 10,001 rows gives `422 query_limit`) with the member `true` and with `false`. |
| C22 | A `query` variable with a value outside its options runs as free text: ClickHouse receives the query with the literal, and the answer is the same as for a `text` variable. |
| C15 | The queries of the heatmap and of the histogram (9.19e, 9.19f). Three panel queries run on the seed table: the buckets of `intDiv(latency, 10) * 10` with `count()`, the columns of `histogram(10)(latency)` taken apart with `tupleElement` as `xMin`, `xMax` and `count`, and a wide frame of `countIf` for each bucket. The columns `count()` and `UInt64` have the kind `number_text`, and the page reads their double. The test saves the three frames as the goldens of FE-44 and FE-46. A version that has no `histogram` function skips that query with a message. |
| C30 | `parseTimeDelta('${bucket}')` with an `interval` value `1m`, `1d`, `1w`, `1y` and `500ms` in a string runs on the server and gives 60, 86,400, 604,800, 31,536,000 and 0.5. |
| C31 | The text of "Auto" in a string runs for 6 ranges, and the statement returns the table text of 7.4a. |
| C32 | A `constant` with the hostile text `a'b"c$d\e;--x` runs in a string (escaped, one row) and in a quoted name (escaped). In code the text stays as it is, and the final SQL check refuses it because it opens a string. The result has no second statement. |
| C33 | An `interval` request with a hostile value gives `422` and no query on the source. |
| C34 | A `constant` request with a different value gives `422` and no query on the source. |
| C35 | The seeded random tests of 16.4 with an `interval` and a `constant` in the document: each hostile value of a `query` or `text` variable still gives an error or keeps P. |
| C40 | The query parameters of 7.6a. For each type, `SELECT` of the parameter gives back each value of a hostile set: quote marks, backslashes, `]`, `,`, Unicode, the escapes of ClickHouse as text, and texts of 1,024 bytes. The escapes of TAB, line feed, carriage return and zero give back the character. The values that the grammars refuse show the measure of 26.8: code 457 or 130 for a zero before the first digit, and a changed value for an integer out of range and for a date out of range. No answer has the code 456 or 457. `a${t:Identifier}` gives a column with the name of the value, so the second scan of 7.6a reads a `{` right after `$`. |
| C41 | After a query with a query parameter, `system.query_log` holds the statement with the value in place of the form, as for the text form of 7.6. |

The cases C40 and C41 are ignored tests in the module `f39` of `crates/hune-source/tests/clickhouse.rs`. The job `source` runs them with the other C cases. The cases A140 and A141 are tests of `crates/hune/src/server/tests/query_params.rs` with the fake source. The unit tests of 7.6a are in `crates/hune-core/src/sql/params.rs` and `crates/hune-core/tests/f39_query_params.rs`. They hold each loose form, the grammar of each type, the caps, the second scan and the name scan. They also check the property PQ with seeded random values.

The ad hoc filters (F36) have the ignored test `adhoc_a_string_literal_compares_with_each_column_type` in `crates/hune-source/tests/adhoc_clickhouse.rs`. The job `source` runs it with `tests/ci/source.sh`. The test makes a table with the types `Int32`, `UInt64`, `Float64`, `Date`, `DateTime`, `Decimal(18, 4)`, `Enum8`, `LowCardinality(String)`, `Nullable(Int32)` and `Nullable(String)`. It runs `$__adhocFilters` with one filter for 27 cases, hostile values included, and compares the count of rows. It also records the code of ClickHouse for a text that does not convert (7.10a).

The cases C30, C31, C32 and C35 are ignored tests in the module `f27` of `crates/hune-source/tests/clickhouse.rs`. The job `source` runs them with the other C cases. C30 also runs the 30 texts of the table of 7.4a. C31 uses the ranges 1 s, 5 min, 1 h, 6 h, 24 h and 7 d with `auto_count` 30 and `auto_min` `10s`. C32 also sends a constant that tries to end its statement, and checks that the seed table keeps its 30 rows.

C35 runs 4 seeds of 500 cases. Set `HUNE_TEST_SEED` to run one seed again.

The cases C33 and C34 are ignored tests `real_c33` and `real_c34` in `crates/hune/src/server/tests/real_ch.rs`. The script `tests/ci/server.sh` runs them. Each test sends a good request first, and reads the SQL text of Hune in `system.query_log`. It counts the queries of Hune in the log before and after the refused requests. The two counts must be equal.

### 16.9a PostgreSQL source integration cases

The `pgsource` job runs these cases on PostgreSQL 15 and 18. Each version has a tag and a digest in `tests/images.lock`. Each test server runs in a disposable container on `127.0.0.1`, with synthetic data only. At its first start it runs `tests/fixtures/postgres-source/init.sql`. This file holds these items:

- the owner role, the database `analytics` and the schema `metrics`;
- tables with one column of each type of 7.16a;
- a table of 1,000,000 rows made with `generate_series`, and a table with a column of 50 MB;
- the role `hune_ro` of 8a.13, with its revokes;
- thirteen deviant roles for the checks R1 to R9. One is a member of a role that can write, without INHERIT (R4). One owns a table (R9). Three can run `pg_terminate_backend`, `pg_logical_emit_message` or `ts_stat` (R7). PS11 uses the roles for R1, R2, R3, R4, R7, R8 and R9;
  - one has `CREATEROLE`, one is a member of `pg_read_all_data`, and one has `CREATE` on a schema;
  - one has a write grant on a table, one has `UPDATE` on a sequence, and one has `INSERT` on one column;
  - one has `EXECUTE` on the functions of `dblink`, and one has `USAGE` on a foreign server;
- the role `hune_rw`, which has all rights, for PS07;
- the roles `hune_md5` and `hune_clear`, which have the contract of `hune_ro`. Their login methods are `md5` and `password`. PS12 uses them.

The file `tests/fixtures/postgres-source/apply.sh` runs `init.sql` at the first start. It gives the passwords to psql as variables, so `init.sql` holds no password.

The cases use three clients. The first is the binary of the build. The second is the client of the crate `hune-source`, called directly. It runs no statement check, because the check is in `hune-core`, so it sends any text. The third is a plain `tokio-postgres` client with no startup parameter of 8a.4.

A proxy on `127.0.0.1` sits between a client and the server in the cases that name it. It records each message of the protocol. In PS07 the test sets the state of the source to `up` by hand, because the probe refuses the role `hune_rw`.

The name of each test starts with its case id in lower case, for example `ps04_row_limit_is_exact`. The script `tests/ci/pgsource.sh` reports each case PS01 to PS15 as PASS, FAIL or NO TEST. It gives the connection to the tests in variables with the prefix `HUNE_TEST_PGS_`. The program of Hune reads none of them. A run with no network sets `HUNE_PGS_NO_PULL=1`, and the script then uses the local images `postgres:15` and `postgres:18`. The script `tests/fixtures/postgres-source/demo.sh` makes a demo database from synthetic data, and it prints a `postgres "demo"` block for the configuration.

| Case | Check |
|---|---|
| PS01 | Each macro of 7.22, expanded and run on a seed table with known times, gives the expected rows. The vectors of 16.13 give the expected texts. `$__timeGroupAlias(ts, $__interval)` and `$__timeFilter` are in the set. |
| PS02 | `$__timeGroup` gives the expected bucket boundaries for each step of the interval table of 7.4 and for each unit of 7.23. |
| PS03 | For 2,000 random values of each run (16.4), `SELECT <value as a substituted literal>` returns the value, unchanged. The set holds a standard string, an `E` string, a quoted name and a list of literals. The values hold quote marks, backslashes, comment marks, `$` and Unicode spaces. Each session has `standard_conforming_strings = on`. |
| PS04 | Limits. A result of exactly `max_result_rows` rows passes, and one more row gives `422 query_limit`. A result above `max_result_bytes` gives its code. A query above `max_execution_time` gives `504 query_timeout`, for one Execute message and for a query of many Execute messages. A SQL text above `max_query_size` gives `422 sql_too_large`. A result with a row of 50 MB and `max_result_bytes` of 1 MiB gives `422 query_limit`, and the client reads no second row. A cut result is never shown as complete. |
| PS05 | Cancellation. The test starts a query that runs 30 s, then closes its client connection. `pg_stat_activity` has no row with its `application_name` within 2 s. The same holds at the client deadline. When the proxy drops the cancel request, the row leaves within 3 s, because of `client_connection_check_interval`. |
| PS06 | Each type of 7.16a gives its kind and its value: the `int8` maximum, `numeric(38,9)`, `numeric` NaN as `null`, `float4` and `float8` with NaN and infinities, `timestamptz` with microseconds, `timestamp`, `date`, `infinity`, `jsonb`, arrays with NULL elements, an enum, a domain, `inet`. A column of each unsupported type gives `422 query_error` with `sqlstate` `null`, and the proxy shows no Execute message. |
| PS07 | The connection side alone. The client of `hune-source` connects with the role `hune_rw`. Each text of `INSERT`, `UPDATE`, `DELETE`, `CREATE TABLE`, `SELECT ... INTO` and a data-changing CTE fails with the SQLSTATE 25006. After `SELECT set_config('default_transaction_read_only', 'off', false)` and after `SET TRANSACTION READ WRITE`, a write still fails with 25006. The next query on the pool shows `default_transaction_read_only = on`. |
| PS08 | The role side alone. The plain client logs in as `hune_ro` and opens `BEGIN READ WRITE`. Each statement of 8a.13 fails with the SQLSTATE that 8a.13 gives. |
| PS09 | One statement. The text `SELECT 1; SELECT 2` sent to the client of `hune-source` gives the server error 42601. Through the binary it gives `422 sql_invalid`. The proxy shows no message `Query` ('Q') whose text is not a constant of 8a.5 or 8a.12. |
| PS10 | Each row of the table of 7.25 gives its result at save and at run. Each refused function gives `sql_invalid`. A refused name inside a comment or a string gives no refusal. |
| PS11 | The probe. The role `hune_ro` gives `up`. Each deviant role gives `misconfigured`, and the log line `source.misconfigured` names its check (R1, R2, R3, R4, R7, R8). The advice A1 to A4 each write one `source.advice` line for one change. |
| PS12 | TLS. The test CA with channel binding succeeds. A wrong `tls.server_name`, a certificate from another CA, and a server with the method `md5` or `password` each stop the connection. `sslmode = "disable"` works on the loopback interface. No system CA store is read. |
| PS13 | A stopped PostgreSQL: the panel query fails with a retryable `source_unavailable`, `/readyz` stays 200, and the state is `down`. After the restart, the state is `up`. |
| PS14 | The pool. After 70 s without a query, the next query succeeds on the first attempt. A connection that the admin session ends before `BEGIN READ ONLY` gives one retry and a success. A connection that it ends after `BEGIN READ ONLY` gives `503 source_unavailable`, and the proxy shows no second run. |
| PS15 | Attribution. `pg_stat_activity` holds the `application_name` and the comment of 8a.11 with the `query_id` and the uid. For 1,000 random subjects, the comment never closes before its end. |
| PS16 | A `custom` variable on the source. An open variable with the typed value `x' OR 1=1 --` in a string gives the escaped literal, and the result has no row. A closed variable refuses a value outside the options with `422 variable_value_refused`, and no query reaches the source. |
| PS17 | Annotations on the source, through the server (7.37). A query of `node_load` with `$__time`, `$__timeFilter`, a region of 30 s, a `text[]` of tags and the variable `env` gives the marks in the order of the time. The log line has the kind `annotation`, and `pg_stat_activity` is not read. The same times come from an `int8` of epoch milliseconds. 1,000 rows give 1,000 marks, and 1,001 rows give `422 query_limit`. An empty result gives no mark. A text with an HTML tag gives `422 annotation_invalid` with no cell value. A seconds column of `$__timeEpoch` gives marks in January 1970 (7.37, the table of the kinds). |

### 16.9b MongoDB source integration cases

The `mongosource` job runs these cases on MongoDB 7.0 and on the newest 8.x release. Each version has a tag and a digest in `tests/images.lock`. Each test server runs in a disposable container on `127.0.0.1`, with synthetic data only. The server runs with authorization on, TLS only and `--noscripting`.

A second server runs without `--noscripting` for the case MS18. A third server is a replica set of two members in one container for the case MS12 and the case MS09. The key `hosts` needs 2 to 7 members. Each port is a free port of `127.0.0.1`, taken at the start of the job (16.17a).

At its first start, each server runs `tests/fixtures/mongo-source/init.js`. This file holds these items:

- the database `analytics`. Its collections are `node_load`, `node_events`, `types`, `big` and `secret`. `node_load` holds 1,000,000 documents made by a loop. `types` has one field of each BSON type. `big` holds one document of 10 MiB. The key `collections` does not list `secret`;
- a second database `other` with one collection;
- the user `hune_ro` of 8b.13, with the role `read` on `analytics`;
- deviant users, one for each check:
  - a user with `readWrite`, one with `readAnyDatabase` and one with `clusterMonitor` (M2);
  - one with a custom role that holds `insert`, and one with a custom role on `anyResource` (M3);
  - one that lives in a database other than the key `auth_database` (M1). Its login fails first, so the probe never reaches the check M1;
- the user `hune_rw`, which has `readWrite` on `analytics`, for MS09;
- the users `hune_sha1` and `hune_plain`, which have the contract of `hune_ro`. Their mechanisms are SCRAM-SHA-1 and PLAIN. MS10 uses them.

The file `tests/fixtures/mongo-source/apply.sh` runs `init.js` with `mongosh`. It gives the passwords to `mongosh` as variables, so `init.js` holds no password.

The cases use three clients. The first is the binary of the build. The second is the client of the crate `hune-source`, called directly. It runs no grammar check, because the check is in `hune-core`, so it sends any pipeline that the wire guard accepts. The third is a plain driver client of the test, with no option of 8b.4, and `mongosh`.

A proxy on `127.0.0.1` sits between a client and the server in the cases that name it. It ends the TLS on both sides with the test certificate, and it records the name of each command. It can close the connections, delay the answers, and close a connection after a given command. In MS09 the test uses the client of the crate `hune-source` with the user `hune_rw`, because the probe refuses this user.

The name of each test starts with its case id in lower case, for example `ms04_row_limit_is_exact`. The script `tests/ci/mongosource.sh` reports each case MS01 to MS18 as PASS, FAIL or SKIP, and it prints a summary for each version. It gives the connection to the tests in variables with the prefix `HUNE_TEST_MGS_`. The program of Hune reads none of them. A run with no network sets `HUNE_MGS_NO_PULL=1`, and the script then uses the local images `mongo:7.0` and `mongo:8.0`.

The script `tests/fixtures/mongo-source/demo.sh` starts a disposable demo server, makes a demo database from synthetic data, and prints a `mongodb "mgdemo"` block for the configuration. The options `--dev` and `--down` of `tests/ci/mongosource.sh` start the servers of one version with no test, and remove them.

The cases of the first part need no driver and no server. They run with the fake client of 8b.16: MS02 (the grammar part), MS03 (the expansion part), MS06 (the golden frames), MS11 (the check functions) and MS07 (the refusal part). They are in the job `rust`, and a build with no `mongo-driver` feature runs them.

Each of these five cases has a second part that runs on a real server. The cases that need a real server are in the job `mongosource`. The tests of MS13, MS16 and MS17 start a server of the crate `hune` in the process of the test, with the real driver.

| Case | Check |
|---|---|
| MS01 | Each macro of 7.34, expanded and run on a seed collection with known times, gives the expected documents. `$__timeFrom` and `$__timeTo` give Date values with the millisecond of `from_ms` and `to_ms`. `$__interval_s` is the `binSize` of `$dateTrunc` for each step of the interval table of 7.4. |
| MS02 | Each stage and each operator of 7.31 runs on the seed data and gives the frame of its golden file in `tests/golden/pipeline-mongo/`. The golden files are Extended JSON. The part with no server checks that each name of 7.31 passes the grammar and that each other name fails it. |
| MS03 | For 2,000 random values of each run (16.4), a query that holds the value in a whole-leaf placeholder and in an embedded placeholder returns the value, unchanged, as a document. The values hold `$`, `$$`, `.`, quote marks, backslashes, `{`, `}`, U+2028 and long texts. A value that starts with `$` is refused (7.35, PM2). No query reads a field that the value names. |
| MS04 | Limits. A result of exactly `max_result_rows` documents passes, and one more gives `422 query_limit`. A result above `max_result_bytes` gives its code. A pipeline above `max_execution_time` gives `504 query_timeout`. A command above `max_query_size` gives `422 sql_too_large`. A `$group` over `node_load` with a large `$push` and `allowDiskUse: false` gives `422 query_memory_limit`. A document of 10 MiB with `max_result_bytes` of 1 MiB gives `422 query_limit`, and the client reads no second document. A cut result is never shown as complete. |
| MS05 | Cancellation. The test starts a query that runs 30 s and has a cursor, then closes its client connection. `currentOp` shows no operation with the `comment` of the query within 2 s. The same holds at the client deadline. A query that has no cursor yet leaves `currentOp` within `maxTimeMS` plus 2 s. The proxy shows one `killCursors`. |
| MS06 | Each type of 7.16c gives its kind and its value: the Int64 maximum, a Decimal128 with a scale, `NaN` and the infinities, a Date with milliseconds and before 1970, an ObjectId, a UUID, a nested document, an array, a missing field. A column that mixes Int32 and Double passes, and a column that mixes Int32 and String gives `422 query_error` with `mongo_code` `null`. A column of each unsupported type gives the same error. A result with no document gives a frame with no column. A document with 257 fields gives the error of the column limit. |
| MS07 | Each row of 7.32 and each name that no table of 7.31 lists gives `422 sql_invalid` at run and `422 invalid_document` at save. The proxy shows no `aggregate` or `find` for any of them. A refused name inside a string value gives no refusal. A key that starts with `$` and is not an operator gives the refusal. |
| MS08 | The user side alone. `mongosh` logs in as `hune_ro`. Each statement of 8b.13 fails with the code that 8b.13 gives. A read of the collection `secret` works, because the role reads it, and Hune refuses it by the list (7.33). |
| MS09 | The connection side alone. The client of `hune-source` connects with the user `hune_rw`. A request with `$out`, with `$merge`, with `$function` and with `$where` at the end of a pipeline, at the first level and in a `$facet`, is refused by the wire guard, and the proxy shows no command for it. The proxy shows only the commands of 8b.5 for a run of all the cases of this table. With `read_preference = "secondary"` on the replica set, a pipeline with `$out` or `$merge` that the test sends with a plain client as `hune_ro` fails with the code 13. A secondary forwards the write of the user `hune_rw`, so this user is not in the test. No call of a write function exists in the code (16.18). |
| MS10 | The link. The test CA succeeds. A certificate with another host name, a certificate from another CA, and a server that offers only SCRAM-SHA-1 or PLAIN each stop the connection. `tls_mode = "disable"` works on the loopback interface. The tests show that no SRV or TXT lookup and no system CA store exist, because the lock file shows no crate for them in the closure of the driver. A test with a TLS server that has a certificate of another CA fails, and the right CA passes the handshake. A CA file that changed after the start is refused. The environment variables of 13.24 change nothing: the test runs again with them set. |
| MS11 | The probe. The user `hune_ro` gives `up`. Each deviant user gives `misconfigured`, and the log line `source.misconfigured` names its check (M1, M2 or M3). The advice A1 and A2 each write one `source.advice` line for one change. A version below 7.0 writes `source.version_unsupported`. The part with no server runs the check functions on the structures of 8b.16. |
| MS12 | The replica set. A block with the topology `replica_set` and the list of the two members gives `up`. A list that lacks a member, or a set name that differs, gives `misconfigured` with the check M4. The driver gets no set name, so the reason is M4 and not a timeout. |
| MS13 | A stopped MongoDB: the panel query fails with a retryable `source_unavailable`, `/readyz` stays 200, and the state is `down`. After the restart, the state is `up`. |
| MS14 | The pool. After 70 s without a query, the next query succeeds on the first attempt. A connection that the admin session closes before the command goes out gives one retry and a success. A connection that the proxy closes after the command went out gives `503 source_unavailable`, and the proxy shows no second command. |
| MS15 | Attribution. `currentOp` and the profiler hold the `comment` of 8b.11 with the `query_id` and the uid. For 1,000 random subjects, the comment fits 1,024 bytes. |
| MS16 | An options query (7.14) and a preview run on a MongoDB source. With `server.query_inspect = true` the inspect text is the compact Extended JSON of the expanded tree and holds no member that Hune adds. |
| MS17 | The golden pipelines of `tests/golden/pipeline-mongo/` give the same result at save and at run, on the same tree. The saved SHA-256 of the expanded command is the value in the log line `query.run`. |
| MS18 | The server without `--noscripting`. A plain client runs `$where` and the test shows that the server permits it. Hune refuses the same text by the grammar, and the proxy shows no command. This case records the known limit of 8b.15. |
| MS19 | A `custom` variable on the source. An open variable with a typed value gives the value as a leaf, and a value that starts with `$` gives `422 variable_value_refused`. A closed variable refuses a value outside the options, and no command reaches the source. |
| MS20 | Annotations on the source, through the server with the real driver (7.37). A pipeline on `node_events` with `$__timeFrom`, `$__timeTo`, a `$match` on `level` and a `$project` of `time`, `timeEnd` (`$add` of 30 s), `title`, `text` and the array `tags` gives the marks in the order of the time. The same times come from an Int64 of epoch milliseconds (`$toLong`). 1,000 rows give 1,000 marks, and 1,001 rows give `422 query_limit`. An empty result has no column and gives no mark. A text with an HTML tag gives `422 annotation_invalid` with no cell value. |

The test file `crates/hune-source/tests/mongosource_hello.rs` makes sure that the `hello` of the driver holds no value of the environment (8b.15). It is not a case MS.

A test cannot set a variable in its own process, so a parent test starts the test binary again with the variables set. It sets `AWS_EXECUTION_ENV`, `AWS_REGION`, `K_SERVICE`, `VERCEL`, `FUNCTIONS_WORKER_RUNTIME` and `KUBERNETES_SERVICE_HOST`, together and in groups. A stub server records the `hello`, or the proxy of this section does so with a real server. The test fails when the key `client.env` is present, or when the name or the value of a variable is in the message. The tests with the stub need no server. The test with the proxy runs in the job `mongosource`.

### 16.10 Browser tests

The tests use Playwright in the test image `tests/browser/Dockerfile`. npm runs only inside this image. The
image uses a committed `package-lock.json`. The tests run against the release build of the binary in compose, with
ClickHouse and seed data. All cases run in Chromium. The cases B01, B02, B03, B07, B08, B15, B16, B18, B103, B112, B122, B141, B163 and B172 also run in Firefox.

The runner drives Firefox with WebDriver BiDi: set `HUNE_BROWSER=firefox` (`tests/browser/lib/bidi.mjs`). Chrome is the default and its client is not changed.

WebKit has no driver on the test host. The library `libwebkit2gtk-4.1` and its `MiniBrowser` are present, but no `WebKitWebDriver`. The runner cannot drive WebKit, so all WebKit checks are manual (16.19).

The cases B170 and B171 depend on the engine of the text area, and Firefox cannot run them as they are. WebDriver BiDi presses the modifier keys as real keys, so the page also sees the keydown of `Control` and `Shift`. It cannot send a key with a `code` that its character does not give (the layout check of B170). It has no IME command (B171). These two cases are a manual check in Firefox and WebKit before 1.0.0 (16.19).

Firefox has no media emulation in WebDriver BiDi. On Firefox the transport sets `prefers-color-scheme` through the theme control of 9.25 (the key `hune.theme` and the event `hune-theme`). The cases test both themes, but not the media query. A case that needs the print media or `prefers-reduced-motion` is not supported on Firefox.

The case B153 has its own limit of 900 s. Its states and its hover probe add about 170 probes to the 317 control changes that it made before F43.

| Case | Check |
|---|---|
| B01 | The list page shows one card for each dashboard, and no folder title for one folder. A card is an `article` with a title link and the tags. A dashboard of a volume has no badge of the last change. A file that becomes not valid after the start shows as a valid card with the warning mark, and the API gives `stale` with no path (9.7). The tag filter keeps the cards with a tag, and the URL holds `q`. The page has no search field (9.7). |
| B02 | The grid: each panel box has the position and size of its grid numbers (rows of 30 px, gaps of 8 px). |
| B03 | Each of the 7 panel types renders its fixture: table, timeseries (lines and bars), stat, barchart, piechart, xychart, bargauge. |
| B04 | Table overrides by name and by regular expression, each with each property of the property set (4.18, 9.14). The regular expressions use the form of 4.17, with the fixture pattern `/^Total$/` (19.29). A stat panel whose `reduce.fields` is `/^Total$/` shows the field `Total`. |
| B05 | Table sort in three states, column filter, and virtual rows: with 10,000 rows the page holds a bounded number of row elements. |
| B06 | Tables show `UInt64` maximum and `Decimal(38, 9)` values as exact text. |
| B07 | Time picker: relative and absolute ranges, a field with an error, and zoom out. The URL holds `from` and `to` as epoch milliseconds or relative text. The refresh menu and its hidden list give the same choices, and the current value is checked. The refresh stops while the tab is hidden. |
| B08 | Variables: query variable with multi, All and search; text box applied on Enter; custom list; dependent refresh; refresh on a time change; `var-<name>` repeated in the URL and `$__all`. |
| B09 | A panel outside the view and a panel in a collapsed section send no query until shown. At most 6 queries are open at once. A click on "Refresh" cancels the older queries and runs them again. |
| B10 | Panel reuse: the panel shows the frame of its source, with and without the source transformations. No extra query. |
| B11 | `filterByValue`, `groupingToMatrix` and the long-to-wide step give the golden output. Series names follow the display-name rule of 9.13. |
| B12 | `?viewPanel=<id>` shows one panel, and the other panels send no query. `Escape` closes the view. The page reads `viewPanel=panel-<id>` and writes the number (9.5). |
| B13 | Light and dark themes. The choice stays after a new load. The system preference applies first, and a change of it applies at once. The theme button is one icon button that gives System, Dark and Light in turn, with no menu, at each width (9.25). Each chart draws again after a change of theme. |
| B14 | axe-core with the WCAG 2.2 AA rules gives no violation in both themes: on the list and dashboard pages from M3b, and also on the editor, history and conflict pages from M4. |
| B15 | No `securitypolicyviolation` event and no console error on any page of the suite. |
| B16 | Editor: the SQL tab and "Run" of the draft; the options pane of the panel; the JSON tab lists each error with its JSON pointer (a fixture with 3 errors gives 3 lines). |
| B17 | Editor: add, duplicate and delete a panel; add, rename, collapse, move and delete a section; grid numbers; move up and move down. |
| B18 | Save: the request has `If-Match` and `Idempotency-Key`. After a network failure that the test injects, the retry has the same `Idempotency-Key`. The draft stays in `localStorage` after a new load until the save succeeds. |
| B19 | Conflict: a second browser context saves first. The first context gets the conflict screen with a line diff and the three choices of section 10. Each choice gives its result. |
| B20 | History: list, read-only view with the note "Data not shown" in each panel and one "Show data" button for the page, diff, restore as a new version. |
| B21 | Export and import of one document. Delete, then restore. |
| B22 | With no primary store, no edit control is in the DOM. The cards of the read-only store have the lock and no three dots, and the dashboard has the lock in the header (9.7, 9.8). |
| B23 | Mode `authzen` with `fake-pdp`: no edit control without `dashboard.write`, no "Run" of a draft without `source.query`, and the notice of 9.6 when the PDP is down. |
| B24 | A panel whose source is down shows its error. The other panels render. |
| B25 | Data links follow 9.18: only `http`, `https` and relative URLs; each value is URL-encoded. A link with `target_blank` = `true` opens a new tab with `rel="noopener noreferrer"`. A link with `false` opens in the same tab. |
| B26 | The application works under `server.base_path = "/dashboards/"`. |
| B27 | The request of a panel query has the `max_data_points` of 9.8: for a fixture panel of known size, the value equals the width that 9.8 names (19.31). |
| B28 | No dashboard: the list page shows "No dashboard yet." and the URL stays the root. |
| B29 | One dashboard: the entry page goes to it with `replaceState`, and the guard key is written. "Back" does not show the list. The brand link shows the list with its row, with no redirect. "Back" and "Forward" in the page show the list again. |
| B30 | One dashboard in a folder, and one empty folder: the entry page goes to the dashboard. |
| B31 | Two dashboards: the list shows, and no guard key is written. |
| B32 | Access lost: a `403` on the dashboard after the redirect shows its page; "Go to the list" shows the list with no redirect. A full reload within 10 seconds shows the list. After a delete, a reload shows the empty state. |
| B33 | One dashboard whose file is not valid now: the entry redirect opens it with its last valid state, and the page shows the warning (9.7, 9.8). |
| B34 | `q=chain` and `q=zzz` show the list, and an empty `q` redirects. With the filters hidden, `q` does not change the cards and it still stops the redirect (9.7). |
| B35 | Mode `authzen` with `fake-pdp`: a subject with one visible dashboard goes to it; a subject with none sees "No dashboard is visible to you."; a subject with more sees the list. |
| B36 | Stat: `justify` `auto` centres an item with no name and aligns left an item with a name. `center` centres both (9.19). The fixture holds a stat with one item and a stat with three items. |
| B37 | Hidden series: an override with the matcher `names` and `hide_from` (9.14). A series with `viz` is not drawn, and the axis ignores it. `legend` removes the legend entry. `tooltip` removes the tooltip row. A legend click changes the page only: the document and its etag stay the same (9.19). |
| B38 | Thresholds in the plot: each value of `thresholds_style` in a `timeseries` and in a `barchart`, with the mode `absolute` and with `percentage`. The test reads the lines and the bands through the hook of 9.29, and checks the count, the positions and the colours (9.19). |
| B39 | Zone: a dashboard with `timezone = "utc"` runs in a Chromium with the zone `Asia/Tokyo` and in one with the zone `America/New_York`. The x axis labels, the tooltip head and the cells of a `time` field are equal in both, and equal to UTC (9.19). |
| B40 | Table: the footer (one reducer and two reducers, with a filter that changes the result), `wrap`, the cell types `gauge` (three modes) and `json`. A `json` cell with the value `<img src=x onerror=alert(1)>` shows the text and runs no script (9.20, 9.1). |
| B41 | CSV: the button "Download CSV" in "View data" gives a file with the rules of 9.11: the header, the quote rule, the `'` before the text cells `=cmd`, `+1`, `-1`, `@x` and a tab, no `'` before a number such as `-1` or `-5.2%` in a number field, a `'` before a mapping text such as `-5 =cmd` in a number field, the file name, and no `securitypolicyviolation` event. |
| B42 | Titles with variables: the panel title and the section title follow the selection. A value `<b>x</b>` shows as text. A name that is not a variable stays as written (9.13). |
| B43 | Drag and resize: the case opens edit mode and drags a panel by its title bar. It checks the placeholder, the `grid` members in the draft and the push of the panel that gives way. It resizes a panel by its corner. It cancels a drag with Escape and checks that the draft does not exist. It moves a panel with Alt and an arrow key. It saves, and checks that the saved document has the new `grid` and nothing else changed. |
| B44 | A dashboard with a table and a time series on a `postgres` source, run in a disposable PostgreSQL 18. `GET /api/sources` gives the kind of each source (6.22). The table shows exact texts: an `int8` above 2^52 and the `int8` maximum, a `numeric(38,9)` with its scale, and a `timestamptz` with a date of 2026 (7.16a). The time series `$__timeGroupAlias(created_at, $__interval)` draws one series for each of three hosts (7.16b). In the editor, the source picker shows the kind of each source. A SQL text that calls `pg_sleep(1)` on its line 2 gives `422` from the preview, and the error box names line 2. |
| B45 | The scanner (9.10): the panel `SELECT 6 # ${n}::int` on a `postgres` source reads the text variable `n` after the operator `#`. The value 3 gives 5. A new value 1 gives no query before Enter, and then one panel query that shows 7. |
| B46 | Inspect (9.11). The case starts two servers. With `server.query_inspect = true`, the session says `query_inspect: true`, the panel menu has `panel-<id>-inspect`, and its dialog `inspect-<id>` shows the expanded SQL in `inspect-<id>-sql` with the button `inspect-copy`. With `false`, the item is absent and the answer of a query has no member `sql`. |
| B47 | A dashboard with a table and a time series on a `mongodb` source, run in a disposable MongoDB 8. `GET /api/sources` gives the kind of each source (6.22). The table shows exact texts: an Int64 above 2^52 and the Int64 maximum, a Decimal128 with its scale, and a Date of 2026 (7.16c). The time series with `$dateTrunc` and `$__interval_s` draws one series for each of three hosts (7.16d). In the editor, the source picker shows the kind of each source. A query with `$out` on its line 3 gives `422` from the preview, and the error box names line 3. |
| B48 | A MongoDB panel whose query finds no document shows the state `empty`, with no column. A nested document shows as a JSON text in its cell. The format `time_series` with no Date column shows the error of 9.22. The scanner (9.10) finds the variable `n` in the string `"${n:number}"` of the query, and a new value of `n` runs one panel query. |
| B49 | The theme button shows System, Dark or Light with its icon, its label and its tooltip. One click gives the next choice in the order System, Dark, Light and System again. The button has no menu and no popup, before and after a click. The choice stays after a reload, and a dark choice does not follow the system. The key Enter gives the next choice, and the focus stays on the button (9.25). |
| B50 | The theme button has the same size and place on the list, a dashboard, the edit mode, the panel editor, the pages "not found" and "dashboard not found", and the page "Hune does not answer." A choice on each page changes the theme (9.25). |
| B51 | Header of the list and of a dashboard (9.6, 9.7, 9.9). An editor finds the sort control, the tag filter and one button "New" of the list in the header at 1280 px, in one row of 44 px. The sort control and the tag filter follow the brand, and the button "New" is at the right. The menu of the button has "New dashboard", "New folder" and "Import", at every width. The three actions are not buttons of the header. At 768 and 390 px the filters and the button "New" with its menu are in the header. The controls of a dashboard are in the header at 1920, 1280, 768 and 390 px, with their names kept and no horizontal scroll. The menu "More actions" has "History", "Export" and "Delete". Up to 900 px it also has the time shift and the zoom out. |
| B52 | A viewer (editing off) has the filters of the list and the buttons of a dashboard in the header at 1280 and 390 px, with no "Edit", no action of the list, and "Export" only in "More actions". |
| B53 | The version line has the form `version:checksum` (9 hexadecimal characters). It is beside the product name on the same line, equals the version of GET api/session, and has a title with the full value and its source. It is the same on the list, a dashboard and the pages of an error, and it shows on the page "Hune does not answer.". At 1280 px or less it hides for the eye and stays for assistive technology, and the title of the brand gives the value (9.6). |
| B54 | The edit mode (9.9, 10.2, 10.3). In edit mode the header has "Add" with its menu part, "Save" and "Revert" in one row of 44 px at 1920, 1280, 768 and 390 px, with no horizontal scroll. Up to 560 px, "Revert" is in the menu "More actions". No item of a menu has a button of its own. The menu of "Add" has three items, and "More actions" has "Dashboard JSON", "Settings", "History", "Export" and "Delete". The panel editor keeps the whole header, and "Add" is there but not active. |
| B55 | At the zoom levels 75, 90, 110 and 125 % and the widths 1920 and 1280 px, six stat panels in one row, a stat with three values, a bar gauge, a pie chart, a bar chart, a time series and a table have no child larger than its box by less than 1 px. Only the table and a longer legend or error text scroll. The list, the edit mode, the panel editor and the history dialog have no horizontal scroll of the page (9.19a). |
| B56 | Editor, no false error for a valid macro. The ClickHouse macros of 7.10 and the PostgreSQL macros of 7.22 in a WHERE, in a SELECT, with blanks, with line breaks, in a string and in a comment show no error in the SQL area, the error box, the panel JSON tab and the dashboard JSON tab. The text areas have `spellcheck="false"`. A wrong case or an unknown name shows the message of the server "is not a macro of Hune" with its line and column (10.3). |
| B57 | Editor, alignment of the selection. At the scale factors 1, 0.9, 1.1 and 1.25, in the light and the dark theme, with long lines, tabs, accents and µ, and with a horizontal scroll to the end, the two layers have equal metrics and an equal box. The gap between the glyphs of the two layers is less than 0.5 px. The gap of a selection edge, after the snap to the device pixel, is less than 0.5 px (10.3). |
| B58 | A reload shows one stable render (9.6, 9.8, 9.25). The case loads a dashboard with a variable, 11 panels and a scroll bar. It reloads the page six times: the system theme light and dark, and a stored choice that differs from the system, each with a warm and a cold cache and with a CPU slowed four times. A probe in the page logs each animation frame from the start of the document. The theme and the background are final in each frame. The first frame that the viewer sees has the font Inter, inline icons, the selector, the header buttons, the row of the variables and all the panel frames. After it, the header, the buttons, the row and each panel frame keep their place within 0.5 px, the width of the page does not change, no panel goes away and no panel loses its content. |
| B59 | The header (9.6, 9.9). At 1920, 1280, 768 and 390 px the header stays at the top after a scroll of 400 px. The box of each button does not change for an absolute range, a range of 90 days, the interval 30m, no interval, a running refresh, the edit mode and the state of "Save" (not active, then active). When the edit mode starts, no button of the view moves: the selector gets narrower by the width of the group plus one gap. A sampler of frames sees two layouts and no layout between them, at the start and at the end of the edit mode. The order is "Add", "Save", "Revert", the time control, the switch. A viewer has no "Edit", no group of the edit mode and no blank slot. The page has no Share control in the header or in the panel menu, and GET api/share answers 404. |
| B60 | Y axes (9.19). A time series with two series, of the units `ms` and `reqps`, has the left and the right scale. `data-axes` gives the two scales with their labels and soft limits. A series with `axis_placement` = `left` moves to the left scale. A hidden axis takes no room. With all the y axes hidden, the plot has no margin at the top (E27). A hard `min` wins over `soft_min`. The grid follows the left scale. A `barchart` has `axis_label` and soft limits on its value axis. |
| B61 | Series style (9.19). An override sets bars for one series and a dashed line with `line_width` 3 for another. `data-series-style` gives the styles. A series with `negative_y` draws below the zero line, and its legend value stays positive. The case reads pixels at known points of the canvas. |
| B62 | Colour modes (9.16). With `palette`, each series has the slot of its place in the series palette: when the series order changes, the colours change with it. A document with `palette_by_name` is refused with `bad_format`. Series 1 and 2 of a time series have the slots 0 and 1 of the series palette, in both themes, and a series with a fixed colour keeps its colour. A stat with `continuous_gr_yl_rd` shows the colours of its stops at the ends of the range, in both themes. A value `null` shows in the colour of the first stop. |
| B63 | Zone names (9.9). A dashboard with `timezone` = `Asia/Tokyo` runs in a Chromium with the zone `America/New_York`. A dashboard with `America/New_York` runs in a Chromium with the zone `Asia/Tokyo`. The x axis labels, the tooltip head, the cells of a `time` field and the footer of the picker equal the named zone. A day of a clock change gives the labels of the zone. An unknown name gives the notice `data-notice="zone"`, and the page uses the browser zone. |
| B64 | Time override (9.8b). The requests of three panels have the `from` and `to` that 9.8b computes: `time_from` = `now-24h`, `time_shift` = `1d`, and both. In a second run the dashboard range is absolute, and the panel with `time_from` does not change. The note shows with its text, and `hide_time_override` removes it. A `reuse` panel shows the note of its source. A range below 0 gives the panel state `error`. |
| B65 | Dashboard links (9.8a). A `link` with `include_vars` and `keep_time` has the URL with `var-<name>`, `from` and `to`. A `link` with the URL `javascript:alert(1)` gives no link and no script. A `dashboards` link with two tags lists the dashboards that have both, without the current one, in the order of 9.8a. A dropdown opens a menu, and a click opens the dashboard with `pushState`. A title with `<b>x</b>` shows as text. |
| B66 | The override form (10.3a). The case adds an override with each matcher kind. It sets a property of each control type, including `axis_placement`, `soft_max`, `draw` and `line_dash`. It checks the draft, the JSON tab, the limit of the properties and the error of a bad pattern. A save gives the expected document. |
| B67 | The forms of the mappings, the data links, the panel links and the transformations (10.3a). The case adds each kind, moves an item, removes an item and checks the draft. The URL `//x` gives the error of 4.23. A mapping with no text and no colour gives its error. A save gives the expected document. |
| B68 | The settings view (10.3b). The case edits the title, the tags, the time and the zone. It adds a `link` and a `dashboards` link, and one variable of each kind. It renames a variable and reads the notice. A save gives a document of format 3 with the expected members. A new dashboard has `"links": []`. |
| B69 | The joined time control (9.9). The time group has the back button, the range button, the forward button and "Zoom out". The refresh group has the refresh button and the interval button. The buttons of a group touch. The page has no second refresh button. Each button works with the keyboard. |
| B70 | The Edit switch (10.2). It has `role="switch"` and `aria-checked`, and Space and Enter change it. It shows only with the right to write. In edit mode the header has "Add", "Save" and "Revert", with no reserved slot before. "Save" is not active with no change. "Revert" asks first with the dialog "Discard your changes". |
| B71 | One header row (9.6, 9.9). At 1920, 1280, 768 and 390 px, in light and dark, in the view, in edit mode, in the panel editor and on the list, the header is 44 px high and never more than 48 px. It has one line of controls, it sticks to the top, nothing wraps, and the page has no horizontal scroll. The items of "More actions" follow the rights of the user, also for a viewer. "Delete" asks for a confirmation, and "Cancel" keeps the dashboard. |
| B72 | The panel editor from a panel view (10.2a). The case opens `viewPanel=<id>` and turns Edit on. The page shows the panel editor of this panel (`data-hune-view="panel-edit"`, `data-hune-origin="view"`), the URL has `edit=1` and `editPanel=<id>` and no `viewPanel`, and no panel view is in the page. "Back to panel view" shows the panel view with its URL, calls `history.back()` (the length of the history does not grow), and the Forward key shows the panel editor. With a changed title, `edit-off` and "Back to panel view" show the dialog. "Save" shows the panel view with the new title. "Discard" in the dialog shows the old title. The focus is in the editor region at the start and in the view region at the end. |
| B73 | The panel editor from the dashboard (10.2a). The menu item "Edit" in edit mode, the same item in the dashboard view and "Add panel" give the editor with the origin `edit`, `view` and `edit`. "Back to dashboard" calls `history.back()` and the stack depth is the same as before. The panel menu in edit mode has no "View". A deep link `editPanel=<id>` has no origin, and "Back to dashboard" uses `replaceState` and shows the state `edit`. "Discard" of a new panel shows the state `edit`, with no console error and with the edit bar. The focus returns to the menu button of the panel. |
| B74 | The panel view (10.2a). Each of the events `view-panel`, `close-view` by Escape and by the arrow after the selector, also at 400 px, and the item "View" for the panel that is shown. The last one adds no entry. The button "History" in the panel view shows the history, and "Back to the dashboard" shows the panel view again, with its URL. A change of the range, the refresh and a variable keeps `viewPanel` and uses `replaceState`. The focus returns to the menu button of the panel after `close-view`. |
| B75 | The dashboard edit mode (10.2a). The events `edit-on`, `open-json`, `open-settings`, `open-history`, `leave`, `discard`, `edit-off` and `save`. The history keeps `edit=1`, and its "Back to the dashboard" shows the state `edit` with the draft. `edit-off` with changes shows the dialog, and each of its three buttons does what 10.2 says. After the exit, the page has no hook of the editor (`edit-grid`, `edit-bar`). The stack after each open and leave pair is the same as before. |
| B76 | Back and Forward (10.2a, rules 9 and 6). A scripted walk goes through the seven states by their controls. Then the case presses Back to the start and Forward to the end. After each press, `data-hune-view` and the URL are the state of the entry, the page shows one view, and the page writes `replaceState` only for the normal form. With changes in the draft, a Back out of edit mode keeps the draft and shows the notice with "Resume" and "Discard". "Resume" opens the state `edit` with the draft. An open Save drawer closes at each press. |
| B77 | Reload (10.2a, rule 10). For each of the seven states, the case reloads the page. The state, the URL and `data-hune-origin` are the same. A sampler of frames (`requestAnimationFrame`, from the first frame) sees no frame with the hold off and a view that is not the state, no frame with an empty main area, and no change of more than 0.5 px of the toolbar. `leave` after the reload uses `history.back()`. A draft comes back with the notice "Your draft is back.". |
| B78 | Deep links (10.2a, rules 1, 3, 4 and 11). The case loads the vectors of `FE-26`: each URL with two or more view names, with an unknown id, with an unknown value and with the form `panel-<id>`. For each, `data-hune-view` is the state of the vector, the URL is the normal form, the page shows one view, and a notice shows for an id that does not exist. The URL with `editPanel` and `viewPanel` shows the panel editor and no panel view. |
| B79 | Rights (10.2a, rule 8, and 10.14). On the server with no primary store: each deep link with edit names shows the state `view` or `panel-view`, the notice "You cannot edit this dashboard.", the URL with no edit name, and no "Resume" notice, also when a draft is in `localStorage`. The draft stays. `editview=history` shows the history with no "Restore". On the server with a primary store, the page injects `editing = false` in the session and presses Back and Forward over edit entries: the same result. A save that gets `403` keeps edit mode and the draft. |
| B80 | Placed controls (4.7a, 9.10). A dashboard places `env` (select), `node` (multi, All, `hide = "label"`) and `min_weight` (number, title "Threshold", second section). `account` has no placement and `region` has `hide = "variable"`. Each hook `variable-<name>` occurs one time and is inside its frame. The frames have the grid places of the placements, and the bar shows `account` only. A change of `env` writes `var-env` and runs the stat with the new value. A change of `node` and an entry of `min_weight` that breaks the number grammar behave as in the bar (B08). A reload with `var-env` in the URL shows the value in the frame. |
| B81 | The row under the header has no height when no variable is left (9.10). A dashboard whose variables are all placed (or have `hide = "variable"`) and that has no dashboard link shows no row: its height is 0, and the first panel starts at the same place at the first frame and after the load of the options (the probe of B58, no move of more than 0.5 px). With a dashboard link, the row stays and holds the link only. A dashboard with variables that have no placement shows the bar with exactly these variables. |
| B82 | Keyboard and accessibility of a placed control (9.26). The Tab order is the order of the grid. `Enter`, `Space`, the arrows, `Escape` and the search field work in a frame as in the bar. After `Escape`, the focus returns to the button. A text control applies on `Enter` and on blur. The accessible names are `<label>: <text>`, also with `hide = "label"`. The frame has `role="group"` and the name of its title, else of its label. Scripted checks (unique ids, resolved references, contrast of the texts, focus ring of 3:1) run in both themes (axe-core needs the network). |
| B83 | The modes of the page (9.8, 9.10). In `viewPanel=<id>`, the bar shows each variable that has no `hide = "variable"`, also the placed ones, and each hook occurs one time. In the panel editor, the bar shows the same set. A frame has no menu button in view mode. A collapsed section with a placed control shows no control, and the bar does not show its variable. The open section shows the control. Print hides the frames and keeps the line of the values. |
| B84 | The editor (10.3b, 10.6). The switch "Place in the grid" gives the placement of 10.6. The section, the title (an empty title removes the member) and the fields give the draft. The select `hide` has no `variable` for a placed variable. The frame is in the draft grid, the bar loses the variable, and a change in the frame acts on the page selection. A move and a resize by the keys change the placement only. A move of a section changes the index, and a delete of a section puts the variable back in the bar at once. "Put back in the top bar" removes the placement. A rename keeps the placement. After a save and a reload, the document equals the draft, and a variable with no placement has no member `placement`. |
| B85 | The layouts of a placed control (9.8, 9.11, 9.19a). At 1920, 1280, 768 and 390 px, and at the zoom levels of B55, the control stays inside its box, the label chip shows an ellipsis when it is short, and the page has no horizontal scroll. The flow keeps the order of the grid and the heights of the formula (68 px for 2 rows, 30 px for 1 row in one column). A frame of 1 row scrolls its body and shows no error. |
| B86 | The format and the server (4.7a). A `placement` with a `section` that does not exist gives `422 invalid_document` with `bad_reference` at `/variables/<i>/placement/section`. A `placement` on a variable with `hide = "variable"` gives `not_permitted`. A document with no placement has no member `placement`, and the placement is the last member of a variable in the canonical form. |
| B87 | Leave warnings (10.2, 10.8, 10.2a). In each edit state with changes, a click on the brand and a choice of another dashboard in the selector shows the dialog. `beforeunload` runs at a reload. The Back and Forward keys show the dialog never. "Resume" from the state `view` and from the state `panel-view` opens the state `edit` with no panel view in the page. "Discard" in the notice removes the draft. |
| B88 | Entry redirect and states (9.7, 10.2a, rule 13). A volume with one dashboard: the redirect uses `replaceState`. Edit on, then Back twice, leaves the page. A state URL (`viewPanel=<id>`, `edit=1`) with `q` gives no redirect. The brand link shows the list with its row. |
| B89 | A walk of events (10.2a). The case runs a fixed seed of 80 random events on the page: the events of the first table, plus Back, Forward and reload. After each event, `data-hune-view` is the state that `parse(location)` gives, the URL is the normal form, the page shows one view, no dialog stays after a Back or a Forward, and the console has no error. The seed and the list of events print when the case fails. |
| B90 | The button "Add" (10.2, 10.6). It is a split button: one click adds a panel and opens its editor. Its menu has "Add panel", "Add section" and "Place a variable in the grid". With several variables that can go to the grid, the last item opens a menu of choice. With one variable, it places the control at once. With none, it is not active. The label hides at 900 px, and the menu part is 20 px wide at 560 px. The keyboard works: Tab, Enter, Space, the arrows and `Escape`. |
| B91 | The colour theme of the chrome (9.25, 9.26). The case opens the page of an error in the dark, the light and the system mode (a light and a dark system). It reads the computed colour of each chrome token. These values equal the values of the copied file `00-tokens.css`, set again by `look.css`. The Hune roles follow their token. The page has the colour `--bg` and the header has the colour `--panel`. The text has 4.5:1 on each surface. The edge of a control and the focus ring have 3:1. The colours on a fill have 4.5:1. |
| B92 | The time range picker of ClickHouse Dash (9.9). The case checks the geometry and the texts of the popup, the quick ranges and the search, the recent ranges, the absolute fields and their errors, and the calendar (two clicks, a preview and the keys). It checks `Enter`, `Escape` and `Tab`, the motion of 160 ms and the reduced motion, 768 and 390 px, and the buttons back and "Zoom out". |
| B93 | Repeat of a panel with direction `h` (4.13a, 9.8c). A dashboard with the variable `host` (3 values, multi, All) and a panel with `h`. With 1, 2, 3, 5 and 6 values and `max_per_row` absent, 2, 3 and 5, each copy has the geometry of the layout rule: c equal columns with a gap of 8 px, the last row not stretched, the height of the panel, and the full width for one copy. The title of each copy shows its option text. The order of the copies is the order of the options, also for the URL `var-host=c&var-host=a`. The request of each copy (read on the network) has the repeat variable with `all` equal to `false` and one value, and the other variables as in the bar. The count of panel requests equals the count of copies. |
| B94 | Repeat of a panel with direction `v`, and the moves of the grid (9.8c). A panel with `v` has panels beside it, below it in the same columns, below it in other columns, and a panel with the full width at the end. The copies are one under the other with the `x` and `w` of the panel. The displayed `y` of each other panel equals the number that the move rule gives (the vectors `FE-27`), and the panels beside do not move. The stored `x` and `w` of a panel with `h` do not change the view. The page has no overlap and no horizontal scroll. |
| B95 | Repeat of a section (4.13a, 9.8c). The copies have the header title with the option text, their panels and the keys `section-<index>-r<n>` and `panel-<id>-r<n>`. The request of each panel of the copy n has the repeat variable fixed to the value of the copy. Each copy opens and closes by itself, a collapsed copy sends no query, and the first section is never a copy. The copies are in the order of the options. A copy of a section whose title has no placeholder has the accessible name of the title, a comma and the option text. Print shows the copies and the line of the values. |
| B96 | Empty and too large (9.8c). A `query` variable with no row gives the state `repeat-empty` for a panel and for a section, with the message and no request. A selection of 24 values gives 24 copies, and 25 values give the state `repeat-limit` with the message of the panel, no copy and no request (the limits of the session are the defaults 24, 12 and 120 of `server.repeat_max_per_*`, and a second server with smaller values gives the same states at those values). A section with 13 values gives the state `repeat-limit`. A dashboard whose repeats cost more than 120 gives the state `repeat-limit` with the message of the dashboard to the repeats that do not fit, in the reading order, and the same result after a reload and after a scroll. A change of the selection moves a panel between the states and the copies at once. Before the options load, the frame is `idle` and no message shows. |
| B97 | The bound of the queries (9.8c). A panel with 24 copies one under the other, taller than the viewport: at the load, the page requests only the copies near the viewport, at most 6 at the same time; a scroll requests the others; at the end there are 24 requests and no more. A new value in the selection aborts the old requests (the source loses them within 2 s, 8.9) and starts the requests of the new copies. A refresh runs each copy one time. The page sends no extra request to read the values (the options request count does not change). With a small `max_concurrent_per_subject`, the copies queue, a copy that waits past `queue_wait` shows `source_busy`, and "Retry" runs that copy only. |
| B98 | URL and states (9.5, 10.2a, rules 1 and 16). `viewPanel=<id>-r<n>` and `viewPanel=panel-<id>-r<n>` show the copy n at full size, with its query. The form `<x>$panel-<id>` of Grafana shows the copy whose value is `<x>`: `b$panel-<id>` shows the copy 2, and `a$panel-<id>` shows the first copy. The URL then has the key of the copy. `2$panel-<id>` (an index), `Bravo$panel-<id>` (the option text) and `B$panel-<id>` (another case) show no view and give the notice. With a slow `query` variable, the form of Grafana keeps its URL and shows the frame of the panel in the state `idle`, with no query, until the options load. Then it shows the copy of its value, or the notice. The menu item "View" of a copy writes `<id>-r<n>`, and of the first copy the number. The item "Edit" opens `editPanel=<id>`. A copy number above the count, or `editPanel=<id>-r2`, is removed with `replaceState` and a notice after the options load. A change of the selection that removes the viewed copy closes the view with the notice. The seeded walk of B89 runs on the repeat dashboard. |
| B99 | The editor (10.3, 10.3b, 10.6). The group "Repeat options" lists only the variables with `multi`, sets `grid.x` = 0 and `grid.w` = 24 for "Horizontal", removes `max_per_row` for "Vertical" and shows the count and the hint. The group is not active for a reuse panel. The item "Repeat by a variable" and its dialog work, and the item is not active for the first section, for a section with a placement and for a section with a repeating panel. The draft grid draws the copies, and only the first copy can move. A rename rewrites `repeat.variable`, the delete dialog lists the panels and sections, and the switch `multi` is not active while a repeat uses the variable. After a save and a reload the document equals the draft. The JSON tab shows each error of 4.34 with its pointer. |
| B100 | Layouts and accessibility (9.8c, 9.26). At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the copies stay inside their boxes, the page has no horizontal scroll, and the flow keeps the order of the displayed `y`, `x` and n. The accessible name of each copy tells it apart. The Tab order follows the grid. The notes have `role="note"`. The scripted checks (unique ids, resolved references, contrast, focus ring of 3:1) run in both themes. |
| B101 | Hostile values (7.11, 9.8c). A `query` variable returns option texts and values with quote marks, comment marks, `$`, Unicode spaces, a value of 1,024 bytes and the text `<img src=x onerror=alert(1)>`. Each copy sends its one value. The server refuses an unsafe value with `variable_value_refused`, and only that copy shows the error; the other copies show data. An option text shows as text, and the page has no element made from it. A URL with 10,000 values of the repeat variable and a variable with 5,000 options give the state `repeat-limit` and no panel request. The page has no console error. |
| B101p | The parity of the copies (15.14c). The capture pairs each copy of the fixtures `repeat-h`, `repeat-v` and `repeat-section` (Grafana `panel-<id>-clone-<k>` with Hune `panel-<id>-r<k + 1>`), and it compares the box, the title and the number of queries of each copy. It reports no fault that the list of open faults does not name. |
| B102 | The look of a text panel (9.19b). A dashboard with one text panel that has each construct. In both themes, the computed values equal the table of the look: the font sizes, the line heights, the weights, the margins, the paddings, the borders and the fills (the colours equal the tokens). A link has the colour `--link` and an underline. The scroll box has `role="region"`, the name of the panel and `tabindex="0"`. A text panel sends no request: the network shows no panel query for it, and `data-state` is `ready` after the first draw. An empty content shows an empty body. A panel with `transparent` has no fill. The title has the option text of a variable, and the content has its value. |
| B103 | Hostile vectors (9.19b, T53 and T56). A dashboard has 40 text panels. Each panel has one payload: `<script>`, `<img onerror>`, `<iframe>`, `<svg onload>`, `<style>`, `<a href=javascript: onclick>`, `<meta refresh>` and the forms of the mutation attacks of mXSS. They also have `javascript:` links in all cases of letters, a tab, an entity and a leading space, `data:` and `vbscript:` links, the forms `//host` and `/\host`, an image that points to a recording server, a `"` in a destination, and an autolink with a script. The page opens no dialog and has no console error and no CSP violation. The recording server sees no request. Each panel has only the elements of the list of 9.19b and only the attributes `href`, `rel` and `class`. No element has `style` or an attribute that starts with `on`. Each `a` has `rel="noopener noreferrer"` and a destination that passes 4.23. The payloads show as text. |
| B104 | Variables in a text (9.19b). A text has `$v`, `${v}`, `${v:text}`, `${v:csv}`, `${v:pipe}`, `${v:glob}`, `\$v`, an unknown name, a name with `__`, `[[v]]`, a placeholder in an inline code span, in a code block and in a table cell, and a placeholder in a link destination. The variable has a single value, several values, "All" with an `all_value` and "All" without it. The values hold `<b>x</b>`, `*x*`, `[x](javascript:alert(1))`, `` ` ``, `|`, `$` and a line of 1,024 bytes. The text shows each value as text, with the form of the table. The destination stays as written. A change of the selection changes the text with no new request and no reload. The elements are the same as before the change. A selection that gives more than 262,144 bytes shows the message of `MD_MAX_EXPANDED_BYTES`. |
| B105 | Limits and failures (9.19b). One panel for each limit: 2,001 lines, 65,537 bytes, a depth of 7, an inline depth of 7, 4,001 nodes and 21 columns. Each panel is in the state `text-limit` with its message and no text. The panels at the limit (2,000 lines, a depth of 6, 20 columns) show their text. The 12 hostile families of the property tests, at 65,536 bytes, show a message or a text, and no task of the main thread is longer than 200 ms. The editor preview shows the same messages. The page has no console error. |
| B106 | The editor (10.2, 10.3, 10.6). "Add text panel" works when no source exists, and it opens the editor with the new panel. The group "Text" has the field, the counter and the help. The preview changes 300 ms after the last key. The errors of a control character, a long line, too many lines and too many bytes show with `text-error`, and the field has `aria-describedby`. The key `Tab` moves the focus out of the field. The tab "Query" has the note and no field. The groups of the options pane are the groups of 10.3. The change of the type opens the dialog in both directions, and "Cancel" changes nothing. "Revert" restores the saved panel. A save and a reload give the same document. The JSON tab shows each error of 4.33a with its pointer. |
| B107 | Layouts and accessibility (9.19b, 9.26). At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the text stays in its box, long words break at the edge, and a wide table and a long code block scroll in the scroll box. The page has no horizontal scroll. The flow has the floor of 160 px. `viewPanel=<id>` shows the text at full size and the menu has "View" only. The menu has no "View data" and no "Inspect". Print shows the text. The keys reach the scroll box and each link. The scripted checks (the contrast of the link, the text and the lines, the focus ring of 3:1, and axe) run in both themes. |
| B108 | The format and the server (4.33a, 6.20). A save with a text panel gives `200` and the canonical bytes. Each invalid document of 16.3 gives `422 invalid_document` with its code and its pointer. A panel query and a preview of a text panel give `400 bad_request`. A `reuse` of a text panel gives `bad_reference`. In the mode `authzen`, a subject that has `dashboard.read` and no `source.query` loads a dashboard with a text panel only. A document that has no text panel keeps its bytes. |
| B109 | The import (15.6). The importer reads the file `tests/fixtures/grafana/text_panels.json`: a panel in each mode, a panel with HTML in a `markdown` content, a panel with a table and an image, a panel with placeholders, a panel with `repeat`, and a panel with an empty content and with a text of spaces. The report lists the items of the rows of 15.6. The server loads the written dashboard, and each panel shows the text of its row: the HTML and the code in a code block, the image as `!` and a link. |
| B110 | The look of a `state_timeline` (9.19c). The fixture of the measure: 1, 3 and 15 rows, the options `row_height`, `line_width` and `fill_opacity` of the table, and the time range of 5 hours. The page equals the numbers of 19.20d: the plot box, the rows, the segments (x and width, 1 px tolerance), the fills and the lines, the texts and their x and y, and the grid lines. The numbers come from `data-timeline` and from the pixels of the canvas. Both themes. |
| B111 | The look of a `status_history`. The cells: the width for `col_width` 50, 90 and 100, the range of the axis, the text that needs 2 characters, the line of 1 px, and a row of 1 sample (the error). The cap of the width: 491 rows pass and 492 fail at a canvas of 982 px. Both themes. |
| B112 | The states and the colours. The mappings of the kinds value, range, regex and special. A numeric field in the colour modes `thresholds` (the texts `< 1` and `1+`, the merge by step), `fixed`, `palette` and `continuous_gr_yl_rd`. A text field, a bool field and the empty text. The ink has 4.5:1 on each fill in both themes. The legend for each mode: the steps, the different states, and none for a continuous mode. Hostile texts (`<img onerror>`, 10,000 characters, bidi characters, U+0000) show as text and make no element. |
| B113 | The data cases. The sample that is null, NaN or mapped as null. `span_nulls` equal to `true` and `false`. A gap in the time. Rows that are not sorted, and two samples with one time. A frame with two time columns, with no time column, with no row and with no data. The format `time_series` with three hosts. The panel with a `reuse` and with `transforms`. `hide_from.viz` equal to `true`. |
| B114 | The caps (9.19c, T57). Panels with 9,999, 10,000 and 10,001 segments: the first two draw, and the third shows its message in the state `timeline-limit` with no picture. A `status_history` at the cap and at the cap of the width, and the new draw after a resize. A panel with 10,000 segments builds the model and draws in 100 ms or less. No task of the main thread is longer than 200 ms. The page has no console error. |
| B115 | The tooltip, the hover and the keyboard. `single`, `multi` and `hidden`. The head with two times and the row "Duration" of a `state_timeline` and the head with one time of a `status_history`. The text of the duration forms. The place of the card and its flip at an edge. The white layer on the hovered box. The keys `ArrowRight`, `ArrowLeft`, `ArrowDown`, `ArrowUp` and `Escape`. The hidden description. The scripted checks of axe and of the focus ring run in both themes. |
| B116 | The editor and the format (10.3, 4.33b). The group "State timeline" and the group "Status history" have each control, with the range of its option. The legend has no mode and no values. A change of the type keeps the data and takes the new options. A save and a reload give the same document. The JSON tab shows each error of 4.33b with its pointer. A document with each type saves with `200` and the canonical bytes. |
| B117 | The import and the parity (15.6, 15.14c). The importer reads the 8 fixtures, and the report has the classes of the rows. The capture compares Grafana and Hune on each fixture: the boxes (1 px), the colours, the texts and the tooltips. The differences of 19.20d are the only differences. |
| B118 | Layouts and zoom (9.19a, 9.26). At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the picture stays in its box, the legend moves under the plot when the panel is narrow, and the page has no horizontal scroll. `viewPanel=<id>` shows the picture at full size. The menu has "View", "View data" and "Inspect". Print shows the picture. |
| B120 | The look of the marks (9.19d). The fixture: 2 time series, points and regions in 3 colours, a range of 5 hours. The page equals the numbers of 19.20e: the x of each line, the width of each region, the line of 2 px with the dash `[5, 5]`, the fill 0.1, the triangle of 10 px by 5 px and the bar of 5 px (1 px tolerance). Both themes. A region that starts before the range and one that ends after it. A mark outside the plot has no line. |
| B121 | The queries (9.8, 9.19d). A dashboard with 4 time series and 2 annotations sends 2 annotation requests at the load, 2 at a refresh and 2 at a change of the range. A change of a variable that one SQL names sends 1. A pointer move, a hover and a legend click send none. An annotation with `enabled` equal to `false` and an annotation whose panels are all in a collapsed section send none. The requests come after the panel requests. A new trigger aborts them. The expansion of the section sends them. The `viewPanel` of a time series sends them. A `stat` panel gets no mark. |
| B122 | The card, the hover and the keyboard. The head with one time and with two times, in the zone of the dashboard. The title, the text with a line end, 8 tags. The place of the card and its flip at the right edge. The hit zone of 24 px and the 8 px of the line. A hostile text is refused by the server (`annotation_invalid`) and the panel shows the note. A text with `<` and `&` that is not a tag shows as written. No element of a card comes from data. The keys `ArrowRight`, `ArrowLeft`, `Home`, `End` and `Escape`. The status text. The second table of "View data". |
| B123 | The caps (9.19d, T63). An annotation of 1,000 rows draws. One of 1,001 rows gives `422 query_limit` and the note. An annotation with a ceiling `max_rows` of 10 gives `ceiling_exceeded` and the note. Two annotations of 1,000 and 1,001 marks on one panel (2,001) draw nothing and show the note "Annotations: too many (2001, at most 2,000)". 2,000 marks draw in 50 ms or less. No task of the main thread is longer than 200 ms. A failed annotation does not stop the other annotations. |
| B124 | The access (11.21, 11.24). In the mode `authzen` with the fake PDP: a viewer with `dashboard.read` sees the marks. A viewer without it gets `403` and no mark. An author needs `source.query` on the source of each annotation to save. The PDP down gives `503 pdp_unavailable` and the note. The route is in the table of 11.21. |
| B125 | The editor and the format (10.3b, 4.5b). The tab "Annotations" has each control with its range. The button "Add annotation" is not active at 8 items. The group "Panels" lists the time series only. A delete of a panel removes its id from the lists and the last one sets `enabled` to `false`, with the notice. A rename of a variable lists the annotation in the notice. The JSON tab shows each error of 4.5b with its pointer. A save and a reload give the same document. The marks of the draft show after the save. |
| B126 | The import and the parity (15.6, 15.14c). The importer reads the 4 fixtures, and the report has the classes of the rows. The capture compares Grafana and Hune on each fixture: the x of the marks (1 px), the widths, the colours and the counts of the requests. The differences of 19.20e are the only differences. |
| B127 | Layouts and views (9.19d, 9.26). At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the marks stay in the plot. The card stays in the panel. `viewPanel=<id>` shows the marks at full size. Print shows the lines, the regions and the triangles and hides the card. The page has no horizontal scroll. The `cursor_sync` crosshair and the drag zoom work with the marks. A time series with `draw` equal to `bars` has the marks. A `barchart`, a `table`, a `piechart` and a `text` panel have none. |
| B130 | The join (4.15a, 9.22a). A time series with 3 queries `A`, `B` and `C` on the same time grid, and a time series whose queries have times that differ (`B` shifted by half a step). The legend shows `A a`, `B b`, `C c` in the order of the queries, also when the refs are `Z`, `A`, `M` in the document. The chart has one point for each distinct time. The tooltip shows one row for each series. The request count equals the count of visible queries, and each body has its `ref`. A panel with one query sends no `ref` and has the legend of today (`a`). The colours follow the order of the fields. A panel with `time_from` and `time_shift` sends the same range to each query. |
| B131 | Hidden queries (4.15a, rule 3). A panel with `B` hidden sends 1 request and its legend has `a`, with no prefix. A panel with all queries hidden sends no request and shows "No data". A hidden first query works. A direct request for a hidden query gives `400` (the page never sends it). The eye in the editor changes the draft and the preview at once. |
| B132 | Failures (9.8d). `B` fails with `sql_invalid`, `A` and `C` answer: the panel draws `A a` and `C c`, the badge lists "Query B: sql_invalid: ...", `data-failed` is `B`, and no "Retry" shows. The names keep the prefix. A slow query keeps the panel in `loading` with the earlier content, and the panel draws when all queries ended. All queries fail: the state is `error` with the first error, the prefix "Query A: " and "Retry", which runs all the queries. A request error (`403`, `503 pdp_unavailable`) shows in all the queries. A `409` reloads the page as for one query. A badge has `role="img"` and the text of each failed query. |
| B133 | The types (9.22a). A table with 2 queries has the select "Query" under it, the headers `A n`, and a change that shows the table of `B`. A `stat` with 2 queries shows two tiles and no join. A bar gauge and a pie chart collect both queries. A state timeline and a status history join on time. A panel with a frame that has no time column shows the error of 9.22a, rule 6. A join of 200,001 rows shows the error and draws nothing, and 200,000 rows draw. A frame with NULL times and equal times follows the rules of 9.22. The unit of a field, a threshold and an override by `query` apply to the right field. |
| B134 | The bounds (9.8d, T67). A dashboard of 60 panels with 3 queries each, taller than the viewport: at the load the page requests only the queries near the viewport, at most 6 at the same time. A scroll requests the others, and at the end the count equals the count of visible queries. A refresh and a new range abort and run each visible query once. A change of a variable that one SQL names sends 1 request for that query, and the other frames stay. A collapsed section sends none. With `max_concurrent_per_subject` = 2, the queries queue, a query that waits past `queue_wait` shows `source_busy` alone, and the panel is partial. A panel with `repeat` and `extra_queries` is refused at save. A document with 400 queries is valid and one with 401 is refused. |
| B135 | Mixed sources and reuse (4.15a, 9.23). A panel with a query on `prod` and a query on `conf` runs each on its own source, and `Inspect` shows two blocks with the SQL of each. A `reuse` panel of a source with 2 queries shows both frames, with the rule of its own type: a table has the select, a stat has two tiles. The source runs its queries out of view and in a collapsed section. A `reuse` panel of the type `barchart` whose source has 2 queries is refused at save. A partial result of the source shows the badge in the reuse panel. "View data" shows the combined fields, and the CSV has the display names with the prefix. |
| B136 | The editor (10.3). The chips `A`, `B`, `C`, with "Add query" (the first free ref), the rename, the eye and "Delete". The button is not active at 8 queries and for a panel with `repeat`, and the tooltip says why. The delete of the first query makes the next query the first, with the members of the panel. The group "Query options" holds the five fields of the panel. "Run" sends one preview for each visible query, and an error of one query shows as a partial result. The change to `barchart` and the change to "Reuse a panel" open the dialog. The JSON tab shows each error of 4.34 with its pointer. After a save and a reload the document equals the draft, and a panel with one query has no `extra_queries`, no `ref` and no `hide`. |
| B137 | The import, the parity and the accessibility (15.6, 15.14c, 9.26). The importer reads the 6 fixtures `multi_query_*.json`. The report has the classes of 15.6. The visual capture (15.14c) compares the display names, the state, the box and the title of each panel, and it reports no fault that the list of open faults does not name. At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the select and the badge stay in their box. The scripted checks (unique ids, resolved references, contrast of the texts and the badge, focus ring of 3:1) run in both themes. The page has no console error. |
| B150 | The fixed buttons of the header (9.6, 9.9, 10.2). At 1920, 1280, 900, 768 and 390 px, in the light and the dark theme, the time control, the switch "Edit", the theme button and "More actions" have the same x and y (to 0.01 px) before the edit mode, after it starts and after it ends. A sampler of frames sees one layout of these four buttons during each change. The group of the edit mode ends before the time control, and the selector gets narrower. The tab order, and the real key Tab from the brand, follow the visual order from the left. The long title in the selector ends with an ellipsis, and its tooltip has the whole text. |
| B151 | The way back of the panel editor (10.2a, 10.3). The editor has no row for the back button: it starts at the foot of the header, and at 1920 and 1280 px it fills the window under the header (48 px more than before). At 768 and 390 px it starts at the foot of the header too. The test runs in the light and the dark theme. The arrow after the selector is the link `editor-back`, with the name and the tooltip "Back to panel view", "Back to edit mode" or "Back to dashboard" for the true target. A click pops the history from a panel view, and the focus goes to the view region. Enter on the link goes back to edit mode, and the focus goes back to the control that opened the editor. Tab reaches the link after the brand and before the time control. The link shows its arrow alone at every width, and it works. |
| B152 | The switch "Transparent background" (4.13, 9.8, 10.3). A dashboard has the 10 panel types and one panel that the test never changes. In the light and the dark theme, the test opens the editor of each type. It measures the computed fill and edge of the preview frame before the switch, after it, after a second click and after a third click. Before, the fill is opaque and the edge shows. After, both are fully transparent, the frame has `data-transparent`, the title bar has no fill, and the title has a contrast ratio of 4.5 or more against the page colour. The edge shows on hover. The draft in the local storage has `transparent` = `true`. The test then saves the document. In the saved view, the 10 changed panels have no fill and no edge at rest, and the edge shows on hover. The panel that the test did not change keeps its fill and its edge. Three panels that are transparent show the states `loading`, `error` and `empty`. Their frame has no fill, and the text of each state has a contrast ratio of 4.5 or more against the page colour. The case fails when the switch has no effect. |
| B153 | The audit of the options of the panel editor (10.3, 10.3a, 10.3h). For each type, the test builds the states of its table (10.3h, rule 14), lists the controls of each state, and changes each one like a person does. It reads the draft in the local storage and a print of the preview (the markup of the frame, a hash of each canvas, and the card of the tooltip after a hover). The rules are F1 to F6: the draft changes in the member of the control and in the `writes` of its row only; the preview changes, unless the control is in the closed list of 30 entries at most; the preview comes back to its first print after the old value is put back; a stale entry and a dead entry fail; each row of each table has a control, and each control has a row and a `data-option`; no control is skipped. It also adds an item for each button "Add" on a fresh page, sets the repeat variable, the direction and the maximum for each row, and saves a title, a description and a width to read them in the view. The case has its own time limit of 900 s. |
| B160 | A closed `query` variable (9.10). A URL value that is not an option gives the default, the first option or "All", with one notice (`data-notice="variable"`) and the new value in the URL. A `multi` URL with one wrong value keeps the others. A change of the range with `refresh = time_change` that removes the selected option gives the same result. An options error and an empty list give no notice. A reload of the new URL gives no notice. |
| B161 | An open `query` variable that is not `multi`. A typed value shows the row `Use`, `Enter` selects it, the URL holds it, and a reload keeps it. A change of the range keeps it. The reopened popup shows the typed row as the first row, selected. No request goes out while the person types (the count of requests). A text that equals an option gives no row `Use`. |
| B162 | An open `multi` variable with "All". `Enter` adds a typed value and keeps the popup open. Two typed values and an option are three checked rows. A click on a checked row removes it. `Enter` in an empty field applies, the URL repeats `var-<name>` in the order, and the select shows the texts. "All" and a typed value exclude each other. |
| B163 | The combobox (9.26). The attributes `role`, `aria-expanded`, `aria-controls`, `aria-autocomplete` and `aria-activedescendant`. The focus stays in the field, the arrow keys move the active row, `Space` types, `Escape` closes with no change, and `Tab` closes. The errors for 1,025 bytes, a pasted control character and `$__all` have `role="alert"`. axe-core gives no violation in both themes. |
| B164 | The state "No options" (9.10). In the bar, in a placed frame and in the panel view: the muted text, the name `<label>: no options`, the popup text, and the panels that run with the empty value. A closed variable with `include_all` shows "All". An open variable takes a typed value. |
| B165 | A `custom` variable and the server (4.9, 7.7). A closed variable: a URL value outside the options is replaced with the notice, and a request sent with `fetch` gives `422` with no source query. An open variable: a typed value `a'b` reaches the panel as a literal. A save of an open variable in a quoted name shows the error in the editor. |
| B166 | The placement and the repeat (4.7a, 4.13a). An open variable in the grid has the same control and the same keys. A repeat by an open `multi` variable makes one copy for each selected option, and a typed value makes no copy. A closed variable is as it was. |
| B167 | The editor (10.3b). The switch is off for a new variable, with the help text of each kind. The switch changes the draft, and the draft grid shows the new control at once. The default of an open `custom` variable can be a typed value. The menu `Examples` puts the text at the cursor, and the undo of the browser takes it out in one step. "Show options" shows the count, 50 rows at most and the error of a wrong SQL. |
| B168 | The import (15.6). The fixtures of 15.14b give the member for the cases of the table: `true`, `false`, absent, a `custom` variable in a quoted name and in code. The report has the `changed` rows. The editor shows the switch on for an imported open variable. |
| B169 | Layouts and themes (9.10). At 1920, 1280, 768 and 390 px, in light and dark, the popup of an open variable stays in the viewport, the rows are 38 px high, the typed rows and the row `Use` keep the contrast of 9.26, and the page has no horizontal scroll bar. |
| B140 | The look of a `heatmap` of raw values (9.19e). The fixture of the measure: 20,000 values over 5 hours, with the defaults, and the options `x_buckets`, `y_buckets` (`linear`, `log2` and `log10`) of example 1 and 2. The page equals the numbers of 19.20g: the grid, the number of cells (126, 37, 383, 272, 101 and 494), the size of a cell, the axis labels, the legend (a canvas of 300 by 9 px) and its labels. The numbers come from `data-heatmap` and from the pixels of the canvas. Both themes. |
| B141 | The shape that is already bucketed. Names that are numbers (sorted, with `+Inf` last) and names that are not. Null cells and zero cells (122 cells for 240 positions), one time row (the error), irregular times (the first step), negative counts, and the ordinal labels with the rule of the thinning. A format `time_series` with the long-to-wide step. Hostile names (`<img onerror>`, 10,000 characters, bidi characters, U+0000) show as text and make no element. Both themes. |
| B142 | The colours and the legend. Each of the 10 schemes in both themes, with the colours of the first, the last and 4 middle steps. The mode `opacity` with both scales and the exponent. `steps` 2 and 128, `reverse`, `min` and `max`, `hide_le` and `hide_ge` (the colour range of the cells that show), `cell_gap` 0, 1, 3 and 4, and `show_legend` off. |
| B143 | The axes and the options of the value axis. `y_axis` hidden (no margin at the top, E27), `y_reverse`, `y_min` and `y_max` with a linear and a log scale. The ticks of a log scale for the three measured cases. The unit and the decimals of the fields. The cap of the raw values: 500,000 pass and 500,001 fail. The note of a log scale with a value of 0 or less: the page skips the value and the note gives the count. |
| B144 | The card, the hover and the keyboard. `single`, `multi` and `hidden`, with the head of the end of the bucket, the rows `Count` and `Bucket`, `Value` and `Name`. The highlight of the cell. The keys `ArrowRight`, `ArrowLeft`, `ArrowDown`, `ArrowUp`, `Home`, `End` and `Escape`. The hidden description. The scripted checks of axe and of the focus ring run in both themes. |
| B145 | The look of a `histogram` (9.19f). `bucket_count` 10 and 30, `bucket_size` 25, `bucket_offset` 10, `combine`, two fields that overlap, `stacking` `normal` and `percent`, `line_width` 0 and 3, `fill_opacity` 30 and 80. The measured cases: negative values, two clusters with a gap, equal values, one value, null values and the legend at the right. The page equals the numbers of 19.20g: the size of the bucket, the bars (x and width, 1 px tolerance), the ticks (every second edge for narrow bars) and the card. The line of a bar stays inside the bar, so the boxes of a stack are one line width apart (E26). The y axis reads the counts of the series that show: a click on the legend changes it. The input of buckets that the query made. Hostile series names. Both themes. |
| B146 | The caps and the time (9.19e, 9.19f, T69). Grids of 249,999, 250,000 and 250,001 cells: the first two draw, and the third shows its message in the state `bucket-limit` with no picture. Histograms of 3,999, 4,000 and 4,001 buckets, and of 500,000 and 500,001 values. A grid of 250,000 cells draws in 700 ms or less. A heatmap of 500,000 values makes its grid in 50 ms or less and a histogram of 500,000 values makes its buckets in 150 ms or less. No task of the main thread is longer than 800 ms. The page has no console error. |
| B147 | The editor and the format (10.3, 4.33c, 4.33d). The groups "Heatmap" and "Histogram" have each control, with the range of its option. The log scale hides the choice "Count". A change of the type keeps the data and takes the new options. A save and a reload give the same document. The JSON tab shows each error of 4.33c and 4.33d with its pointer. A document with each type saves with `200` and the canonical bytes. |
| B148 | The import, the parity and the layouts (15.6, 15.14c, 9.26). The importer reads the 8 fixtures, and the report has the classes of the rows, with the old form of Grafana 7 to 9 removed. The visual capture (15.14c) compares the cells and the bars, the texts, the box and the title of each panel, and it reports no fault that the list of open faults does not name. At 1920, 1280, 768 and 390 px and at the zoom levels of B55, the picture stays in its box, the colour bar stays in its box, and the page has no horizontal scroll. `viewPanel=<id>` shows the picture at full size. Print shows the picture. |
| B170 | The folds (10.3c, 10.3e). A query with a WITH, two subqueries and a call on several lines, in a ClickHouse source. Each marker has a name and `aria-expanded`, and is not in the Tab order. A click and the key `Ctrl+Shift+[` fold and unfold. The text area has the same value, selection and `scrollTop`, and keeps the focus. The rows are 20 px high, and the chip adds no height. The gutter shows the real line numbers. The error mark of a hidden line shows on the row that hides it. `Ctrl+Shift+]` unfolds all. The status messages are exact. Each new key reaches the page and `preventDefault` stops the browser. A key with `Alt`, and `Cmd+Shift+[`, are not taken. The keys use `event.code`, so a layout with another character works. |
| B171 | The folded view and the edit (10.3e). A typed letter, `Backspace`, `Delete`, `Enter`, a paste, an IME composition and a dead key each end the folded view, and the edit lands at the caret that the click mapped. One `Ctrl+Z` undoes the edit. The composition starts after the folded view ended. A drag over two chips copies the full text (read back from the clipboard). `Ctrl+A` and `Ctrl+C` copy all. The arrow keys, `Home`, `End`, `PageUp` and `PageDown` unfold and move the caret natively. A click on a chip unfolds one node. "Run" and `Ctrl+Enter` work while folded and send the same text. The case drives the IME with `Input.imeSetComposition`. A real dead key of a physical layout, and a real IME of an operating system, stay a manual check. |
| B172 | The format (10.3d). The button and the key, on ClickHouse, PostgreSQL and MongoDB texts that give a result of 20 lines, 500 lines and 501 lines. A result of 500 lines or less makes one undo step and one redo step. A result of 501 lines or more has the button "Undo format" and the keys `Ctrl+Z`, `Cmd+Z`, `Ctrl+Shift+Z`, `Cmd+Shift+Z` and `Ctrl+Y`. Any other input removes the own entry. A second format says "Nothing to change." and makes no undo step. The caret keeps its non-blank count. A selection formats its statements only, and a collapsed caret formats the whole text. A text with an unclosed bracket, an unclosed string, an unclosed comment and a JSON text that is not valid are refused with the line and column, and the caret goes there. The draft changes as for an edit, and the save is not blocked. |
| B173 | The block and the pair (10.3f). The band covers the lines of the innermost block, over the full width, and moves with the caret. It does not show while the person types, with a selection or while folded. Both brackets of a pair have a mark, and a bracket with no partner has the mark in `--danger`. A bracket in a string has no mark. `Ctrl+Shift+\` jumps, also from the folded view. A mark and a band move no glyph at the factors 1, 0.9, 1.1 and 1.25: the rectangle of each character is the same with and without them (the shift is 1/64 px at most, the unit of the layout), and the pixels of the colour layer are the same when the outline and the band are hidden. The case B57 runs unchanged for the selection. The contrast of each token colour over the band is 4.5:1 or more, and that of the bar and the mark is 3:1 or more, in both themes. |
| B174 | The caps and the errors (10.3c). A text of 262,144 bytes works, and a text of 262,145 bytes shows the notice `too_large`, with ASCII and with 2-byte, 3-byte and 4-byte characters. A text of 128 and of 129 levels. A SQL text that is far under and far over 100,000 nodes, and a JSON text of exactly 100,000 and of 100,001 nodes. 2,000 foldable blocks show 2,000 markers, and 2,001 blocks show 2,000 markers and the notice. In each capped case the colours stay, and the save and the run send the same bytes as with no tree. A hostile text (a very long line, 10,000 unclosed brackets, a lone surrogate, a NUL, a BOM, a string or a comment with no end, dollar quotes) gives no exception and no console error. |
| B175 | The access (10.3g, 9.26). The text area is the only accessible text, and it holds the full text while folded (the accessible tree has no other text). The layers are `aria-hidden`. The messages of the status line come in order. `Tab`, `Escape` then `Tab` and `Ctrl+Enter` work as before, and the focus never leaves the text area for a fold, a format or a jump. The markers have names and are not in the Tab order. The page has no animation and no transition with `prefers-reduced-motion: reduce`, and without it. The case runs scripted checks in both themes. It does not run axe-core, which needs the network (as in B14 and B82): the CI image runs axe-core. |
| B176 | The time and the budget (10.3c, 9.30). The parse of a text of 262,144 bytes takes 50 ms or less, and the format takes 300 ms or less. The folded view of a text of 19 lines draws in 100 ms or less. The folded view of a text of 262,144 bytes draws in 100 ms or less (the least of 7 runs, with two frames). A key in a text of 262,144 bytes takes less than 100 ms to the second frame after it (the least of 5 keys). At a key, the script of F42 takes less than 50 ms, the script of the colour layer takes less than 50 ms, and no task of the main thread takes 100 ms or more. The colour layer draws only the rows near the visible rows (fixed). The page measure of 9.30 has no byte of F42, and the request list of the dashboard page has no file of `editor/ast/`. |
| B177 | The kinds of source (10.3c). The panel editor of a ClickHouse source, a PostgreSQL source and a MongoDB source, the SQL of a variable, the SQL of an annotation and a panel with two queries (4.15a) each have the tool row. A text that only one dialect reads (`#`, `$$ ... $$`, `x::Int`, `arrayMap(x -> x, a)`, `$facet`) folds and formats under the dialect of its source. A change of the source changes the dialect, and each query of a panel has the dialect of its own source. |
| B190 | An `interval` control: the options with "Auto" first, a choice, the URL `var-<name>`, the keys, the listbox of 9.10. A reload keeps the choice. |
| B191 | "Auto" with a panel whose SQL shows the text through the SQL of "Inspect": the text changes with the time range (6 h gives `10m`, 5 min gives `10s`). The same text goes to two panels. |
| B192 | An `interval` with a `placement`: the control is in the grid, and the bar does not show it. |
| B193 | A `constant`: no control in the bar, no `var-<name>` in the URL, and the URL `var-k=zzz` does not change the SQL. The value is in the SQL of the panel. |
| B194 | The titles `${bucket}` and `${k}` show `Auto` or the option text and the value of the constant. |
| B195 | The editor adds each kind, edits the list, the "Auto" fields and the value, shows an error of 4.37, and keeps `hide` = `variable` for a constant. The JSON tab agrees. |
| B196 | The import of a Grafana dashboard with an interval list `1m,1.5h,1M` and a constant shows the report of 15.6. |
| B197 | A URL `var-bucket=7m` that is not an item is dropped, with the notice of 9.10. |
| B198 | The items and the value `<img src=x onerror=alert(1)>` (in the editor) show as text, and no script runs. The CSP report is empty. |
| B199 | A dashboard with `interval`, `constant`, `query`, `custom` and `text` variables loads, and a change of the interval runs only the panels that name it. |
| B200 | The forms of `timeseries`, `barchart` and `xychart` (10.3h). The groups and the labels follow the tables, in the order of the tables. A change of each control writes the member of the table with a valid value, and the data hooks show the effect: `data-series-style`, `data-axes`, `data-thresholds` and the pixels of the canvas for a time series, the bars and their labels for a bar chart, the marks and the lines for an XY chart. The rows of display hide and show as the tables say, and the status line tells it. An optional member leaves the draft when its value is `absent`. A value that the form does not know (`draw` = `x`) shows the error, and the form writes nothing. |
| B201 | The forms of `stat`, `bargauge` and `piechart` (10.3h). The same checks as B200 for the three types: the reduction (Show, Calculation, Fields), the text mode, the colour mode, the graph mode, the orientation, the display mode, the unfilled area, the pie type, the labels, the legend values and the legend sort. The test reads the DOM of each type (`data-align`, the text of the tiles, the class of the track, the slices) as the proof of the effect. An invalid pattern in "Fields" shows the error and writes nothing. |
| B202 | The forms of `table`, and of `heatmap` and `histogram` when they exist (10.3h). The test checks the header switch, the wrap, the sort list (add, move, remove, 8 items at most), the width, the alignment, the filter, the footer calculations (8 at most) and the cell type with its mode. The draft changes in the member of each control, and the preview shows the effect. The checks of the buckets, the log scale that writes `size`, the colour mode, the steps, the gap and the bounds are not in the case yet, because the page does not have the two types (F31). The pure rules of these rows are in the test FE-78. The case fails when the type list of the editor offers `heatmap` or `histogram`, so that the checks are added. |
| B203 | The keyboard and the screen reader in the options pane (9.26, 10.3h). For one panel of each type: the Tab order equals the visual order; a radio group has one stop; the arrow keys, `Home` and `End` move and write; the slider keys work; a switch and a check box take `Space`; each control has the name of its label in the accessibility tree; each choice and each check list is a group with a legend; the status line says the change; the focus stays on the control that changed, and goes to the group head when its row hides. |
| B204 | The look of the options pane (9.26, 10.3h). In light and dark, at 1400, 768 and 390 px, for one panel of each type: the page has no horizontal scroll bar; no control is wider than its pane; no label is cut; each target is 24 by 24 px or more; the text has 4.5:1 and the border of a control and the focus ring have 3:1; a pressed segment differs from the others by more than the colour. |
| B205 | The live preview (10.3h, rule 7). For a choice, a switch, a slider and a number field: the draft and the frame of the preview change in the same task; 50 `input` events of a slider in one frame make one write (`data-draft-writes`); 20 moves of a slider make 20 writes or less and each change is one task of 100 ms or less; the number field writes after 300 ms, and `Enter` writes at once. |
| B206 | The rules of display (10.3h, rule 15). For each row with a `when`, in a state where it is hidden, the member changes through the tab "JSON" and the picture of the preview does not change. The picture is the pixels of the canvas, the size of the plot and the legend, and the markup without the attributes `data-*` (the hooks write the options as text). In the state where the row shows, the member of the control is in the draft after a hide and a show. No member leaves the draft because its row hides. |
| B207 | The charts stay painted across the states of the grid (10.2a, rule 17). The case walks the switch "Edit" on and off, the panel editor from the edit mode and from the view, and "Back" and "Forward" over these states. A probe samples each frame after its paint, and a `MutationObserver` records each new canvas. The probe also samples at the end of the task that writes the hook `data-hune-view`, and the steps wait for states, not for a fixed time. While the grid shows, each frame keeps its plot and its canvas, each canvas has pixels, and no frame waits. No panel query goes out. The mark of an annotation hides in edit mode and shows again after it. The dashboard has a time series, a bar chart, a state timeline, a stat, a heatmap, a panel that repeats and a section that repeats, in both themes. The heatmap has its own fit: it must draw before the paint when its frame returns to the page. |
| B208 | The gap under the sticky header (9.6, 10.3). The content of each view with the page layout starts one token `--page-pad` under the header. The view mode of the dashboard is the reference. In the panel editor of each of the 12 types, the preview and the options pane start at least that far under the header. The settings view, the JSON view and the history view obey the same rule. The case runs at 1280, 1024, 768 and 390 px, in both themes, and at the browser zoom 90 %. The page has no horizontal scroll and no overflow. |
| B209 | The row of the query settings in the tab "Query" (10.3). At 1280 px, Source, Format, Name, Min interval, Max points, the button "Options" and "Run" are on one row: their tops differ by 2 px at most. At 768 and 390 px the row wraps and the page has no horizontal scroll. The popover "Query options" holds "Reuse a panel", "Relative time", "Time shift" and "Hide time info". It shows a dot while one of them is not at its default. The forms still write the members. The Tab order is the visual order, each control has a name, and each target is 24 px at least. |
| B210 | The charts of a section draw in the task that expands it (9.8, 10.2a rule 17). The case collapses and expands a section and the copies 1 and 2 of a section that repeats. It does the same in the draft grid of the edit mode. A probe samples each frame after its paint, at the end of each click task on a section toggle, and in the next task. The steps wait for states, not for a fixed time. After each expand, each frame keeps its plot and its canvas, and each canvas has pixels. A canvas with a bitmap less than half of its box is blank. No frame waits, and no panel query goes out. A section that is collapsed at the load sends no query. Its first expand shows the empty frames, sends one query for each panel, and then draws the charts. The dashboard has a time series, a bar chart, a state timeline, a pie chart, a heatmap and a stat, in both themes. |
| B211 | The data link tokens of Grafana 12 (9.18). A table has a link of the form of the OTel dashboard of the ClickHouse plugin: `${__url_time_range}`, `${env:queryparam}` of a multi variable, `${svc:queryparam}` with "All", and the value of the cell. Its href is the href of Grafana 12.4.2. Two more links give `${__value.time}` of a `DateTime64(9)` in whole milliseconds, `${__value.numeric}` of a number and of a text (`NaN`), `${__series.name}`, `${__timezone}`, `${__dashboard.uid}` and a `constant` in the form `queryparam`. A link `${q:queryparam}` of a text variable that holds `javascript:alert(1)` gives no link and no anchor. A click on the first link opens the same dashboard in the same tab, with the values of the link in the URL. |
| B212 | The time forms of Grafana in the URL (9.5, 9.9). The browser is in `America/New_York`. The URL of Grafana 12.4.2 with `timezone=Europe/Paris`, an ISO range and `orgId=1` shows the range in Paris on the range button, with no warning notice. The panel request has the epoch milliseconds of the link. After a shift back, the page writes epoch milliseconds, `timezone` after `to`, and no `orgId`. The compact form and the form with no zone letter give the same range in UTC. The milliseconds of the ISO form stay, and with no `timezone` the zone of the document applies. An offset and a bare date give the notice of 9.5 and the range of the document. An unknown zone gives the notice `zone-url` and the zone of the document, and the page drops the name at the next write. A known zone of the URL hides the notice of an unknown zone of the document. A zone with markup and a time of 4,000 digits give two notices that cut the values, and no element comes from them. |
| B213 | The drawer "Inspect value" of a table cell (9.20, 4.18). A column with `inspect = true` holds a JSON value of more than 10,000 characters, and a second column holds `<img src=x onerror=alert(1)>`. A column with no `inspect` has no button. The button shows on hover and on the focus of the cell, has the name "Inspect value" and is 24 px at least. A click opens the drawer with the JSON text and an indent of 2 spaces. The key `Tab` on a focused cell reaches the button, and `Enter` opens the drawer with the hostile text as text: no element comes from it. `Enter` on a focused cell opens the drawer too. `Escape` closes it, and the focus goes back to the button or to the cell. Each target of the drawer is 24 px at least. "Copy to Clipboard" writes the shown text. No script runs. |
| B214 | The entry redirect in mode `authzen` (9.7, 11.25). The policy of `fake-pdp` lets `solo` see one dashboard of six. `GET /api/dashboards` gives six items to `admin` and one item to `solo`. `solo` opens the list page and goes to this dashboard. The tab loads one document, `history.length` does not change, and no row or empty state of the list enters the DOM. The case then removes the guard of 9.7. A click on the brand link shows the list with one row, and no redirect follows. "Go to the list" on a refused dashboard does the same in a new tab. After a delete of the item that is not valid, `reader` sees two dashboards and gets the list. Without the ProcurAuth tools, the case is `skip`. |
| B224 | The cosmetic series options (4.18, 4.28, 9.19). The values come from the probe of Grafana 12.4.2 of 2026-11-22. The dash arrays `10, 20` and `0, 20` reach the canvas as they are, with a butt cap and a round cap. A log scale of base 2 over the values 1 to 910 has the labels 1, 4, 16, 64, 256 and 1024. A null threshold of 150000 ms joins a gap of 2 minutes and not of 3, so the line has 3 runs and not 4. A bar with `before` ends half its line before its time, and `after` starts half its line after it. The centred zero gives -60 to 60 for the data 10 to 50. An axis width of 80 puts the plot 80 px from the left. A border and the colour `series` give labels 9 px from the plot, a line along the plot and a mark of 4 px at each label, in the colour of the series. A bar chart gives the value axis -48 to 48 and a gutter of 70 px. |
| B223 | The gradient modes of a time series and of a bar chart (4.28, 4.30, 9.19), in both themes. The values come from the probe of Grafana 12.4.2 of 2026-11-22. `opacity` gives one fill gradient from the top of the plot at the alpha 0.5 to its bottom at the alpha 0, and a plain line. `hue` gives #9AF3AE at the top and #719A42 (dark) or #8AB757 (light) at the bottom, for the fill and the line. `scheme` gives the bands of the thresholds and the stops of Green-Yellow-Red. The mode `fixed` keeps the plain colour. Bars and bar charts have one gradient over the plot at the alpha 0.61. A horizontal bar has one CSS gradient over the track. |
| B216 | The fill, the line, the grid lines and the orientation `auto` of a bar chart (4.18, 4.30, 9.19). The values come from the probe of Grafana 12.4.2 of 2026-11-22. The canvas of `fill_opacity` 61 has the fill `rgba(255, 0, 0, 0.61)` and a line of 1 px in the plain colour. `fill_opacity` 0 gives no fill call, and `line_width` 0 gives no stroke call. The box of a bar at the line width 4 is the box at 0 less 2 px at each side. `axis_grid` = `false` removes the grid lines and keeps the labels of the value axis. `auto` draws horizontal bars in a plot narrower than it is high and vertical bars in a wide plot. A wider page draws the chart again with vertical bars, and the first width draws the horizontal bars again. The horizontal bars have the fill alpha 0.61 and a border of 3 px inside the box, and grid off keeps the ticks. |
| B215 | The new units (4.20, 9.15). The server takes a document with the new unit ids. A `stat` with the unit `string` shows `1234.5678`. A table shows the texts of Grafana 12.4.2 for `bool`, `bool_on_off`, `kbytes`, `rowsps`, `dtdhms`, `sishort` and `decmbytes`, and `true` or `false` for a `bool` value. Examples are `True`, `Off`, `1.50 MiB`, `0.500 rows/s`, `1143 d 02:50:32` and `00:00:42 ago`. The cells hold text only. |
| B222 | The percent change and the text sizes of a `stat` (4.29, 9.19). The case runs in both themes. A stat from 100 to 120 shows `120` and `20%` with an arrow up in green, and from 120 to 100 `-16.7%` with an arrow down in red. A change of 0 shows `0%` in red with no arrow. A first value of 0 and a last value of 0 show no change. `inverted` and `same_as_value` give the colours of FE-82, and a filled tile gives its ink. The case checks the sizes, the places and the colours against the values of Grafana 12.4.2 in stacked and wide tiles and in the text modes `value_and_name`, `name` and `none`. `title_size` and `value_size` give the exact font sizes. An item of each row shows no change. The texts are text nodes. |
| B217 | Annotations from a PostgreSQL source and a MongoDB source (9.19d). A dashboard has a time series and one annotation on each kind, on disposable PostgreSQL 18 and MongoDB 8.0 servers. The page sends one annotation request for each, with no query text in the body. The hook `data-annotations` of the panel has the two regions of MongoDB and the two points of PostgreSQL, in the order of the time and with the colour of each annotation. The card of each kind shows the time, the title, the text and the tags. The tab "Annotations" of the settings offers the sources of the three kinds. The SQL area of the MongoDB annotation formats its text as JSON, and the SQL area of the PostgreSQL annotation formats its text as SQL (10.3c). The case is a `skip` when the build has no MongoDB driver (8b.2). |
| B218 | The ad hoc filters (9.10a, 4.10c). A server of the case has `adhoc_columns`. The form of the bar offers the permitted keys of 6.14 and the operators `=` and `!=`. A filter added with the form runs panel 1 (the macro) again and not panel 2, with the closed request member. The address holds the item of Grafana, two filters give two items in order, and a remove runs panel 1 again and gives the focus to the plus button. A link of Grafana 12.4.2 with `%21%3D`, a label after `,` and `__gfp__` opens with its filters. A hostile value matches no row. |
| B219 | The refusals of the page for the ad hoc filters (9.10a). An address with a key that is not permitted, the operator `>`, a control character, two parts and 17 valid items keeps 16 filters and shows the notice with the dropped items. The address then holds 16 items, and the plus button is disabled. The form refuses a value of more than 1,024 bytes, with the error, and sends nothing. A server whose list holds one key gives that key alone in 6.14, and an item of another key is dropped. |
| B225 | The gauge (4.33e, 9.19g). A dashboard has eight gauges: three sizes, threshold labels, a percent field with a neutral value, a grid of five, a row of three, a column of six, a query with no row, and a fixed colour with a hostile field name. In both themes, the record `data-gauge` of each gauge equals the rules of 9.19g for the box of the page and the widths of its font, within 0.01 px. The pixels show the filled arc, the empty arc, the band of the thresholds and no paint inside the arc. The canvases hold the value texts, the labels and "No data". The column of six keeps 75 px and scrolls. A title and the description are text nodes, and the panel has no image element. The colours of the light theme are the light shades. |
| B180 | A table with `organize` (rename, hide, order), then `sort_by` and `limit`. The header shows the new names, the order and the cut. "View data" shows the same. |
| B181 | A stat and a table with `calculate_field` in the four modes. The new field shows with its default name, its alias, and with `replace_fields`. |
| B182 | A table with two queries and `merge`. One frame shows, with no select of frames. A `merge` of a panel with one query does nothing. |
| B183 | A time series with two queries and `join_by_field` (`null` key, `outer`). One chart draws, with the legend `A a` and `B b`. The mode `inner` removes the rows of one query. |
| B184 | A time series with a long table and `partition_by_values`. One series draws for each value. A table shows each frame in its select with the option text of 9.22a. |
| B185 | A `join_by_field` of three frames with 99,000 rows each shows the error of the cap (rule 13). The query limit of 100,000 rows bounds each frame, so two frames cannot go above the cap. A `partition_by_values` of 150 values shows the error of the frame cap. Both leave the other panels as they were. |
| B186 | The editor adds each of the eleven kinds, edits its fields, moves it, removes it, and shows an error of 4.37. The JSON tab and the draft agree. |
| B187 | The import of a Grafana dashboard with the eleven ids shows the report of 15.6 and the panels with the new kinds. |
| B188 | A `reuse` panel with `with_transforms` applies the new kinds of its source and of its own, in this order. |
| B189 | The names `<img src=x onerror=alert(1)>` and a name with `$` as a rename, an alias and a converted value show as text. No script runs, and the CSP report is empty. |
| B220 | The logs panel (4.33f, 9.19h). In both themes: the time in the zone of the dashboard with milliseconds, the level texts and the colours of Grafana 12.4.2, lines of 28 px with a text of 14 px on 22 px, 12 px between the columns, a level column of the width of the longest level text, newest first and oldest first, wrap (several rows, no side scroll) and no wrap (one row and a side scroll), and the plain items with `details` = `false`. The details row opens under the line with a click and closes with a second click; the arrow keys, Home and End move the one tab stop, and Enter opens the details row. |
| B221 | The logs panel and its threat T104 (4.33f, 7.16e, 9.19h). Hostile bodies, levels and field values stay text: no element and no event attribute come from the text, and no script runs. 10,001 rows give 10,000 lines and the note of the cap of lines. 201 bodies of 20,000 characters give 199 lines of 10,000 characters and the sign of the cut. A query with no row shows "No data". A frame with no text column shows the message. The save of a table with the format `logs` gives `422` with `not_permitted` at the pointer of the format. |
| B226 | The map with no tile source (9.19i, 12.8). The header of each kind of answer is the header of 12.8, byte for byte, and the session member `geomap` is `null`. A map of four points and one bad row draws the plain background and four markers, with the note of the skipped row. The pixel of a marker has the fill of `dark-green` at the alpha 0.16. The view `fit` frames the points. The card of a marker shows a hostile name as text, with no element and no script. The buttons and the keys change the zoom, and a button is off at zoom 0. No request of the page leaves the origin of the page. |
| B227 | The map with a tile source on 127.0.0.1 (9.19i, 12.8, 13.12c). The case starts a server with a block `geomap`, a tile server of its own with TLS on 127.0.0.1, and a Chrome of its own that maps the test name `tiles.hune.test` to 127.0.0.1 and gets no other host. The header has the tile source in `img-src` only, and the session gives the three values. Each tile request has a path of three whole numbers, no `Referer`, no cookie (a cookie of the test host exists) and no authorization. The page has 6 requests in flight at most. A pan makes no request, and the first request comes 200 ms after the release. A host with no CORS header gives no tile and no second request in the view. A tile of 512 px is dropped. The attribution is text. With `refresh=10s` and no CORS header, the host gets one request for each tile of the view in 25 s, through three runs of the panel. A pan survives a refresh, and the new draw asks for no tile. A map of 24 tiles of 256 px with 16 MiB of padding each grows the renderers by less than 64 MiB (security review 3, findings F1 and F5). |
| B228 | The cards of the list page (9.7). A server with the default threshold and three dashboards in two folders shows no sort control and no tag filter. The page has no title and no counter, and no word about the access. Two folders give two folder titles in capitals, with a menu of the folder. A card is 400 px wide. The title is on one line with an ellipsis and the full name in its `title` attribute. The block of cards is centred, across and down, in a window that is taller than the block. In a short window it starts at the top and the page scrolls. At 390 px there is one column. A card that Hune never saved has no badge of the last change. A card that Hune saved has the badge `Last change <age>` at the top right, and the cards keep one height. A writable card has the three dots at the top right, after the badge, and no other button. The menu has "Edit", "Export", "Move" and "Delete" in this order. "Edit" links to `?edit=1` and opens the edit mode. "Delete" opens the delete dialog with the name, and nothing is deleted. A threshold equal to the count hides the filters, and a lower threshold shows them. With the filters hidden, `q` has no effect. A server with editing off shows a card with no three dots. The sort control and the tag filter open a menu that touches the button, they open with the key Down, they mark the chosen line with a check, and Escape gives the focus back to the button. The tag menu has a chip and a count on each line. |
| B229 | The marks of the edit mode on the panel frames (10.16). The view has no resize handle (`.panel__resize`), no `data-drag-handle` and no `aria-keyshortcuts`. In edit mode each panel has one handle that shows, one drag mark on its title bar and one set of key shortcuts. After the switch "Edit" goes off, after "Discard" in the exit dialog, and after "Revert all changes" followed by the switch, none of them stays on the frames that the view grid uses again. After "Revert all changes" the edit mode goes on with one handle for each panel, and no handle is made twice. A new entry gives them again. |
| B230 | The id of a new folder (10.7, 10.12). The dialog "New folder" has the field "Title" only. The page makes the id from the title: "Équipe réseau" gives `equipe-reseau`. The same title again gives an id with a dash and four characters, and the two folders have the same title. A title of symbols gives `folder-` and four characters. The dialog "Import" has the last entry "New folder..." in the select of the folders. The entry shows a field for the title. The folder is made when the user chooses "Import", and the dashboard is in it. |
| B231 | The range and the refresh in edit mode (9.5, 10.2a, rule 12). A person chooses "Last 24 hours" and the refresh "30s" in edit mode. After the switch goes off, the controls and the address still hold them. The same holds for the entry by `?edit=1` and for a save that has no default box. The saved defaults do not change. |
| B232 | The defaults from the Save dialog (10.9). With no difference, "Save" is not active. A different range makes "Save" active and "Revert" not. The dialog shows one box for each value that differs, both off at first, with the hint of the current value and the default. The range alone, then the refresh alone, change only their own members of the saved document. The address then names only the values that still differ. |
| B233 | The view of one panel has no page scrollbar (9.8). At 1440x900, 1280x720, 768x600 and 390x844, in the light and the dark theme, for a dashboard without variables and a dashboard with variables: the document and the body are not taller than the window, and no vertical scrollbar takes room from the page. Without variables the panel starts one page padding under the header and ends one page padding above the window edge: 16 px, and 12 px at 390 px. With variables the panel ends at least one page padding above the window edge. |
| B240 | The JSON view of the edit mode (10.4). The text has colours: a key, a text, a number and the punctuation each have a colour, in the light and the dark theme, with 4.5:1 or more on the area. The bars of the gutter have 3:1 or more. The text area is transparent, and the first row of the layer is the first line of the text. A draft with no change has no mark. A changed line gets `modified`, two new lines get `added`, a deleted line leaves a mark at the border, and a syntax fault marks the line of the list of errors. A revert gives back the saved text and clears the marks. The area ends one page padding above the edge of the window, and the page has no scroll, at 1440x900, 1280x720, 768x900 and 390x800, with and without the list of errors. A text of 6,000 lines draws a window of rows and not all of them, a jump draws the row at the top, and a text of one line has 2,000 coloured parts at most. |
| B241 | A read-only store (9.7). A server with a primary volume and a read-only volume `lab`. The list shows the store as a root group with a lock and its title. Its folders are under the title. Each card of the store has the lock and no three dots, and all cards have one height. The menu "New" lists New dashboard, New folder and Import. The dialog "New dashboard" lists the folders of the primary only. |
| B242 | A file of the read-only store that is not valid after the start (9.7, 9.8). The API gives `read_only`, `store` and `stale`, with no path. The card has the warning mark and the lock. The page has the warning notice, the lock and no switch "Edit". The mark goes when the file is valid again. |
| B243 | The header selector (9.6). The read-only store is a row with a lock. A click opens its two folders, and a click on a dashboard of the store opens it. |
| B244 | A write to the read-only store (6.8). The API gives `403 read_only_store` for a create, a save and a delete, and the primary accepts a write. The page of the dashboard with `edit=1` has no edit control and has the lock. |
| B260 | Select and copy a legend (9.8e). A drag over the text of a legend item selects the text and toggles no series. A plain click toggles the series, and a second click gives the first state again. A triple click selects the text. Ctrl+C on a focused item with no selection copies its text. |
| B261 | Select and copy a link (9.8e). A drag over the text of a link in a table cell selects the text and does not follow the link. A plain click follows the link. |
| B262 | Select and copy the text of a chart (9.8e). The line chart, the bar chart and the pie chart each have a layer of 1 to 400 texts that is hidden from assistive technology. A drag over the labels of the vertical axis selects them, and a copy gives one label on each line. A triple click selects a text of the pie chart. The tooltip shows over the plot, and the point of the plot is not under the layer. |
| B263 | Select and copy a header (9.8e). A drag over a sortable header selects its text and does not sort. A plain click sorts. A drag over two cells selects text. |
| B264 | Select and copy the lines of a logs panel (9.8e). A drag inside one cell uses the copy of the browser. A drag over a line, over two lines and from the middle of a cell copies one text line for each log line, with one space between the cells. A triple click selects the whole line and leaves the details closed, or open when they were open. A double click leaves the details as they were. A plain click toggles them. Ctrl+A with the focus on a line copies the six lines. |
| B265 | Select and copy the rows of other panels (9.8e). A triple click and a drag over two lines of a logs panel with wrap copy one text line for each line. A drag over table cells uses the copy of the browser, with a tab and a line end. The rows of the horizontal bar chart and of the bar gauge copy as the name and the value on one text line. |
| B266 | The cursor over text and the right button (9.8e). The cursor is the text cursor over a log line, a legend label, a table cell, a panel title and an axis label, and the pointer over a sortable header and a legend item. The right button on selected log lines, a legend label, axis labels and table cells keeps the selection and toggles no line, no series and no sort. No code of the page stops the menu of the browser. |
| B270 | The look of the JSON view (10.4). In the light and the dark theme, each kind of text and each depth of bracket has the colour of its token. The editor, the line numbers, the selection with and without focus, and the scroll bar of 10 px have the values of the palettes. The layers take no pointer. |
| B271 | The caret in the JSON view (10.4). A click puts the band and the strong number on its line. A caret that touches a bracket puts a box on it and on its pair, and the guide runs along the lines between them. A selection has no band and no box. A line with six spaces has a guide box of six columns and one row high. A syntax error has a wavy underline under its token, and it goes when the text is valid. |
| B272 | The layout of the page (9.6). At 1440x900, 1280x720, 768x900 and 390x800, in the light and the dark theme, on the list, on a dashboard, in the view of a panel, in the edit mode, in the panel editor and in the JSON view: the header has the width of the window and is at the top, `#app-main` starts under it and is the one scroll container (positioned, `overflow: auto`), the document has no vertical scrollbar and is not taller than the window, and nothing scrolls on the x axis. A low list scrolls in the container and the header keeps its place. The row of the variables stays under the header after a scroll. A drag of a panel near the bottom edge scrolls the container and not the document. |
| B290 | The loading state of a variable (9.10). The options requests are held. The control of one value and the control with chips show a spinner and "Loading…", are disabled, and have `aria-busy="true"` and the name `<label>: Loading…`. After the answer, the height of the bar and the top of the first panel are the same as before. |
| B291 | The error of the options (9.10). A variable with a table that does not exist and a variable that goes over the memory limit fail. The control has a red edge and keeps its value. The popup gives the short sentence for the ClickHouse number 60 and 241, a button "Retry", and a folded detail with the full text. No `select__error` is in the bar. The bar and the grid do not move. "Retry" sends one request for this variable. |
| B292 | The variable that waits (9.10). A variable below a failed parent sends no request and shows "waiting for <label of the parent>". The panel runs with the default value. A variable below a parent that loads sends no request before the answer of the parent, and sends one after it. |
| B293 | A failure blocks nothing (9.10). A failed variable with "All" and a failed open variable keep their value, and the URL has no default. The panel runs with these values. A typed value in the popup of the failed open variable goes to the URL and to the panel. |
| B294 | The cache of the options (9.10). One request for each variable at load. A refresh in the time of the cache sends none. A new quick range sends one request, for the variable that follows the period. A return to the first range sends none. |
| B295 | The cancel of the options (9.10). A refresh cancels the request in flight of the same variable. A click on the brand cancels the request that runs. |
| B296 | The chips (9.10). One chip for each value, and the name of the control keeps the text of the selection. A click on a chip opens the popup. A click on a cross removes the value, and the URL and the panel follow. `Enter` on a cross removes. The last chip and the chip "All" have no cross. `Backspace` in the field of an open variable removes the last typed value. At 390 px, the chip "+<n>" shows the count of the hidden chips and stays in the row. |
| B300 | The list page with dashboards with no folder (9.7). The root group of the primary is first and has no title. The folder below it keeps its title. The root group of the read-only store is the first group under its title, with a lock and no menu. GET `api/folders` has the member `root` and no root item. |
| B301 | The dialog New dashboard (10.7). Its select has "Root (no folder)" first, and the folders after it. A dashboard that the dialog makes has the folder `null` in the API, and it shows in the root group. |
| B302 | The dialog Import (10.12). Its select has "Root (no folder)" first. A file that the dialog imports there has the folder `null`. |
| B303 | The dialog Move (10.7). A dashboard in a folder can move to the root. A dashboard at the root has no root entry and moves to a folder. The API shows both moves. |
| B304 | The header selector (9.6). The dashboards with no folder are lines on their own, first, then the folder row and the store row. The root dashboard of the read-only store comes before its folders. |
| B305 | A server with no primary store (11.25). The member `root` is read-only and belongs to no store. The root dashboards show in the block of the store with a lock and no menu, and no New button shows. |
| B320 | The preview of each panel type (9.11). A dashboard has one panel of each of the 14 queried types, with the queries held. Each preview shows the family of its type, hidden from assistive technology, with the light. A family that has no shapes shows the neutral form: no shape and no claimed part. The harness releases the queries and reads each loaded panel. The box of the preview is the box of the loaded panel, and no claimed part differs from the loaded panel. |
| B321 | The light and the reload (9.11). The light has the animation `panel-skel` and a block display. With the reduced motion, the animation is `none` and the display is `none`. The preview uses neutral tones in the light and the dark theme. After the load, a reload has `data-wait="reload"` and shows no preview. |
| B400 | The circle of the loaded pie chart (9.19a, 9.29). A dashboard has three pie charts: a legend at the right, a donut with a legend at the bottom, and a pie with no legend. At five widths and zoom levels (1000 px, 1283 px, and 1183, 1011 and 1366 px at the zoom 1.1, 1.25 and 0.9), the box of a canvas has a fraction of a pixel. The hook `data-geometry` gives a centre at half of the real rect and a radius of half of the shorter side less 8 px, within 0.02 px. The donut has a hole of 0.55 of the radius. The case fails if no box has a fraction. |

A build made with `--no-default-features` has no MongoDB driver and refuses the `mongodb` block at start (8b.2). The runner then reports B47 and B48 as `skip`, with the reason "no MongoDB driver in this build". It detects this case from the exit status and the message of `hune check-config`. A build with the driver (the default build) runs both cases against a disposable MongoDB 8.0 with `openssl` and `docker`.

On a failure, the job keeps the screenshots, the traces and the console log of the failed case as CI artefacts. The tests
use no pixel baseline.

#### 16.10a Front-end units

The pure functions of the browser have their own layer: the functions of 9.9, of 9.12 to 9.23 and of 10.15. The job
`frontend` runs `node --test tests/frontend/` in the browser test image (16.10). It needs no browser and no binary.

- Each function has golden vectors in `tests/frontend/vectors/<function>.json`: an input, the options and the expected
  output. Section 9 gives the id of each vector set.
- The functions include these:
  - the date math and the time zones (9.9), and the frame to fields (9.12);
  - the display names (9.13), the field configuration and the matchers (9.14);
  - the units and decimals (9.15), the colours and thresholds (9.16), and the value mappings (9.17);
  - the data-link tokens (9.18), the transformations and the long-to-wide (9.21, 9.22), and the panel reuse (9.23);
  - the line diff (10.15).
- The vector sets FE-06 and FE-14 hold the plural time words of 9.15 and the series order of 9.22. The tests read the vector files as they are. They keep no list of deviations.
- These cases are in the sets that exist:
  - FE-06: the 21 new units. Each one has a small, a large, a negative and a zero value, and a case with `decimals`.
  - FE-06, the 30 units of 4.20: the file `FE-06-units-h.json` and the test `fe06_units_h.test.mjs`. The file holds the texts of Grafana 12.4.2 for the 30 new units of 4.20. It has 30 values and the decimals absent, 0 and 2, from the measure of 2026-11-22.
  - The same file holds the display of a number, a text and a `bool` value for each unit. The member `grafana` marks the 20 cases where the page differs on purpose (19.20n). The test also compares the 59 ids of 4.20 in four places: docs/02, `hune-core`, the check of the editor and its unit list.
  - The test `barchart_p03.test.mjs`: the bar chart and the engine edit E34 on a fake of the canvas. The fill has the alpha `fill_opacity` / 100, and the line has the plain colour at `line_width`, as in the probe of Grafana 12.4.2. The values 0 give no call, and each side of the box of a bar moves in by half the line width. `axis_grid` = `false` removes the grid lines in both orientations. The rule of `auto` is `width < height`, and a resize across the rule draws the chart again. 9 of its 10 tests fail before the change.
  - FE-04: the variables in titles. FE-05: the matcher `names` and the property `hide_from`.
  - FE-10: the new calculations. This includes the example of `delta` in 9.19 and a field with `null`, `NaN` or no row.
  - FE-11: the footer lines.
  - The test `table_inspect.test.mjs`: the text of the drawer "Inspect value" of 9.20. It holds the raw text, the indent of a JSON text, the depth limit, and the limit of 1,048,576 code units with its note. It also holds hostile values with markup in a fake `document`.
  - FE-01: the zone names (`isKnownZone`, a clock change, the label of 9.9).
  - FE-01, `url_state.test.mjs` and `runtime.test.mjs` (9.5): the three time forms of Grafana in the URL and the bounds of 4.4. They also hold the forms that Grafana reads as 1970 and the dates that do not exist. They hold two `timezone` names and a zone of the URL in the runtime.
  - The same tests hold hostile texts of 1,000,000 characters and the cut of the error text. The time is the minimum of 7 runs after 2 warm-ups.
  - The tests `editor_props.test.mjs` and `editor_forms.test.mjs`. The property catalog (4.18) agrees with `tests/fixtures/dashboards/f2_members.json`. Each form writes only members of section 4.
- These eight vector sets exist:
  - `FE-18`: the CSV file of 9.11.
  - `FE-19`: the sort of the legend table and of the tooltip rows (9.19).
  - `FE-20`: the lines and the bands of the thresholds (9.19).
  - `FE-21`: `formatTime` of 9.19 in each zone.
  - `FE-22`: the range of a panel with a time override and its note (9.8b). It holds the cases of `time_from` and `time_shift`. It holds an absolute range of the dashboard, a clock change, a range below 0 and `now/d` at the first millisecond.
  - `FE-23`: the colour modes (9.16). It holds the 36 hex values of the series palette. It holds the stops and the interpolation of each continuous mode in both themes, and the values `null` and `NaN`.
  - `FE-24`: the axis plan (9.19). It holds the side for `auto`, `left`, `right` and `hidden`, three units, the range of a scale with hard and soft ends, and the title.
  - `FE-25`: the URLs and the lists of the dashboard links (9.8a). It holds the tokens, `include_vars`, `keep_time`, a fragment, a refused URL, the filter by tags, the current dashboard and the order.
  - `FE-26`: the states of the page (10.2a). The file `tests/frontend/vectors/FE-26-view-state.json` has three sets of vectors.
    The first set holds the URLs of rule 1: each level, two names, three names, an unknown id, an unknown value, `panel-<id>` and no right.
    The second set holds the rows of the second table of 10.2a. Each vector has the next state, the history action, the origin and the dialog. The third set holds the leave target for each origin.
  - `FE-27`: the repeat (4.13a, 9.8c). The file `tests/frontend/vectors/FE-27-repeat.json` has six sets.
    - The first set holds the values. It has the order of the options, "All", a subset, a `custom` variable and a `query` variable. It has a value that is not an option, and a selection that is the same set in two orders.
    - The second set holds the states and the limits. It has 0, 1, 24 and 25 values, and a section with 12 and with 13 values. It has the walk of the budget of the dashboard for three layouts of repeats.
    - The third set holds the layout. It has 40 vectors of direction `h` (k from 1 to 25, `max_per_row` from 1 to 24 and absent) and of direction `v`. It has the moves of the other panels, with the vectors that match the measures of Grafana 12.4.2.
    - The fourth set holds the keys and the URL forms: `<id>`, `<id>-r<n>`, `panel-<id>-r<n>` and `<x>$panel-<id>`. It also holds the values that the page refuses, such as an id `0` or an id that starts with a zero.
    - The fourth set also holds the copy that the form `<x>$panel-<id>` of Grafana names (10.2a, rule 1). It has text values, number values, the first copy and a section copy.
      It has texts that name no copy. These are an index, the option text, another case and a value that is not selected. It has a value with markup and a value with the characters of a pattern.
      A test checks that the form waits while the values load (rule 16).
    - The fifth set holds the request. It has the member `variables` of each copy, with the repeat variable fixed.
    - The sixth set holds the queries of a trigger, with a fake API. k copies give k requests. 6 requests at most are open at the same time. The states `repeat-empty` and `repeat-limit` give no request. A repeat of an unknown variable, or of a variable that cannot repeat, gives `repeat-empty` and no request. A limit of the session that is present and not valid gives `repeat-limit` and no copy. It also covers the abort at a change, and the count after a scroll.
  - The test `tests/frontend/repeat.test.mjs` runs `FE-27`. It checks that `repeat.js` has no DOM access and that its default constants equal the table of 4.35. The limits in force come from the member `repeat_limits` of the session (6.10), and the module reads them as an argument. It checks that the functions run in 50 ms or less for 200 panels and 120 copies. It checks that the module is in the list of the pure layer (`static_rules.test.mjs`).
  - The tests `editor_forms.test.mjs` and `view_state.test.mjs` get the cases of the group "Repeat options" and of rule 16 of 10.2a.
  - `FE-28`: the grammar of the Markdown (9.19b). The file `tests/frontend/vectors/FE-28-markdown.json` has 7 sets.
    - The sets are the blocks (140 vectors), the inline constructs (140 vectors) and the links with the autolinks (60 vectors, with the check of `checkUrl`).
    - The other sets are the tables, the tight and loose items, the limits and the escapes. Each limit has a vector at the maximum and a vector one above.
    - Each vector has the lines and the expected tree. The test helper `serialize` writes a tree as text, with the escape of `&`, `<` and `"`. It is not part of the page.
  - `FE-29`: the hostile vectors. The file `tests/frontend/vectors/FE-29-markdown-hostile.json` has 80 vectors.
    - They are the 40 payloads of B103 and a list of mutation attacks.
    - They are also the destinations with a space, a control character, a bidi character, a zero-width character, `%0a` or a backslash. Other vectors have a link with `"` or `<`, and a text with U+0000.
    - For each vector the test builds the DOM with a fake `document` and checks the elements, the attributes, the destinations and the text.
  - `FE-30`: the variables in a text (9.19b). The file `tests/frontend/vectors/FE-30-text-vars.json` has the forms of the table: a single value, several values, "All" with and without `all_value`, and a `text` variable.
    - It has the names that stay as written and the escape `\$`. It has the places that change: a leaf, a code span, a code block and a cell. It has the place that stays as written: a destination.
    - It has the hostile values of B104. For each one, the tree before and after the replacement has the same shape.
  - `FE-31`: the states and the segments (9.19c, sections 2 to 4). The file `tests/frontend/vectors/FE-31-timeline-states.json` has 5 sets.
    - The sets are the states of a value, the merge, `span_nulls`, the gaps and the cells. The states cover the mappings, the colour modes, the step texts, the empty text, bool, null and NaN.
    - It has 20 vectors that come from the measure of Grafana. Each one has the frame, the options, the x range, and the expected segments (x and width in pixels, 1 px tolerance), colours and texts.
    - The other vectors give the cases where Hune differs on purpose: the merge by state, the mapping before the step, and the bool colour.
  - `FE-32`: the geometry (9.19c, sections 6 and 7). The file `FE-32-timeline-geometry.json` has the rows for 1 to 40 rows and for each `row_height`. It has the left gutter for 20 names and the cell width for each `col_width`.
    - It also has the number of characters for 60 widths, and the ink for 40 fills in both themes. The vectors for the characters have the measured pairs of widths and counts.
  - `FE-33`: the caps and the hostile texts (9.19c, section 5, and T58). The file `FE-33-timeline-caps.json` has the counts at 9,999, 10,000 and 10,001. It has the rule of half the width at 491 and 492 rows for a width of 982, and the message texts.
    - It has 30 hostile state texts. The test builds the legend and the card with a fake `document`. It checks that each text is a text node and that no element comes from data.
  - `FE-34`: the duration text and the legend (9.19c, sections 9 and 10). The file `FE-34-timeline-text.json` has 60 durations from 0 ms to 3 years and the legend for each colour mode.
  - The tests are `tests/frontend/timeline_model.test.mjs` (`FE-31`, `FE-32`, `FE-34`) and `timeline_dom.test.mjs` (`FE-33`). A test also runs the model for 10,000 and 100,000 samples and checks the time (50 ms and 200 ms on the reference machine).
  - The test `static_rules.test.mjs` lists `timeline_model` in the pure layer. It checks that the constants of `timeline_model.js` equal 9.19c.
  - The tests `editor_forms.test.mjs` and `editor_draft.test.mjs` get the cases of the groups of 10.3 and of the type change.
  - `FE-35`: the annotations of a panel and the check of an answer (9.19d, rules 3 to 5). The file `tests/frontend/vectors/FE-35-annotations.json` has 4 sets.
    - The sets are `forPanel` (enabled, `panels` absent, `panels` with and without the id, a `reuse` and a copy of a repeat), `readAnswer` (30 answers: 10 that are valid and 20 that are not), `marks` (the order, the tie, the range) and the cap (1,999, 2,000 and 2,001).
    - It also has the check that the constants of `annotations.js` equal 4.35.
  - `FE-36`: the hit test and the geometry (9.19d, rules 7 to 10 and 13). The file `FE-36-annotations-geometry.json` has 40 vectors. They give the x of a point and of a region for given scales, and the hit zone. They give the nearest mark at 0, 8 and 9 px and a hit inside the width of a region. They give the place and the flip of the card.
  - `FE-37`: the card and the keys with hostile texts (9.19d, rules 11, 12 and 15, and T61). The file `FE-37-annotations-dom.json` has 40 hostile texts. The test builds the card, the status text and the second table of "View data" with a fake `document`. It checks that each text is a text node and that no element comes from data. It checks the keys and the head text for a point and a region.
  - `FE-38`: the queries of a panel (4.15a, 9.8d). The file `tests/frontend/vectors/FE-38-queries.json` has 4 sets. The first set holds `queriesOf` and `visibleQueries` (one query, three queries, a hidden first query, a first query with `ref`, all hidden). The second set holds the body of a request (no `ref` for one query, a `ref` for a panel with extra queries). The third set holds the state of a panel for 30 combinations of the states of 3 slots (9.8d). The fourth set holds the list of the failed refs and the text of the badge.
  - `FE-39`: the join (9.22a, rules 6 to 8). The file `FE-39-join.json` has 40 vectors. They cover two and three frames, equal times and shifted times. They cover a frame with one row, a frame with no row, an unsorted frame and a frame with NULL times. They cover equal times in one frame and a frame with no time column.
    - They cover the kinds `number`, `number_text`, `text` and `bool`, and labels. They cover the cap at 200,000 and 200,001 rows.
    - A test also runs the join for 8 frames and 200,000 rows. It checks the time: 150 ms on the reference machine.
  - `FE-40`: the combination by type and the names (9.22a, rules 4 and 5, 9.13, 9.14). The file `FE-40-combine.json` has 30 vectors. They give the fields of each type for 1, 2 and 3 frames. They give the prefix when a query failed, and the display names with labels. They give the matcher `query` on a joined frame and on a collected list. A table has the first frame, and the choice that stays or goes back.
  - `FE-41`: the scheduler (9.8d). The file `FE-41-slots.json` has 20 sequences of events. The events are a time range, a refresh, an abort, a `409`, a retry and a hidden query. They also include a change of a variable that one SQL names, and of a variable that two SQL name.
    - Each vector gives the requests that the runtime starts, in order, and the state of the panel after each event.
    - A test checks two facts. The runtime never has more than 6 requests at the same time. It never starts a request for a hidden query.
  - `FE-42`: the grid of raw values (9.19e, section 2). The file `tests/frontend/vectors/FE-42-bucket-raw.json` has 5 sets.
    - The sets are the time size, the value size, the log rows, the counts, and the refusals. The data set is the 20,000 values of the measure, saved with the vectors.
    - It has 36 vectors that come from the measure of Grafana. They are the 6 grids of example 1 and 2 (columns, rows and cells that have a count: 126, 37, 383, 272, 101 and 494), the nice size for each option, and the counts of the values on an edge.
    - The other vectors give the cases where Hune differs on purpose. They are a `null` value and a `NaN` value (both skipped). They are a time outside the range (ignored) and equal values (size 1). The last case is a value of 0 or less on a log scale (skipped, with the note that gives the count).
  - `FE-43`: the sizes (9.19e, section 2, and 9.19f, section 2). The file `FE-43-bucket-sizes.json` has the list `T` for 120 ranges and the list `N` for 120 values of `Y`. It has the list `Z` of the histogram for 120 pairs of `Y` and `H`. It has the rule of whole numbers from 1 to below 10. It has the lowest and the highest number of each list, and the cases of equal values and one value.
  - `FE-44`: the shape that is already bucketed, and the buckets of the histogram (9.19e, section 1, and 9.19f, sections 1 and 2). The file `FE-44-bucketed.json` has the vectors of both types.
    - The heatmap vectors are the order of the names (numbers, `+Inf`, text), the cells, and the step of the first two times. They are null and zero cells (122 of 240), negative counts, one time row (the error) and irregular times. They also have the frames of the three goldens of C15.
    - It has 14 histogram vectors from the measure. They are the sizes for the default and for 10 buckets, and the size 25 with and without the offset 10. The other vectors are negative values, two clusters, equal values, one value and null values.
  - `FE-45`: the colour scale (9.19e, sections 4 and 5). The file `FE-45-heat-colors.json` has these vectors.
    - The 64 colours of each of the 10 schemes in both themes. The 64 colours of `oranges` that the measure shows (`#fff5eb`, `#fff3e7`, `#fff1e4`, `#ffefe0` and `#7f2704`).
    - The levels of the 126 cells of example 1. The range after `hide_le` and `hide_ge` (101 and 263 for the data of the measure). The opacity of the mode `opacity` for both scales.
    - The test lets a difference of 2 for the 9 exact schemes and 8 for `viridis` against the colours that Grafana drew.
  - `FE-46`: the caps, the texts and the time (9.19e, section 3, 9.19f, section 4, T69 and T70). The file `FE-46-bucket-caps.json` has the counts at 249,999, 250,000 and 250,001 cells. It has the counts at 3,999, 4,000 and 4,001 buckets, and at 500,000 and 500,001 values. It has the message texts and 30 hostile names. The test builds the description and the legend with a fake `document`. It checks that each text is a text node and that no element comes from data.
  - The tests are `tests/frontend/bucket_model.test.mjs` (`FE-42` to `FE-45`) and `bucket_dom.test.mjs` (`FE-46`). A test also runs the model for 500,000 values. It checks the time: 50 ms for the heatmap and 150 ms for the histogram on the reference machine.
  - The test `static_rules.test.mjs` lists `bucket_model` in the pure layer. It checks that the constants of `bucket_model.js` equal 9.19e and 9.19f.
  - The tests `editor_forms.test.mjs` and `editor_draft.test.mjs` get the cases of the groups of 10.3 and of the type change.
  - `FE-48`: the selection of a closed and of an open variable, and the notice (9.10). The file `tests/frontend/vectors/FE-48-open-variables.json` has 4 sets.
    - The set `selection` has 60 vectors. They cover the URL, the current selection and the default. Each has a value that is an option or a value that is not. They cover a closed and an open variable, single and `multi`, with and without `include_all`, and an empty list of options.
    - The set `typed` has 20 vectors for `typedValueError`: 0, 1, 1,024 and 1,025 bytes, the multi-byte characters at the limit, U+0000, U+001F, U+007F and `$__all`.
    - The set `notice` has 10 vectors for `replacedNotice`: one value, three values, five values, a value of 41 characters, "All" and "no value".
    - The set `runtime` runs the class `Runtime` with a fake API. It checks one notice at load and one notice after a reload that removes the value. It checks no notice for an open variable, for an options error and for an empty list.
    - A test runs `resolveSelection` for 10,000 values and 10,000 options and checks the time (100 ms on the reference machine).
  - `FE-49`: the form of the editor and the texts (10.3b, T79). The file `FE-49-open-variables-form.json` has 40 hostile texts. The test builds the control and the preview with a fake `document` and checks that each text is a text node. It checks `defaultErrors` for an open and a closed `custom` variable. It checks the three texts of the menu `Examples` with the lexer of the page, and the body of the preview of a variable.
  - `FE-76`: the rules of the forms of group A (10.3h, rules 4 and 8). The file `tests/frontend/vectors/FE-76-options-a.json` has the vectors of `timeseries`, `barchart` and `xychart`. A vector has a type, the options now, a row id and a value. It gives the options after the write (the exact JSON, with the order of 4.38) and the ids of the shown rows. The vectors cover the removal of an optional member, `sort_by` "None", each rule of display and each bound of a range.
  - `FE-77`: the rules of the forms of group B (10.3h, rules 4 and 8). The test `tests/frontend/editor_options_b.test.mjs` reads the file `FE-77-options-b.json` with the vectors of `stat`, `bargauge` and `piechart`. A vector has a type, the options now, a row id and a value. It gives the options after the write (exact JSON, order of 4.38) and the ids of the shown rows, or the error of the form. The vectors cover the reduction and the pattern of "Fields". They cover the limits of the check lists, the optional members and each rule of display.
  - `FE-78`: the rules of the forms of group C (10.3h, rules 4 and 8). The test `tests/frontend/editor_options_c.test.mjs` reads the file `FE-78-options-c.json`. The file has write vectors and parse vectors of `table`, `heatmap` and `histogram`.
    - A write vector has a type, the options now, a row id and a value. It gives the options and the property set after the write (exact JSON, order of 4.38 and 4.18) and the ids of the shown rows.
    - A parse vector has a text. It gives the value or the error of the form.
    - The vectors cover the sort list, the footer, the width and the durations of `x_buckets.value`. They also cover the write of the log scale and the hidden rows of the colour.
    - The test makes 20,000 random changes of the buckets. Each change MUST keep the heatmap valid (4.33c).
  - `FE-79`: the tables and the audit rule (10.3h, rules 1 and 14). The test `tests/frontend/editor_options.test.mjs` reads the tables of 10.3h in `docs/04` and compares them with the code tables.
    - It checks that each member of `newOptions`, and each optional member, has a row or an entry of NO_CONTROL. It checks that each default equals the value of a new panel, and that each range equals the range in 4.27 to 4.33d.
    - It checks that each choice is 280 px wide or less. It checks that each rule of display is true in one state and false in one state.
    - It checks that the new files have 102,400 bytes or less.
    - The test `editor_audit_rule.test.mjs` gives the pure rule of the audit (`tests/browser/lib/audit_rules.mjs`) synthetic results. It has one result for each verdict.
  - The loaded pie chart against the specification (9.19, 9.19a). The file `tests/frontend/piechart_loaded.test.mjs` runs the real module on a fake DOM. The box has a fraction of a pixel. It checks the circle in the hook `data-geometry`, the scale of the bitmap and the donut hole. It checks that a panel with no `legend` gets no default. The browser case B400 checks the same circle in the browser.
  - The previews of a panel that waits (9.11): the file `preview_common.test.mjs` tests the registry and the shared helpers of `web/js/preview/`. The file `preview_measure.test.mjs` tests the comparison of the measurement harness by tolerance class. The browser case B320 runs the harness.
  - `FE-80`: the URL form of the ad hoc filters (9.5, 9.10a). The file `tests/frontend/vectors/FE-80-adhoc-url.json` has 17 read vectors and 7 write vectors from the measure of Grafana 12.4.2 of 2026-11-22, with the two differences on purpose. The test `tests/frontend/adhoc.test.mjs` also checks the selection, the notice and the cap of 16. It checks the rules of a filter, the request member and the member `adhoc` of 6.14. A parse of 16 items of 100,000 characters takes less than 50 ms.
  - `FE-81`: the level of a log line (7.16e, 9.19h). The file `FE-81-logs.json` has 242 rows and 218 bodies. A row has a value of the level field and a body. Each item has the level that the code of Grafana 12.4.2 gives (the functions of the served bundle). It also has the level texts of the new logs panel. The generator compared the code of Hune with the code of Grafana on 20,000 more random bodies, with no difference.
    - The test `tests/frontend/logs_panel.test.mjs` reads the file. It also checks the order, the time, the line ends and the caps of lines and characters. It checks hostile lines in a fake DOM, the details row and the keys. The time of 10,000 lines is 250 ms or less (the minimum of 7 runs after 2 warm-ups).
  - `FE-83`: the gradient modes and the cosmetic series options (4.18, 4.28, 4.30, 9.19). The test `tests/frontend/fe83_series_options.test.mjs` reads the file `FE-83-series-options.json` with the values of the probe of Grafana 12.4.2.
    - The vectors cover the two colours of `hue` in both themes. They cover the place, the offsets and the colours of the stops of each mode, for lines, areas and bars.
    - They also cover 8 log scales of base 2 (the range, the grid lines and the labels). They cover the null threshold, the bar alignment, the centred zero and the dash arrays.
    - The engine runs on a fake canvas. The test checks the paint function of a series (E43) and the offsets of the bar alignment (E44). It checks the dash array in device pixels at dpr 1 and 2 (E45), and the look of an axis (E46).
    - The test also checks the rows of the editor: the write rule of each new row and the parse of a dash array.
    - These verdicts fail: no effect on the preview, a wrong member, a stale entry, a dead entry and an unstable print.
    - These fail too: a control that the table lists and the pane has not, a skipped control, and a list of 31 entries.
  - `FE-82`: the percent change of a stat (4.29, 9.19). The test `tests/frontend/stat_percent.test.mjs` reads `FE-82-stat-percent.json`, which comes from a measure of Grafana 12.4.2 in both themes.
    - It checks the text and the arrow of 20 changes. They have a rise, a fall, 0, a first value of 0, -0, a negative first value, NULL and NaN cells.
    - It checks the colour of the modes `standard`, `inverted` and `same_as_value` in both themes, and the contrast rule of the value text.
    - It checks the sizes of the value, of the name and of the change in 18 tiles. The tiles are stacked and wide, with a graph, in the text modes `name` and `none`, and with fixed sizes.
    - It checks the write rule and the rules of display of the four rows of 10.3h.
  - `FE-84`: the gauge (4.33e, 9.19g). The file `tests/frontend/vectors/FE-84-gauge.json` holds the measure of the classic gauge of Grafana 12.4.2 of 2026-11-22: 58 gauges of 34 panels in the dark theme. The test `tests/frontend/gauge.test.mjs` computes the cells and the box of each canvas. It also computes the centre, the radius, the widths of the arc and of the band, and the value text. These agree with the measure within 0.02 px, and the box of a canvas within 0.05 px.
    - It also compares the angles of the filled arcs and of the parts of the band, with their colours in the dark theme. It compares the place and the size of the threshold labels, and the size and the box of the titles.
    - It checks the pure rules. An empty scale gives no angle. It checks the scale of the percent units, the entries of the band and the colour of a value on a step.
  - `FE-85`: the map (9.19i, 13.13). The file `tests/frontend/vectors/FE-85-geomap.json` holds the lists of the valid and the refused tile URLs, the projection and the tiles of a view. The Rust test of the configuration reads the same lists, so the page and the server have one grammar.
    - The test `tests/frontend/geomap.test.mjs` checks the constants of 4.35, and Web Mercator with its clamp at 85.0511. It checks the tiles of a view with the cap of 256. It checks the URL of a tile with three whole numbers only, the queue of 6 requests and the cache of 256 tiles. It also checks the rows with no valid position, the number grammar of 7.7, `MAX_GEO_POINTS` before any position, and the linear size.
    - A render with a fake DOM checks that hostile names stay text in the card. It checks that a tile request has `crossOrigin` "anonymous", `referrerPolicy` "no-referrer" and a URL with z, x and y only. It checks the delay of 200 ms and no second request in the same view. It checks the drop of a tile of 512 px and the note above 256 tiles.
  - The tests are `tests/frontend/fe48_open_variables.test.mjs` and `fe49_open_variables_form.test.mjs`. The test `static_rules.test.mjs` takes the new limit of 9.30 and checks that `vars.js` and `var_examples.js` have no `innerHTML`, no `outerHTML` and no `insertAdjacentHTML`. The test `fe02_var_options.test.mjs` keeps its vectors.
  - The tests are `tests/frontend/queries.test.mjs` (`FE-38` to `FE-40`) and `runtime_slots.test.mjs` (`FE-41`). The test `static_rules.test.mjs` lists `queries` in the pure layer, takes the new limit of 9.30, and compares `MAX_JOIN_ROWS` with 4.35.
  - The tests `editor_forms.test.mjs` and `editor_draft.test.mjs` get the cases of the chips, of the delete of the first query and of the dialogs (10.3).
  - The tests are `tests/frontend/annotations.test.mjs` (`FE-35`, `FE-36`) and `annotations_dom.test.mjs` (`FE-37`). A test also runs `marks` and `hitTest` for 2,000 and 10,000 marks and checks the time (5 ms and 20 ms on the reference machine).
  - The test `static_rules.test.mjs` lists `annotations` in the pure layer and takes the new limit of 9.30. It checks that `panels/annotate.js` has no `innerHTML`, no `outerHTML`, no `insertAdjacentHTML` and no `document.write`.
  - The tests `editor_forms.test.mjs` and `editor_draft.test.mjs` get the cases of the tab "Annotations" and of the delete of a panel (10.3b).
  - The test `tests/frontend/markdown.test.mjs` runs `FE-28` and the test `markdown_dom.test.mjs` runs `FE-29` and `FE-30`. The test `markdown_property.test.mjs` has the properties below.
    - Seeded random texts, with the seeds 1 to 8 and 5,000 texts for each seed. The alphabet holds the characters of the grammar, the scheme names, the payloads of `FE-29` and non-ASCII characters. The text is built from 1 to 60 pieces.
    - P1: the parser never throws. Its answer is a tree or a limit.
    - P2: each node has a kind of the list, each destination passes the rule of 9.19b, and each text leaf is a string.
    - P3: `stats.steps` is 6 times the number of characters at most, line ends included.
    - P4: the depth and the count of nodes are within the limits.
    - P5: the DOM that the builder makes has only the elements and the attributes of 9.19b.
    - P6: with a hostile replacement, the tree has the same shape as with the identity.
    - P7: two runs on the same text give equal trees, and the input is not changed.
    - 12 families of hostile texts, each at 4,096, 16,384 and 65,536 characters. They are nested quotes, nested lists, open emphasis with `*` and with `_`, mixed marks, brackets, and bracket pairs with `(`.
    - They are also backtick runs of 1, 2, 3 and more characters, backslashes, `http://` runs, wide tables and many short lines. Each one shows the bound P3.
  - The test `static_rules.test.mjs` lists `markdown` in the pure layer. It fails on a token of rule 1 of 9.19b in `markdown.js` and `panels/text.js`. It fails on an element name or an attribute name outside the lists of rules 2 and 3. It checks that the constants of `markdown.js` equal the tables of 4.35 and 9.19b.
  - The tool `tests/frontend/tools/markdown_fuzz.mjs` runs the properties P1 to P7 with a seed and a count from its arguments. It prints the seed and the text of the first failure. A person runs it by hand. The job `nightly` does not run it yet.
  - The tests `editor_forms.test.mjs` and `editor_draft.test.mjs` get the cases of the group "Text" of 10.3 and of "Add text panel" of 10.6.
  - The test `tests/frontend/view_state.test.mjs` runs the vectors of `FE-26`. It checks the round trip `parse(render(state))` and the normal form of the URL.
    It also runs a walk of 2000 events with a model of the history stack, for 40 seeds. The model has `push`, `replace`, `pop`, Back and Forward. It keeps the URL and the state equal.
- The tree of the query text (F42, 10.3c to 10.3g). The vector sets FE-55 to FE-67 are files of `tests/fixtures/ast/`. Each case has an input text, a dialect and an expected outline. The outline has the node types with their offsets, the clause names, the `foldable` marks and the pairs. The outline is the contract. The tests do not compare the whole object.
  - `FE-55`: ClickHouse `WITH` and CTE. One CTE, three CTEs, a scalar `WITH 1 AS x` and a CTE that holds a CTE. A `)` that is not there, and a name that is not there.
  - `FE-56`: ClickHouse subqueries and `JOIN`. A subquery in `FROM`, in `IN` and in `EXISTS`. `INNER`, `LEFT OUTER`, `ANY LEFT`, `GLOBAL`, `ASOF` and `CROSS JOIN` with `ON` and `USING`. A `LEFT(` function that is not a join. A `UNION ALL` of three queries.
  - `FE-57`: ClickHouse `ARRAY JOIN`, lambdas, `FORMAT` and `SETTINGS`. `LEFT ARRAY JOIN`, `x -> x * 2`, `(x, y) -> x + y`, `arrayMap` with a lambda, `WITH FILL`, `WITH TOTALS`, `LIMIT 1 BY x`, `FORMAT JSON`, `SETTINGS a = 1, b = 2`.
  - `FE-58`: ClickHouse names and literals. Backtick names with a doubled backtick and with a bracket inside, and double-quote names. Arrays `[1, [2, 3]]`, tuples `(1, 'a')`, a map `{'a': 1}` and a parameter `{name:String}`. The macros of 7.10 with and without arguments, the variables `${name}`, `${name:csv}` and `$name`, and a macro in a string (which stays text).
  - `FE-59`: comments and strings for both dialects. The comments `--`, `#` and `/* */`, and a comment with no end. A string with `''` and with a backslash. A bracket in each of them. A line comment at the end of the text with no line end. The line ends CR LF and CR.
  - `FE-60`: PostgreSQL. The bodies `$$ ... $$` and `$tag$ ... $tag$` with brackets and line ends inside; the casts `x::Int` and `x::text[]`; `WITH RECURSIVE`; the operators `->`, `->>`, `#>` and `@>`; the strings `E'...'` and `U&"..."`; the parameter `$1`; a `LATERAL JOIN`; `RETURNING`; a `#` that is an operator.
  - `FE-61`: MongoDB. A pipeline with `$match`, `$group`, `$sort` and `$limit`, and a nested pipeline in `$facet`, in `$lookup` and in `$unionWith`, with a depth of 3. Typed objects (`$date`, `$oid`) and the placeholders `"$host"` and `"$__timeFrom"`. A comma at the end of a list, a duplicate member, a `]` that is not there, and a value after the first one. Each gives `error` nodes and no exception.
  - `FE-62`: the caps. The sizes 262,144 and 262,145 bytes, with ASCII and with 2-byte, 3-byte and 4-byte characters. The depths 128 and 129. The node counts 100,000 and 100,001. Each case has the reason and the `at` offset.
  - `FE-63`: the SQL format for ClickHouse. Goldens for rules 1 to 13 of 10.3d, each with input and output. Each output formats to itself.
  - `FE-64`: the SQL format for PostgreSQL. Goldens for `$$`, `::`, JSON operators, `WITH RECURSIVE`, `RETURNING` and a `#` operator.
  - `FE-65`: the JSON format. Goldens for 2 spaces, an object of 79, 80 and 81 columns, an array that fills, nested pipelines, and the refusals.
  - `FE-66`: the structure functions. `folds`, `bracketPartner` and `blockAt` for each caret offset of 30 texts. An offset outside the text (-1 and the length plus 1) gives `null`.
  - `FE-67`: odd texts. An empty text, a text of blanks, a BOM, `U+0000`, a lone surrogate, a pair of surrogates and bidi controls. A TAB, a line of 100,000 characters, and 10,000 open brackets. The result is a tree or a cap, never an exception.
  - The tests are `tests/frontend/ast_sql.test.mjs` (`FE-55` to `FE-59`), `ast_pg.test.mjs` (`FE-60`), `ast_json.test.mjs` (`FE-61`), `ast_format.test.mjs` (`FE-63` to `FE-65`), `ast_structure.test.mjs` (`FE-66`) and `ast_props.test.mjs` (`FE-62`, `FE-67` and the properties below).
  - The test `ast_props.test.mjs` runs seeded random texts, with the seeds 1 to 8 and 2,000 texts for each seed and dialect. The alphabet holds the keywords, the brackets, the quotes, the comment starts, the macros, the variables, the dollar quotes and non-ASCII characters. The text is built from 1 to 80 pieces.
    - A1: no function of the tree throws, and the input is not changed.
    - A2: the tokens cover the text with no gap and no overlap, and each position is inside the text.
    - A3: each child is inside its parent, and the children of a node are in order and do not overlap.
    - A4: each code bracket belongs to exactly one node with `open` or `close`, or to an `error` node. No bracket of a string or a comment does.
    - A5: a macro and a variable are one node or one token, and their bytes equal the text.
    - A6: the time is linear. The parse of a text takes at most 2.5 times as long as the text of half the size. The sizes are 32,768, 65,536, 131,072 and 262,144 bytes. The last text takes 100 ms or less (the median of five runs).
    - A7: a text over a cap gives `capped` with the right reason, and no other result.
    - A8: two runs on the same text give equal trees.
    - A9: the format of a result that is not refused keeps the sequence of the non-blank tokens, with the same kinds and texts.
    - A10: the format is idempotent.
    - A11: a comment, a string, a name, a macro and a variable keep their bytes after a format. A line comment is always followed by a line end or by the end of the text.
    - A12: a refused format returns the input, and gives a reason and a place.
    - A13: the layout does not depend on the number of blanks. A text and the same text with each blank run changed to another blank run give the same result. The two runs have a line end in the same places.
    - A14: a text with random comments and random blanks (3,000 texts) is formatted or refused, never changed in its tokens.
  - The test `static_rules.test.mjs` lists `editor/ast/` in the pure layer. It fails on a DOM name in these modules (`document`, `window`, `Node`, `Range`). It fails on a recursion on the depth of the input. It fails on an import of these modules from a module of the page. It checks that the constants of `nodes.js` equal the table of 10.3c.
  - The test `ast_props.test.mjs` also runs the parse and the format of 262,144 bytes, and checks the times of 9.30 with the limits of rule A6.
  - The tool `tests/frontend/tools/ast_fuzz.mjs` and its copy `tools/ast_fuzz.mjs` run the properties A1 to A14 with a seed and a count from their arguments. They print the seed and the text of the first failure, as `markdown_fuzz.mjs` does. A person runs the tool by hand. The job `nightly` does not run it yet.
- The selection of the new variable kinds (F27, 4.10a, 4.10b, 9.10).
  - `FE-68`: the selection of the new kinds. 40 vectors. They cover the options of an `interval` with and without "Auto", the URL value, the current value and `default`. They cover a value that is not an option, `$__auto` and the request member. They also cover the URL value of a `constant`, its request member and the display text.
  - `FE-69`: hostile texts. 40 texts (markup, `$`, quote marks, very long) as an item, as a value and as a URL value. The test uses a fake `document` and checks that only `textContent` is used.
  - The sets `FE-02` and `FE-30` get the cases of the two kinds. Their other vectors do not change.
- The eleven transforms (F28, 4.25a, 9.21a). The numbers FE-62 to FE-67 of the first plan were in use (F42), so these sets are FE-70 to FE-75.
  - `FE-70`: names. `organize`, `filter_fields_by_name` and `rename_by_regex`. 48 vectors from the measure of Grafana (the ids `o01` to `o25`, `ff01` to `ff18`, `rr01` to `rr16`), with the differences of 19.20j marked. The test also runs 40 hostile names (markup, `$`, very long) and a slow pattern on 1,024 names (50 ms).
  - `FE-71`: `calculate_field`. 45 vectors. They cover the four modes with null cells, NaN, infinity and a `bool`. They also cover the error for a text operand, the alias, and `replace_fields` with and without a time field.
  - `FE-72`: `sort_by`, `limit` and `convert_field_type`. 50 vectors, with the table sort, the stable order, the cut at 0 and at `end` `last`, and the conversion table of 9.21a, rule 12.
  - `FE-73`: `merge` and `join_by_field` on lists of frames. 60 vectors from the measure (`m01` to `m13` and `j01` to `j23`). They cover the keys, the conflicts, the null keys and the order of the keys. They also cover the product of `inner`, the frames with no key, and the refs and prefixes of 9.22a.
  - `FE-74`: `reduce` and `partition_by_values`. 30 vectors (the titles, the kinds of the reducer fields, a text field as a row, the labels, the order of first appearance, `keep_fields`).
  - `FE-75`: the caps and the time. `MAX_TRANSFORM_ROWS`, `MAX_TRANSFORM_FIELDS`, `MAX_TRANSFORM_FRAMES`, `MAX_TRANSFORM_CELLS` and `MAX_MERGE_STEPS` each at the bound and one above. A merge counts one step for each field of a compare. A join that would pass a cap fails before it builds a row.
  - `FE-75` also checks the cap of the cells, which is the member `transform_max_cells` of the session. A configured value applies. An absent or bad value fails closed. No file of `web/js` holds a copy of 10,000,000.
  - The timing test of `FE-75`: `join_by_field` and `merge` for 200,000 rows in all, `sort_by` for 200,000 rows, `convert_field_type` and `calculate_field` for 200,000 rows, each in 300 ms or less.
  - A measure file `tests/frontend/vectors/measure-f28.json` holds 237 cases measured on Grafana (the input spec, the Grafana output). The test runs the prototype on each case and checks that the 60 differences are the ones that 19.20j lists.
- The vectors of drag and resize are in `tests/frontend/editor_drag.test.mjs` and `tests/frontend/editor_drag_pixels.test.mjs`. They hold the layouts of Grafana for a path of cells, the clamp ranges and the pixel rules.
- Three tests of `tests/frontend/grid_frame.test.mjs` check the menu item Inspect: `inspectAllowed` and `inspectModel` of `web/js/panel_frame.js` (9.11).
- The test `tests/frontend/engine_removals.test.mjs` checks the removals E17 to E21 and E35 to E42 of the copied engine (9.24). Its lists name the helpers of `app_util.js` (E35), the bucket readout (E36), the colour kernel (E37) and the members of the state shim (E38). They also name the chart card (E39) and the alignment groups (E40). For E41, they name the legend filter, the totals legend and the mode button, and for E42 ten options that Hune never sets. It fails when a removed function comes back as a call in its engine file, or a removed member as a call in `web/js`. It also fails on a removed engine option in a panel, and on an import of a removed name or of the shim `app_ui_copy.js`.
- The regular expressions of the fixtures run in two engines: the check of `hune-core` (Rust) and the `RegExp` of the
  page (Node). Each pattern gives the same accept or refuse result and the same matches in both (19.30).
- The vectors of a Grafana rule come from Grafana itself, through the capture of 16.13, where Grafana can give them.

### 16.11 PostgreSQL failover test

The test proves that a failover of the primary loses no acknowledged save, makes no duplicate, and stops no read. It runs in
the `failover` job (16.17). The CloudNativePG drill of 16.19 completes it before 1.0.

The compose project `tests/failover/compose.yaml` holds these services:

| Service | Role |
|---|---|
| `etcd` | The configuration store of Patroni, one node. |
| `patroni-1`, `patroni-2`, `patroni-3` | PostgreSQL 18 with Patroni, built from `tests/failover/Dockerfile.patroni`. Settings: `ttl: 10`, `loop_wait: 2`, `retry_timeout: 4`, `synchronous_mode: true`, `use_pg_rewind: true`. |
| `haproxy` | The one address of the primary, as the `-rw` service of CloudNativePG. It checks `GET /primary` on port 8008 of each node every 1 s and closes the sessions of a node that goes down. |
| `hune-a`, `hune-b` | Two instances with `storage.postgres.host = "haproxy"`, `poll_interval = "1s"`, `statement_timeout = "2s"`. |
| `driver` | The load and the journal: the integration test `failover` of the crate `hune`. |
| `checker` | The checks of the end, with `psql`. |

The script `tests/failover/run.sh <mode>` runs the steps. The mode `kill` sends SIGKILL to the primary. The mode `partition`
removes the primary from the network without a TCP reset, as a cut node does. The job runs both modes.

1. Start `etcd`, the three Patroni nodes and `haproxy`. Wait until Patroni reports one leader and two replicas.
2. Through `haproxy`, as the superuser, create the two roles of 5.25. Run `hune migrate-store -c migrate.hcl` once with
   the owner role, then give the grants of 5.25 to the application role. Start `hune-a` and `hune-b` with the
   application role. Wait until `/readyz` gives 200 on both.
3. Create 20 synthetic dashboards through `hune-a`.
4. Start the driver. Every 200 ms it saves a random dashboard, on `hune-a` and `hune-b` in turn, each save with a new
   `save_id`. On an answer 503 it sends the same request again every 1 s, for at most 120 s.
   The request keeps its `save_id` and its `If-Match`. Every 100 ms it reads the list and one dashboard on both instances. It writes each request and answer to its
   journal.
5. After 30 s of load, find the leader with the Patroni REST API and apply the mode to it.
6. Wait until Patroni reports a new leader (at most 60 s). Record the time.
7. Keep the load for 60 s more. Then start the old leader again, or connect it again. Wait until it streams as a replica
   (at most 120 s).
8. Stop the driver. Wait 2 s, then run the checker.

| Pass criterion | Check |
|---|---|
| F1 | No acknowledged save is lost. For each save with answer 200 in the journal, the history of its uid holds exactly one version with its `save_id`. The last etag of each uid equals the etag of its last acknowledged save. |
| F2 | No duplicate: no two versions have the same `save_id`. |
| F3 | Reads never fail. Each read of the journal on both instances has the answer 200. |
| F4 | During the gap, each save fails with 503 and a retryable error within 10 s. No answer is 500, and no request hangs. |
| F5 | The first acknowledged save comes at most 15 s after the new leader, and at most 30 s after step 5. |
| F6 | Within 2 s after the end of the load, both instances list the same etag for each uid. |
| F7 | The old leader, now a replica, holds the same history rows as the new primary. |
| F8 | `/readyz` gives 200 on both instances during the whole test. |
| F9 | No Hune container restarted (restart count 0). |

### 16.12 ProcurAuth series U

The `procurauth` job runs the runner `access-conformance` of the ProcurAuth repository with the role `pep` and the series U.

Section 11.32 owns the run: the fixture, the configuration `tests/conformance/config.hcl`, the profile
`tests/conformance/profile.json` (service id, operations, skip conditions) and the expected counts. This paragraph gives
the CI side only.

- The job fetches the public repository `ProcurAuth/procurauth` at one full commit: the commit of the git dependency
  `procurauth-pep` (11.10, 17.9). `docs/CONFORMANCE.md` of Hune names it.
- The script `tests/conformance/run.sh` builds the runner and the static binary of Hune in Docker.
- It starts Hune in a container with `--network none` and a read-only root. A ClickHouse container joins the network
  namespace of Hune and listens on `127.0.0.1:8123`. At its first start it runs `tests/fixtures/clickhouse/init.sql`
  (16.8): the profile and the user of 8.12. The configuration names that user, with `password_file`.
- The runner runs in the same network namespace. It is the client, the edge and the PDP on `127.0.0.1:8181`. As the
  edge, it signs the identity tokens with the test keys of ProcurAuth. Hune never reaches a real PDP.
- At the commit of 19.28, the run gives the counts of 11.32: 110 cases, 61 passed, 0 failed and 49 skipped.

The file `tests/conformance/expected.txt` lists the known failed cases, one id on each line with its reason. The script exits
with 0 only when the set of failed cases equals this list. At the exit of M6, the list MUST be empty, or each line MUST have
a written reason. `docs/CONFORMANCE.md` records the counts of passed, failed and skipped cases.

### 16.13 Importer tests

| Case | Check |
|---|---|
| I01 | Each row of 15.6, each row of its value tables and each step of its rule of value mappings has a synthetic fixture in each input form that holds the member: v1 raw, v1 wrapped, v2alpha1, v2beta1. The fixtures include a `byRegexp` override and a stat `reduceOptions.fields` with the pattern `/^Total$/` (19.29), each v2 variable sort name, and a v2 transformation item. The expected document and the expected items are golden files. |
| I02 | A member that no row names (the fixture holds `"zzz": 1`) gives `unsupported` with the reason "unknown member". The member `"zzz0": false` gives metadata. |
| I03 | The JSON report validates against `tests/golden/import/report.schema.json`. The Markdown report equals its golden file. |
| I04 | Exit codes: 0; 1 for an item without acceptance; 1 for a stale line; 2 for each error of 15.2 and 15.4; 3 for each case of step 3 of 15.13 and for a held volume lock. |
| I05 | A dry run writes nothing: the hash of the store tree is the same before and after. With `-c`, the results `unchanged`, `would_write`, `would_refuse`, `would_create` and `would_rename` are correct. |
| I06 | Idempotence: the second `--write` run gives `unchanged` for each dashboard, writes no history version and exits 0. The volume tree is byte-equal. |
| I07 | `--replace`: a changed dashboard gets one version with the author `import-grafana`. An equal dashboard gets none. |
| I08 | The uid rule of 15.8, with a collision that gives the `g-` form. |
| I09 | Folders: the root gives no folder; the folder map applies; a folder uid that breaks the rule and has no entry blocks its dashboards; two folders to one id give exit code 2. |
| I10 | Sources: a mapped ClickHouse source; an unmapped one is `unsupported`; a MongoDB query that 7.28 to 7.33 refuse is `unsupported`, the panel is removed, and the report holds its JSON cut to 200 characters. |
| I11 | `--link-prefix` rewrites the links. The nginx map equals its golden file. `$` is encoded as `%24`. Short links get their targets. |
| I12 | Each written document passes the validation of section 4 and the statement check of section 7. |
| I13 | `--report` or `--nginx-map` inside a git checkout gives exit code 2. |
| I14 | The amd64 and arm64 binaries give byte-equal documents and reports (except `created_at`). The `image` job runs this case. |
| I15 | Expansion vectors (below). |
| I16 | A, B, A: import A, then `--replace` with B, then `--replace` with A again. The third run gives `replaced`, a new version of kind `import`, and the bytes of A in the store. Its `save_id` differs from the `save_id` of the first run. |
| I17 | A crash of the importer after the write and before the report (fault injection). PostgreSQL, after the commit: a new run reads the store at step 3 and gives `unchanged`. Volume, after the history files and before the rename: a new run gives `replaced` (or `written` for a new uid) through the roll-forward of 5.14, step 5. In both cases the uid has one version for the write. |
| I18 | `--replace` with a dashboard in another folder gives `moved` (same bytes) or `moved_replaced` (other bytes). A folder with another title gives `renamed`. Without `--replace`, each of these cases gives exit code 3 and writes nothing. |
| I19 | A uid whose current file is not valid (a hook writes bad JSON): `--write --replace` saves over it with the etag of the raw bytes. The history holds a version of kind `external` without bytes (5.14), then the version of kind `import`. |
| I20 | Each row of 15.6 for these members has a fixture in each input form: the matcher `byNames` (`include`, `exclude`, an absent mode, 65 names, `prefix` and `readOnly`), `matcher.scope` (`series` and `nested`), `__systemRef`, `justifyMode`, `thresholdsStyle.mode` (the six values and `series`), the cell types (`gauge` in the three modes, the older `gradient-gauge`, `lcd-gauge` and `json-view`), `custom.footer`, `custom.wrapText`, `custom.hideFrom`, `legend.sortBy`, `legend.sortDesc`, `tooltip.sort`, `tooltip.hideZeros`, the new reducers, the new units, the new macros, and each format of 7.9 in a string and in code. |
| I21 | The sample "Stream": the dry run has no `unsupported` item for `justifyMode`, `matcher.scope` or `hideSeriesFrom`. The 14 items of these three features are `mapped` or metadata. The other items stay as they are. |
| I22 | The milestone PG. Each PostgreSQL row of 15.6 has a fixture in each input form that holds the member: both `format` texts, the builder, the macros, a group macro with a third argument, `$__interval` as an interval, a macro of the other dialect. A source map to a block of the wrong kind gives exit code 2. A map without `-c` takes the dialect from the type. |
| I23 | The milestone MMG. Each MongoDB row of 15.6 has a fixture in each input form that holds the member: both `queryType` values, a pipeline with each macro of 7.34, the wrapper of `$__from`, the bare token, a refused stage, a `$lookup` outside the list, the shell syntax, an Extended JSON form that 7.28 refuses, a placeholder in a key. A source map to a block of the wrong kind gives exit code 2. A map without `-c` takes the kind from the type. |
| I30 | The dashboard and panel members. Each row of 15.6 for these members has a fixture in each input form. The fixtures cover `timezone` (a name, `UTC`, `default`, `GMT`, a name with a blank), the dashboard links (both types, the flags, an empty title, 9 tags, 17 items, `tooltip` and `icon`), and `timeFrom`, `timeShift` and `hideTimeOverride` (each text form, a refused form, a panel with `-- Dashboard --`). |
| I31 | The properties. The rows of `color` (the 10 continuous ids, `palette-classic-by-name`, `palette-saturated` with one `changed` item each, and `shades`), `axisPlacement`, `axisLabel`, `axisSoftMin`, `axisSoftMax`, the five style members in an override and in `defaults`, `lineStyle` (the three fills and an array that is not the default) and `transform`. Each row runs in a `timeseries`, a `barchart` and a `table`. |
| I32 | Each written document has `format_version` 3 and the member `links`. The dry run of the sample "Stream" gives the same counts as before for the other members, and the member `links` is `[]`. |
| I33 | The golden documents of `tests/golden/import/` equal the files that the importer writes in format 3. The golden report equals the new report. |
| I34 | Idempotence in format 3: the second `--write` run gives `unchanged` for each dashboard. A volume of format 2 and a run of `--write --replace` give the documents of format 3 (`replaced`). |
| I35 | Bounded work (findings D6 and D7 of the security review of 2026-11-16). A list of 60,001 panels with reuse panels and repeats gives one item "the dashboard has N panels" and no check across panels. 201 panels give the same item, and 200 panels give the items of the checks. A PostgreSQL text of 262,144 bytes of `$__timeGroup(` with no `)` takes a linear time. A timing test takes the minimum of 7 runs after 2 warm-ups. Each placeholder form of 7.6 of an open `custom` variable in a MongoDB query closes the variable, and `$names` does not. |
| I36 | The fixture `tests/fixtures/grafana/datasource_variable.json` has the shape of the plugin examples: `${datasource}` in a panel, in a target and in a query variable, and an item of `__inputs`. Each row of 15.6 for the data source variable runs in the forms v1, v2alpha1 and v2beta1. Separate tests cover a variable that the map does not name, `$datasource` in a SQL text, the format 2, and a row with panels at its own `y` (expanded, collapsed, two rows, a `y` below the row). A timing test checks that the scan of a SQL text for these names is linear: 4 times the names and the placeholders give less than 9 times the time. The dry run of the 5 example dashboards of the ClickHouse plugin 4.5.1 gives a document for each dashboard without an ad hoc variable, once each item is accepted. |
| I38 | The data link tokens of Grafana 12. `crates/hune-grafana/tests/link_tokens.rs` imports the two links of the table "Traces table" of the OTel dashboard of the ClickHouse plugin 4.5.1, with their U+FEFF. Both links stay, with no item and with no U+FEFF. With `--link-prefix` and the uid of the run, both links get the Hune uid. A panel link and a data link with each of the six tokens of 9.18 give no item. `${<name>:raw}`, `${__value.calc}`, `${__field.labels.<name>}` and `${__from:queryparam}` stay `unsupported`. |
| I39 | The ad hoc variable. `crates/hune-grafana/tests/adhoc_import.rs` has 5 tests. The variable keeps the kind `adhoc` with the keys of `defaultKeys` and `filters`, and each query on its source gets a `changed` row that asks for `AND $__adhocFilters`. The SQL does not change. With no key, the keys come from the configured list; with no list, the variable is removed. A key outside the list or the grammar is removed. An unmapped source removes the variable. The v2beta1 form maps the same way. The CLI gives the lists of the configuration to the importer. |
| I40 | The logs panel. `crates/hune-grafana/tests/logs.rs` has 7 tests in the forms v1, v2alpha1 and v2beta1. The logs panel of the plugin example "Simple ClickHouse OTel Dashboard" imports with one `changed` item (its builder form). The four options map, a default leaves the canonical form, and a bad value is `unsupported` with the default. Each option that Hune does not have is metadata at the value of a new Grafana panel and `unsupported` else, with its fallback. The format 2 gives `logs` on a logs panel and `table` on another panel. A logs panel keeps its first query. |
| I41 | The gauge. `crates/hune-grafana/tests/gauge.rs` has 7 tests. A new classic gauge of Grafana 12.4.2 imports with no item, and its canonical options hold `reduce` only. Each option maps to its member of 4.33e, with `custom.neutral` of the defaults as `neutral`. The sizes are metadata with `sizing` = `auto`. A value out of range, a `neutral` that is not a number and a `neutral` in an override are `unsupported`. A member of the new gauge and an unknown member of `text` are listed. |
| I42 | The map. `crates/hune-grafana/tests/geomap.rs` imports a `geomap` with a base layer, a tile layer `xyz`, a marker layer and each view. The written document holds no host of a tile source and no member `basemap`. The base layer and the tile layer give a `changed` item each. Each preset view gives its centre, `fit` gives its highest zoom, and a zoom with a fraction is rounded with a `changed` item. The location modes `geohash` and `lookup`, a star, a text, a scale and the measure tools are `unsupported`. A panel with two queries keeps the first query. |
| I45 | The bar chart members. `crates/hune-grafana/tests/barchart_p03.rs` has 7 tests. `custom.fillOpacity` and `custom.lineWidth` map to `fill_opacity` and `line_width` with no item, and the canonical form leaves out 80 and 1. `null` and an absent member give the defaults. A value out of range or of another type is `unsupported` with the default, and the member in an override is `unsupported` with "property removed". `orientation` `auto` maps with no item, and an absent orientation gives `auto`. `custom.axisGridShow` `true` and `false` map to `axis_grid` in `defaults` and in an override, and `null` is metadata. The value `false` stays `unsupported` in a `timeseries`, a `histogram` and an `xychart`. The two bar charts of the dashboard "Data Analysis" of the plugin examples give one item only, the gradient mode. |
| I43 | The percent change and the text sizes of a `stat`. `crates/hune-grafana/tests/stat_p11.rs` maps `showPercentChange` = `true`, `percentChangeColorMode` `inverted` and `same_as_value`, and `text.titleSize` and `text.valueSize` from 1 to 200. `showPercentChange` = `false`, `standard`, `text: {}` and a size of 0 stay metadata and give no member, with the counts of before. A value of another type, a size out of range or with a fraction, an unknown mode and `text.percentSize` are `unsupported` with no member. A valid size stays when the other one is bad. |
| I47 | The defaults. `crates/hune-grafana/tests/wave_h_defaults.rs` has one test for each case of 15.5 that a `false` or a `0` turns off: the grid lines in three types with an axis and in an override, and `wideLayout` of a `stat`. A test checks that a `barchart` keeps 0 and `false` of its fill, its line and its grid lines in the document (4.18, 4.30). The old footer of a table has a test for `fields` empty, one name and two names, for `countRows`, for a footer that is not shown and for `enablePagination`. It also has tests for a reducer with no equal and for a bad `fields`. A test counts each member of the footer one time. A list of 65 names in `fields` gives an item, and 200,000 names stop at the 65th. `prepareTimeSeries` has a test for each of the four formats. The bar gauge has a test for `sizing` absent, `auto`, `manual` and another value. One golden case, `tests/golden/import/h0_defaults.txt`, holds the four items in one dashboard. |
| I44 | The members of the series panels. `crates/hune-grafana/tests/series_p10_p14.rs` imports the members of four panels of the examples of the ClickHouse plugin 4.5.1: the two gradient time series of "Query Analysis", the bar chart `hue` and the time series with `axisWidth` 3 of "Data Analysis". These members give no item. Each value of `gradientMode`, `barAlignment`, `spanNulls` as a number, `scaleDistribution.log`, `lineStyle.dash`, `axisCenteredZero`, `axisWidth`, `axisBorderShow` and `axisColorMode` has a test, also out of range. `symlog` stays `unsupported`. |

The expansion vectors include:

- the built-in variables `$__interval_ms`, `$__from` and `$__to`, in code and in a string, and the names that stay text (`$__range`, `$__range_ms`);
- the macros with the suffix `_ms`, `$__dateFilter` (also for a range across UTC midnight) and `$__dateTimeFilter`;
- `$__dt`, the second name of `$__dateTimeFilter`;
- `$__conditionalAll` for "All", "All" with an `all_value`, one value and two values;
- `$__conditionalAll` for a first argument with a value that the rules of code refuse, while the macro gives `1=1`;
- each format of 7.9 for one value, for a list and for "All", in a string, and the refusal of each one in code;
- the injection vectors. Each format of the last group of 7.9 has a value that holds `'`, `\`, `"`, `--`, `/*`, `*/`, `;`, `$__timeFilter` and U+2028;
- in these vectors, a string keeps the output in its literal, and the decoded literal is the text of the format;
- in these vectors, code and a quoted name give the error of 7.18 at save and `422 sql_invalid` at run.

The vectors of the kind `mongodb` are in `tests/golden/expansion-mongo/` and `tests/golden/pipeline-mongo/`. The first holds each macro and each format of 7.34, with the injection vectors of 7.35. The second holds one accepted text and one refused text for each name of 7.31 and 7.32. It also holds the expected frame of each accepted text, in Extended JSON. The capture tool makes the macro vectors from the pinned plugin, as the next paragraph says for PostgreSQL. The recorded differences have the same file.

The vectors of the dialect `postgres` are in `tests/golden/expansion-pg/`. They hold each macro of 7.22, each unit of 7.23, the escapes of 7.21 and the injection vectors of 7.24. The capture tool makes them from the pinned Grafana with a PostgreSQL data source. The recorded differences have the same file, `tools/parity/vector-differences.json`.

The expansion vectors are the reference for the macros and the variables. Each file of `tests/golden/expansion/` holds a
template, the variable values, the range, `max_data_points`, `min_interval` and the expected SQL. The unit tests of
`hune-core` compare the expander with the committed vectors, with no Grafana.

The `import` job makes the vectors again from Grafana. The compose project `tests/compose/grafana.yaml` runs a pinned
Grafana image with the ClickHouse plugin 4.5.1, and a ClickHouse with synthetic data. The plugin archive has a SHA-256 in
`tests/images.lock`. The line holds the download URL of the `linux/amd64` archive, because grafana.com refuses a download
with no `os` and `arch`. The capture tool of 15.14a reads `executedQueryString` for each synthetic dashboard. The job fails when
a vector differs from the committed file.

### 16.14 Restore drill

The `store` job runs the drill on each change. It proves that `hune export` and `hune import` (5.38, 5.39) lose nothing.

1. Start from the synthetic seed volume: 20 dashboards in 3 folders, with saves, one move, one delete and one restore in
   their history. One dashboard also has a version of kind `external` without bytes: a hook wrote an invalid file, and a
   save replaced it (5.14). The drill checks that export, import and `copy-store` keep this version, without bytes.
2. `hune export -c volume.hcl --to /out/a`.
3. `hune migrate-store -c migrate.hcl` (owner role) on an empty database, then the grants of 5.25. Then
   `hune import -c postgres.hcl --from /out/a` (application role).
4. `hune export -c postgres.hcl --to /out/b`.
5. `diff -r /out/a /out/b` finds no difference.
6. Do the same in the other direction with `hune copy-store --from postgres.hcl --to volume2.hcl` (5.37), into a new
   volume.
7. `hune restore -c <file> --uid <uid>` (5.40) of the deleted dashboard on each store gives the document again.

Pass criteria: no difference in step 5 and step 6; each etag is equal on the two stores; `serve` on each restored store gives
the same answer to `GET /api/dashboards`.

### 16.15 Image and deployment tests

| Case | Check |
|---|---|
| G01 | `readelf -l` of each binary shows no program interpreter. |
| G02 | `hune version` runs on an amd64 runner and on an arm64 runner, each with its own binary. |
| G03 | The image of 17.5 builds locally with no push. |
| G04 | `docker run --read-only --user 65532:65532 --network none -v "$PWD/deploy/compose/volume/config.hcl:/etc/hune/config.hcl:ro" -v "$PWD/tests/fixtures/secrets:/run/secrets:ro" <image> check-config -c /etc/hune/config.hcl` exits 0. The directory `tests/fixtures/secrets/` holds test secret files with the names of the example. |
| G05 | The image runs `serve` with the volume example. The health command exits 0 within 10 s, and Docker reports `healthy`. |
| G06 | SIGTERM to the container: exit 0 within `server.shutdown_grace`. |
| G07 | Each compose example of 17.6 and 17.7: `docker compose up --wait` gives healthy services. `GET /api/dashboards` through the published port gives 200. `docker compose down -v` removes everything. |
| G08 | `kubeconform -strict` in Docker accepts each manifest of 17.8, with the pinned schemas of Kubernetes and of the CloudNativePG resources. |
| G09 | `check-config` exits 0 on each configuration file of `deploy/` (the files of 17.6, 17.7 and 17.8, `migrate.hcl` included). The job mounts each file at the path of its example, with the test secret files at the paths that the file names. A file with an unknown key, such as a key that 13.8 to 13.12 do not hold, fails the case. |

### 16.16 Fuzz and load tests

Fuzz tests run from M7. `tests/images.lock` has no image with a nightly toolchain, because the tag
`rust:nightly-trixie` does not exist (2026-11-22). Thus the `nightly` job prints a note and runs no fuzz target. A person
runs the targets with `tests/ci/fuzz.sh` or `tests/ci/nightly.sh --local`, on a host with the nightly toolchain.

The targets of `fuzz/` are `lexer`, `lexer_pg`, `expand`, `document_parse`, `hcl_config`, `grafana_import` and `frame_decode`. Each target
runs 10 minutes, with a corpus from the fixtures. A crash fails the job, and its input becomes a regression test.

The target `lexer_pg` reads the lexer and the statement check of the dialect `postgres` (7.20, 7.25). Bit 3 of the flags
byte of `expand` selects the dialect `postgres`. With bit 3 clear, the dialect is `clickhouse`.

The front-end tool `ast_fuzz.mjs` (16.10a) tests the tree of the query editor. It runs in Node. A person runs it by hand: the job `nightly` does not run it yet. It is not a target of `fuzz/`. A crash or a property that fails fails the job, and its text becomes a vector of `tests/fixtures/ast/`.

The load test runs before 1.0.0 and before each minor release, in the manual job `load`. The tool is k6 in a pinned image,
with the script `tests/load/dashboard.js`.

- 52 viewers, a number of browsers that was measured on a Grafana instance that queries ClickHouse, each on the synthetic 45-panel dashboard.
- A refresh every 30 s for 30 minutes. Each viewer has at most 6 open queries, as the browser.
- Each panel query reads `SELECT number FROM numbers(100)`, so the time of ClickHouse is small and stable.

Pass criteria:

- No answer is an error.
- The p95 of the panel query time is at most 50 ms above the p95 of the same query sent straight to ClickHouse.
- The resident memory of the process grows by less than 5 percent in the last 20 minutes.

### 16.17 CI jobs

The workflows are `.github/workflows/ci.yaml` (each push and pull request), `nightly.yaml`, `conformance.yml` and
`release.yaml` (17.3). Each job calls one script of `tests/ci/`. Each script uses Docker only.

The scripts `tests/ci/all.sh` and `tests/ci/release.sh` are the release path of the project. They run every job and the release
with no GitHub Actions and no GitHub billing. The workflows in `.github/workflows` are an option. They run the same jobs on GitHub.

The script `tests/ci/all.sh` runs the jobs of the merge gate at the same time: `policy`, `rust`, `store`, `pgsource`,
`mongosource`, `api`, `source`, `frontend`, `browser`, `import`, `image`, `procurauth` and `failover`. A full run takes about as
long as its longest chain of jobs, and not the sum of the jobs. 16.17a gives the order, the options and the isolation of the jobs.

The script runs `procurauth` and `failover` on each full run, with no path filter. The job `failover` is a stub until M5.
The jobs `nightly` and `load` stay by hand.

A full run of `all.sh` on a clean work tree writes the commit in `target/ci-all.ok`, after all jobs passed. The script
`tests/ci/release.sh` (17.3) refuses a commit that this file does not name.

| Job | When | Command | Main steps |
|---|---|---|---|
| `policy` | push | `tests/ci/policy.sh` | The checks of 16.18. |
| `rust` | push | `tests/ci/rust.sh` | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --locked` (units, seeds 1 to 64, configuration). |
| `store` | push | `tests/ci/store.sh` | `docker compose -f tests/compose/store.yaml up --abort-on-container-exit --exit-code-from store-tests`: `cargo test -p hune-store --features conformance --locked` on a tmpfs volume, a tmpfs of 1 MiB, PostgreSQL 15 and 18; then the restore drill. |
| `api` | push | `tests/ci/api.sh` | The compose project of 16.8 and its runs: `cargo test -p hune --test api --locked -- --test-threads=1`. Then the environment test K14 (13.24) in the same project. |
| `source` | push | `tests/ci/source.sh` | The cases of 16.9 on the two ClickHouse versions. |
| `pgsource` | push | `tests/ci/pgsource.sh` | The cases of 16.9a on PostgreSQL 15 and 18, with the compose project `tests/compose/pgsource.yaml`. The job also runs the ignored tests `pg_real` of the binary `hune`, which hold the cases A47, A50 and A51. |
| `mongosource` | push | `tests/ci/mongosource.sh` | The cases of 16.9b on MongoDB 7.0 and 8.x, with the compose project `tests/compose/mongosource.yaml`. A build with no `mongo-driver` feature runs only the cases of the first part. |
| `frontend` | push | `tests/ci/frontend.sh` | Build the browser test image; `node --test tests/frontend/` in it (16.10a). |
| `browser` | push | `tests/ci/browser.sh` | Build the test image; `npx playwright test` in it. |
| `import` | push | `tests/ci/import.sh` | `cargo test -p hune-grafana --locked`; the CLI cases of 16.13 on the release binary; the expansion vectors from `tests/compose/grafana.yaml`. The job runs the service `import-tests` with `docker compose run` (`ci_compose_exec`), because the service `plugin-fetch` ends early with the code 0. A time limit of 25 minutes ends the run. |
| `image` | push | `tests/ci/image.sh` | The musl builds of 17.2 on an amd64 and an arm64 runner; the cases of 16.15; the case I14. |
| `procurauth` | pull request that changes `crates/hune/src/access/`, the configuration code (`crates/hune/src/config/`, `crates/hclconf/`), `tests/conformance/` or the ProcurAuth commit (11.32); nightly; release | `tests/conformance/run.sh` | 16.12. |
| `failover` | pull request that changes `crates/hune-store/`; nightly; release | `tests/failover/run.sh kill` then `tests/failover/run.sh partition` | 16.11. |
| `nightly` | each night | `tests/ci/nightly.sh` | 1,000 random seeds; the fuzz targets only with `--local` (16.16); the `source`, `pgsource` and `mongosource` jobs on both versions again. The job `pgsource` runs after the job `source`. |
| `load` | manual | `tests/load/run.sh` | 16.16. |

A pull request merges only when the jobs of the merge gate pass. The gate grows with the milestones of section 18, because
a job can only pass when its code exists:

| From the exit of | The merge gate runs | Gate cases of `store` and `api` |
|---|---|---|
| M1 | `policy`, `rust`, `image`; `api` on the volume in mode `open` | `api`: A01, A02, A33 |
| M2 | also `source` | `api`: also A18 to A24 and K14a |
| M3a | also `import` | no change |
| M3b | also `frontend` and `browser` | no change |
| M4 | also `store` on the volume | `store`: the common tests of 5.34 except SC-44 and SC-47, the volume tests of 5.35, SX-01; `api`: A01 to A38 and A46 on the volume, with and without a primary store |
| M5 | `store` and `api` also on PostgreSQL 15 and 18; `failover` when its paths changed | `store`: every case, the restore drill included; `api`: A01 to A38 and A46 on both backends |
| M6 | `api` also in mode `authzen`; `procurauth` when its paths changed | `api`: also A39 to A45 and K14b |
| M7 | no new job | `api`: also K14c, with the OTLP exporter |
| MPG | also `pgsource` | `api`: also A47 to A51 |
| MMG | also `mongosource` | `api`: also A52 to A56 |

The file `tests/ci/gate-cases.txt` holds this table as data: one line `<milestone> <job> <case ids>` for each cell. The
file `tests/ci/milestone` holds the last milestone that passed, for example `M2`. The jobs `store` and `api` run every
case on each push. They fail only when a case of the gate list of that milestone, or of an earlier one, fails. A job or a
case that the gate does not run yet gives a result for information only.
A person can run the whole gate with `tests/ci/all.sh`.

### 16.17a The parallel run of the gate

The script `tests/ci/all.sh` starts the jobs as background processes. It does not stop at the first failure. Each job runs to its end, so one run shows each failure. The exit code is 1 when a job fails.

| Option | Effect |
|---|---|
| `--list` | Print the jobs and exit. |
| `--only JOB[,JOB]` | Run only these jobs, and the build tasks that they need. A run with `--only` does not record the gate. |
| `--from JOB` | Run `JOB` and the jobs after it in the order of `--list`. A run with `--from` does not record the gate, except `--from policy`. |
| `--max-jobs N` | Start at most `N` tasks at the same time. The default has no limit. Use it on a host that is smaller than the host of this section. |

Some jobs wait for a build task. Each task runs one time, and its result serves each job that needs it. A job that needs a failed task does not start. The other jobs still run.

| Task | What it does | Jobs that wait for it |
|---|---|---|
| `setup` | Build the builder image of 17.2 when it does not exist. | `build-debug`, `build-binary`, `import` |
| `build-debug` | `cargo test --workspace --locked --no-run` in the builder: the debug artifacts of the test profile. | `policy`, `rust`, `frontend` |
| `build-binary` | Fetch the sources of `Cargo.lock` one time, then build the release musl binary one time (17.2). | `store`, `image`, `procurauth`, `frontend`; `api` and `browser` when their compose files exist |

The job `frontend` waits for the two long builds and for the other jobs of the run, except `image`. Its timing tests need a quiet host. The jobs `pgsource`, `mongosource` and `source` run `cargo` on the host and wait for no task. 
The jobs `pgsource` and `mongosource` build their tests in the background while their containers start. They also run their two versions at the same time. Each version has its own compose project, ports and output.

Each job and each task writes its output to `target/ci-logs/<name>.log`. The script prints one line when a task starts and one line when it ends. It prints one line each minute that names the tasks that run. At the end it prints a table with the duration of each task and the total. It also shows the failed lines and the last 40 lines of the log of each failed task.

**Isolation of the jobs.** Each job names its containers, networks, volumes, images and folders with its own process id. No job uses a fixed port of the host. A job that publishes a port takes a free port of `127.0.0.1` at its start, with `ci_free_port` of `tests/ci/ci-lib.sh`. The function takes a port from 20000 to 29999, which is below the ephemeral ports of Linux. It reserves the port in `target/ci-ports/`, so two jobs never take the same port. The default mode of `procurauth` and the restore drill publish no port of the host.

**Cargo folders.** Each group of jobs has its own target folder, so the jobs do not wait for the build lock of one another. The folder `target` serves `build-debug` and `rust`. The folders `target/policy`, `target/import` and `target/host` serve `policy`, `import`, and the three jobs that run `cargo` on the host. The host builds cannot share artifacts with the builder builds, because the paths of the registry differ. The release binary is in `target/<triple>/release/`.

The vendor step runs one time. It does nothing when `Cargo.lock` has not changed since its last run. A second vendor run writes each source file again, and cargo then builds the git dependencies and `hune` again, which takes more than 3 minutes. A new commit also builds the release binary again, because the binary holds the commit and its date (17.2).

**Resources.** The full gate needs Docker. On a host with 22 cores and 93 GB of memory, a full run with a cold cache takes about 11 minutes. A host with fewer cores takes longer, and `--max-jobs` limits the load. The `target/` folder of a run takes about 25 GB of disk: `target/debug` takes 13 GB and `target/host` takes 6 GB. Make sure that 40 GB of disk are free.

The jobs that are off the longest chain run with a low CPU weight (`nice` and `--cpu-shares`). The longest chain is the release build and the jobs that use its binary.

The seeded random tests of `hune-core` (16.4) run their seeds on all cores of the host, with one thread for each seed at a time. Each seed has its own generator, so a seed gives the same cases as in a run on one thread. The seeds are still 1 to 64, with the same count of cases. The first failure stops the other threads after their current seed. Its report names the seed.

### 16.18 Policy checks

The `policy` job runs these checks. Each check fails the job when its command finds a match or a difference. The patterns
are split with `''` where they would match the policy script itself.

| Check | Command |
|---|---|
| No image push | `grep -rnE --exclude=policy.sh 'ghcr''\.io\|docker/login''-action\|packages: *wr''ite\|push: *tr''ue' .github/workflows tests/ci tests/compose tests/conformance build tools deploy` prints nothing. A second check does the same for `docker push`, `docker login`, `buildx --push` and `imagetools create`. |
| No environment read outside the engine (3.4, R2) | `grep -rnE --include='*.rs' '(^\|[^[:alnum:]_])env::(var\|var_os\|vars\|vars_os\|set_var\|remove_var)\b' crates \| grep -vE '^crates/hclconf/src/env\.rs:\|/build\.rs:\|/tests/'` prints nothing. Only `crates/hclconf/src/env.rs` is exempt; tests and `build.rs` are exempt too. |
| No environment read in the MongoDB driver copy (13.2 rule 5, 8b.15) | The check searches the Rust files of `vendor/mongodb` (the test files excepted) for `env::var`, `var_os`, `vars`, `option_env!`, `getenv`, `/.dockerenv` and `resolv.conf`, and `crates/` for the names of the platform variables. It skips comment lines. A finding is accepted only when the list `tests/ci/env-read-allow.txt` holds it. The list is empty. |
| Local copy of the MongoDB driver (8b.2) | `tools/check-vendor-lock.sh --mongodb`: each file of `vendor/mongodb` is in `vendor/mongodb.lock.json` with its SHA-256, each changed Rust file holds `// hune:`, the reverse of `vendor/mongodb.patch` gives the upstream hashes of the lock, `Cargo.toml` and `fuzz/Cargo.toml` hold the `[patch.crates-io]` line, and `Cargo.lock` has the crate with no source |
| No read of the configuration file outside the module `config` (13.2, rule 4) | `grep -rnE --include='*.rs' 'hclconf::' crates \| grep -vE '^crates/hclconf/\|^crates/hune/src/config/'` prints nothing |
| No process spawn | `grep -rnE --include='*.rs' 'process::Command' crates \| grep -v '/tests/'` |
| No file write outside the volume backend and the CLI output module | `grep -rnE --include='*.rs' 'fs::(write\|create_dir\|create_dir_all\|remove_file\|remove_dir\|remove_dir_all\|rename\|hard_link\|set_permissions\|copy)\|File::create\|OpenOptions\|openat\|renameat\|unlinkat\|mkdirat' crates \| grep -vE '^crates/hune-store/src/volume/\|^crates/hune/src/cli/output\.rs:\|/build\.rs:\|/tests/'` |
| No command document from a caller (8b.5) | `grep -rnE --include='*.rs' '\.run_command\(\|run_cursor_command\|run_raw_command' crates \| grep -vE '^crates/hune-source/src/mongo/driver\.rs:\|/tests/\|/tests\.rs:'` prints nothing |
| No MongoDB crate outside the driver module (8b.2) | `grep -rnE --include='*.rs' '(^\|[^[:alnum:]_:])(mongodb\|bson)::' crates \| grep -vE '^crates/hune-source/src/mongo/driver\.rs:\|/tests/\|/tests\.rs:'` prints nothing, and `grep -lE '^(mongodb\|bson) *=' crates/*/Cargo.toml` prints only `crates/hune-source/Cargo.toml` |
| No write call of the driver (8b.5) | `grep -rnE --include='*.rs' '\.(insert_one\|insert_many\|update_one\|update_many\|replace_one\|delete_one\|delete_many\|drop\|create_collection\|create_index\|create_indexes\|drop_index\|find_one_and_[a-z_]+)\(' crates/hune-source/src/mongo \| grep -vE '/tests/\|/tests\.rs:'` prints nothing |
| No SRV lookup (8b.4) | `grep -rnE 'mongodb\+srv\|dns-resolver\|srv_service_name\|hickory' Cargo.toml crates/*/Cargo.toml deps.lock.txt` prints nothing |
| No simple protocol for the SQL of a caller (8a.5) | `grep -rnE --include='*.rs' 'simple_query\|batch_execute\|copy_in\|copy_out' crates \| grep -vE '^crates/hune-store/\|^crates/hune-source/src/postgres/fixed\.rs:\|/tests/'` prints nothing |
| No unsafe code | each crate root holds `#![forbid(unsafe_code)]`: `grep -L 'forbid(unsafe_code)' crates/*/src/lib.rs crates/hune/src/main.rs` prints nothing |
| No remote load in the front end | `grep -rnE "(src\|href)=[\"']?https?:\|import[^;]*[\"']https?:\|url\([\"']?https?:" web` |
| No inline script | `grep -rnP '<script(?![^>]*\bsrc=)(?![^>]*type="application/json")' web` |
| npm only in test images | `find . -name package.json -not -path './tests/browser/*' -not -path './tools/parity/*' -not -path './target/*' -not -path './vendor/*'` prints nothing |
| Copied chart engine | `tools/check-vendor-lock.sh --chdash`: each file of `web/js/vendor/chdash/` and `web/css/vendor/chdash/` is in `web/js/vendor/chdash/chdash.lock.json` with its correct SHA-256, each changed copy holds `// hune:`, and the reverse of `vendor/chdash.patch` gives the upstream hashes of the lock (9.24) |
| Lexer vectors | the SHA-256 of `tests/fixtures/sql-lex-vectors.json` equals its line in `tests/images.lock` (`tools/check-vendor-lock.sh`) |
| Engine `hclconf` | `crates/hclconf/PROVENANCE` exists, says "not a copy" and names the edition 2021; `crates/hclconf/Cargo.toml` holds `edition = "2021"` (17.1) |
| Configuration reference | `hune print-reference --format markdown \| diff -u docs/configuration-reference.md -` (K13, 13.21) |
| Licences, advisories, bans, sources | `cargo deny --locked check advisories bans licenses sources` with `deny.toml` (17.9) |
| Dependency list | `deps.lock.txt` equals the list that 17.9 generates. The local copy of `mongodb` is a line with the tag `(vendor)`. |
| Synthetic fixtures | No file under `tests/` or `deploy/` holds a word of the forbidden word list as a whole word. An extra check searches each file outside `target/`, `.git/` and `private/`. The list is outside the repository: the CI secret `HUNE_POLICY_WORDS`, or the untracked file `private/policy-words.txt`. With no list, both checks fail. |
| Addresses in tests | each IPv4 literal under `tests/` and `deploy/` is in `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` or the documentation ranges `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` (`tools/check-addresses.sh`) |
| Documentation language | `python3 -I tools/ste_lint.py docs/*.md README.md CHANGELOG.md` reports no error |

The exemption `/tests/` also covers the files named `tests.rs`. This applies to the three checks for the MongoDB driver:
the command document, the MongoDB crate and the write call. The first pattern is a method call, because `run_command` is also the name
of a function of the CLI. The crate name `mongodb::` or `bson::` counts only at the start of a path: `config::mongodb::block`
is a module of Hune.

### 16.19 Manual checks before 1.0.0

- The CloudNativePG drill. Run two Hune instances of the example of 17.8 under load on a K3s cluster with CloudNativePG.
  Run a failover drill. Pass: the criteria F1 to F3, F8 and F9 of 16.11. Saves fail with 503
  during the gap of 1 to 2 minutes.
- The load test of 16.16.
- The verification of 15.14, when a dashboard changed after the cutover verification.
- The cases B170 and B171 in Firefox. WebDriver BiDi cannot run them (16.10). Run them by hand. Pass: the same results as in Chromium.
- The whole browser suite in WebKit. WebKit has no driver on the test host. Run the cases by hand in `MiniBrowser` or in Safari.
  Pass: the same results as in Chromium.
- The input checks of B170 and B171 in Firefox and WebKit. Make a real IME composition and a real dead key (for example `´`, then `e`).
  Each of them must end the folded view before the composition starts. Use a layout where the key `BracketLeft` gives another character.
  Ctrl+Shift+Bracket must still fold.

## 17. Delivery

### 17.1 Repository layout

The repository is `ProcurAuth/hune` (https://github.com/ProcurAuth/hune). The project licence is `MIT OR Apache-2.0`. The files
`LICENSE-MIT` and `LICENSE-APACHE` hold the texts. The project depends on the repository https://github.com/ProcurAuth/procurauth.

```text
hune/
  Cargo.toml                 workspace; [workspace.dependencies] with exact pins (17.9)
  Cargo.lock                 committed; every build uses --locked
  vendor/mongodb/            the local copy of the crate mongodb 3.10.0 with no environment read (8b.2); vendor/mongodb.patch,
                             vendor/mongodb.lock.json
  rust-toolchain.toml        one exact stable version, the two musl targets, rustfmt, clippy
  deny.toml                  cargo deny policy (17.9)
  deps.lock.txt              the resolved crate list (17.9)
  CHANGELOG.md  README.md  LICENSE-MIT  LICENSE-APACHE  LICENSE-THIRD-PARTY
  crates/
    hune-core/              format, limits, ids, canonical JSON, etag, lexer, macros, variables, frames; no I/O
    hclconf/                 HCL engine of Hune, with PROVENANCE; edition = "2021" (below)
    hune-store/             Store trait, volume, PostgreSQL, conformance suite (feature "conformance")
    hune-source/            DataSource trait, ClickHouse HTTP client, frames, limits
    hune-grafana/           the importer (pure functions)
    hune/                   the binary: build.rs, src/main.rs, src/config/ (key table), src/cli/ (output.rs),
                             src/server/, src/access/, tests/ (api, failover)
  web/                       the front end of 9.2; web/js/vendor/chdash/ and web/css/vendor/chdash/ hold the copied engine
  docs/                      this specification, the configuration reference, ACCESS.md, CONFORMANCE.md
  deploy/
    compose/volume/          compose.yaml, config.hcl (17.6)
    compose/postgres/        compose.yaml, config.hcl (17.7)
    kubernetes/              example manifests (17.8)
  build/
    Dockerfile.build         the builder image (17.2)
    Dockerfile               the FROM scratch image (17.5)
  tests/
    images.lock              every test image and archive, with its digest
    ci/                      one script for each CI job, `all.sh` (the merge gate) and `release.sh` (16.17, 17.3)
    compose/                 the compose projects of the test layers
    fixtures/                synthetic data: grafana/, dashboards/, clickhouse/, sql-lex-vectors.json
    golden/                  expansion vectors, reports, documents
    browser/  conformance/  failover/  load/
  tools/
    ste_lint.py              the language check of 16.18
    parity/                  the verification tools of 15.14 (scripts only, no data)
    check-vendor-lock.sh  check-addresses.sh  changelog-section.sh  third-party.sh  update-chdash.sh
  fuzz/                      cargo-fuzz targets (16.16)
  .github/workflows/         ci.yaml, nightly.yaml, conformance.yml, release.yaml
```

The `.gitignore` file holds `/target/`, `/dist/`, `/private/` and `/parity-out/`. It does not hold `/vendor/`: the directory holds the local copy of `mongodb`.

The crate `hclconf` pins `edition = "2021"` in its `Cargo.toml`, and its `PROVENANCE` file records it. Reason: its file
`src/env.rs` calls `std::env::remove_var`, which is an `unsafe` function from the edition 2024. Each crate of the
workspace forbids `unsafe` code (3.4, R5). The other crates take the edition of the workspace. The environment rule of
3.4 (R2) and the check of 16.18 exempt `crates/hclconf/src/env.rs` only.

### 17.2 Build

The build gives one static binary for each target: `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. Each
target builds on a runner of its own architecture: `ubuntu-24.04` for amd64 and `ubuntu-24.04-arm` for arm64. So no build
crosses an architecture.

The builder image comes from `build/Dockerfile.build`. Its base is the official `rust` image of the version of
`rust-toolchain.toml`, pinned by a multi-architecture digest in `tests/images.lock`. It adds `musl-tools` and `jq` from
Debian, and `cargo-deny` at the version of `tests/images.lock`.

```dockerfile
ARG RUST_IMAGE
FROM ${RUST_IMAGE}
ARG CARGO_DENY_VERSION
RUN apt-get update \
 && apt-get install -y --no-install-recommends musl-tools jq \
 && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked "cargo-deny@${CARGO_DENY_VERSION}"
```

The build has two stages. The first stage has network access and fetches the sources of every crate. The second stage has
no network and builds.

```sh
# Stage 1, with network: fetch the sources of every crate of Cargo.lock. The directory vendor/ of the
# repository holds the local copy of mongodb, so the fetched sources go to target/cargo-vendor/.
docker run --rm -v "$PWD:/src" -w /src hune-builder \
  sh -c 'cargo vendor --locked --versioned-dirs target/cargo-vendor > vendor-config.toml'

# Stage 2, without network: build the static binary.
TRIPLE=x86_64-unknown-linux-musl          # aarch64-unknown-linux-musl on the arm64 runner
docker run --rm --network none -v "$PWD:/src" -w /src \
  -e SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" \
  -e HUNE_BUILD_COMMIT="$(git rev-parse HEAD)" \
  -e CARGO_INCREMENTAL=0 \
  -e "CC_x86_64_unknown_linux_musl=musl-gcc" -e "CC_aarch64_unknown_linux_musl=musl-gcc" \
  -e RUSTFLAGS="--remap-path-prefix=/src=/hune --remap-path-prefix=/usr/local/cargo=/cargo" \
  hune-builder \
  cargo build --release --locked --offline --config vendor-config.toml --target "$TRIPLE" -p hune
```

The release profile of `Cargo.toml`:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = "symbols"
debug = false
incremental = false
```

`build.rs` reads `HUNE_BUILD_COMMIT` at build time. `hune version` prints `hune <version> <commit of 12 characters>
<target>`. The same sources, toolchain and builder image give the same bytes. The release job checks it (17.3).

### 17.3 Release workflow

A person starts the workflow `release.yaml` by hand
(`workflow_dispatch`) on `main` with two inputs: `version` (for example `0.1.0`) and `arm64`, a boolean.

1. Check the input. It matches `^[0-9]+\.[0-9]+\.[0-9]+$`. It equals the version of the workspace in `Cargo.toml`.
   `CHANGELOG.md` has the heading `## <version> - <YYYY-MM-DD>`. The tag `v<version>` does not exist.
2. Run the jobs `policy`, `rust`, `store`, `api`, `source`, `browser`, `import`, `image`, `procurauth` and `failover` on
   the commit. All must pass.
3. Build each target two times, in two new containers (17.2). The two binaries of a target must have the same SHA-256.
4. Check each binary: no program interpreter (`readelf -l`), and `hune version` prints the version and the commit.
5. Make the archives (two, or one when `arm64` is `false`) and `SHA256SUMS` (17.4).
6. Make a draft release `v<version>` with the title `Hune v<version>`, the target commit, the archives and `SHA256SUMS`. The notes
   are the version section of `CHANGELOG.md` (`tools/changelog-section.sh`). GitHub creates no tag for a draft.
7. Check the draft: it has exactly these files (three, or two when `arm64` is `false`); download them;
   `sha256sum -c SHA256SUMS` passes.
8. Publish the draft with one call. The publish creates the tag.
9. When the job fails before the publish call starts, delete the draft. After the publish call starts, never delete the
   draft or the release. A person then checks the release and the tag by hand.

The release always has an `amd64` archive (a static musl binary). The workflow builds the `arm64` archive when `arm64` is
`true`. With `arm64` set to `false`, it builds the `amd64` archive only. Use this value when the repository has no
`arm64` runner. `SHA256SUMS` and the draft then hold one archive, and step 7 expects two files.

The workflow gives the value to the CI, and its job `image` also runs on `amd64` only. With `arm64` set to `true`, the workflow
builds and checks both architectures. `ARCHIVES` below holds the names of the archives.

```sh
gh release create "v$V" --draft --target "$GITHUB_SHA" --title "Hune v$V" --notes-file notes.md \
  $ARCHIVES SHA256SUMS
gh release download "v$V" --dir check
(cd check && sha256sum -c SHA256SUMS)
gh release edit "v$V" --draft=false --latest
```

The release job has the permission `contents: write` only. No workflow has `packages: write`, logs in to a registry or
pushes an image (16.18).

#### Release with no GitHub Actions

The scripts `tests/ci/all.sh` and `tests/ci/release.sh` are the release path of the project. They run every job and the release
with no GitHub Actions and no GitHub billing. The workflows in `.github/workflows` are an option. They run the same jobs on GitHub.

The operator makes a release by hand, on `main`, with these steps:

1. Run `tests/ci/all.sh` on the commit. Every job of 16.17 must pass. A full run on a clean work tree writes the commit in
   `target/ci-all.ok`.
2. Run `tests/ci/release-build.sh <version>` on a machine of each architecture. It does the steps 3 to 5 above and makes the
   archive in `dist/`.
3. Copy the archive of the other architecture into `dist/` of the machine that makes the release.
4. Push `main`. The target commit of the release must be on `origin`.
5. Run `tests/ci/release.sh <version>`. Add `--amd64-only` when no arm64 machine exists.

The script `tests/ci/release.sh` does the steps 1 and 5 to 9 above, with `gh` that the operator has logged in. It checks that
`main` and `origin/main` are at the commit, that the work tree is clean, and that `target/ci-all.ok` names the commit. It makes
`dist/SHA256SUMS` and the notes (`tools/changelog-section.sh`). It makes the draft, checks it, and publishes it with one call.
It deletes the draft when a step fails before the publish call. After that call it never deletes anything.

The option `--partial-gate "<text>"` of `tests/ci/release.sh` replaces the check of `target/ci-all.ok`. Use it only when a partial gate is
enough for the release. The text MUST NOT be empty, and the script refuses a text of blanks.

The text names each job
that ran on the commit and each job that did not run. It also says that GitHub Actions did not run. The script prints the text,
and it writes the text in the release notes under the heading "Checks". All other checks of step 1 stay. The script never writes
`target/ci-all.ok` itself.

### 17.4 Artefacts and SHA256SUMS

| File | Content |
|---|---|
| `hune_<version>_linux_amd64.tar.gz` | The directory `hune_<version>_linux_amd64/` (below). |
| `hune_<version>_linux_arm64.tar.gz` | The directory `hune_<version>_linux_arm64/`. The release has it only when the arm64 build runs. |
| `SHA256SUMS` | One line for each archive: the SHA-256 in hex, two spaces, the file name. The output of `sha256sum`. |

Each archive holds one directory with these files:

| File | Content |
|---|---|
| `hune` | The static binary, mode 0755. |
| `Dockerfile` | The image file of 17.5. |
| `config.reference.hcl` | The output of `hune print-reference`. |
| `deploy/` | The examples of 17.6, 17.7 and 17.8. |
| `LICENSE-MIT` | The text of the MIT licence. The project licence is `MIT OR Apache-2.0`. |
| `LICENSE-APACHE` | The text of the Apache-2.0 licence. |
| `LICENSE-THIRD-PARTY` | The licence notice of each crate of `deps.lock.txt`, with the local copy of the crate `mongodb` (Apache-2.0). It also holds the notice of the copied chart engine (MIT, ClickHouse Dash), of the font Inter (SIL OFL 1.1) and of `hclconf` (`tools/third-party.sh`). These parts keep their own licences. |
| `README.md`, `CHANGELOG.md` | The files of the repository. |

The archives are deterministic:

```sh
tar --sort=name --mtime="@${SOURCE_DATE_EPOCH}" --owner=0 --group=0 --numeric-owner --format=gnu \
  -cf - "hune_${V}_linux_${ARCH}" | gzip -9n > "hune_${V}_linux_${ARCH}.tar.gz"
sha256sum "hune_${V}_linux_amd64.tar.gz" "hune_${V}_linux_arm64.tar.gz" > SHA256SUMS
```

An operator checks a download with `sha256sum -c --ignore-missing SHA256SUMS`.

### 17.5 The image

The image is `FROM scratch` and holds the binary only. The operator builds it locally from the release archive, or the CI
builds it for a test. The project publishes no container image. Nobody pushes it to a registry: not GHCR, not Docker Hub, not any other registry. The
release archive holds the `Dockerfile` for a local build.

```dockerfile
# Build locally in the extracted release archive: docker build -t hune:<version> .
# Never push this image to a registry.
FROM scratch
COPY --chmod=0555 hune /hune
USER 65532:65532
ENTRYPOINT ["/hune"]
CMD ["serve", "-c", "/etc/hune/config.hcl"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=20s --retries=3 \
  CMD ["/hune", "health", "-c", "/etc/hune/config.hcl"]
```

The contract of the image:

- The process is PID 1 and handles SIGTERM itself (section 14). There is no shell and no init.
- The image has no CA store. Each TLS link names its CA file in the configuration.
- The image has no writable path. Hune writes only to the volume path of the configuration, which the operator mounts.
- The configuration mounts at `/etc/hune/config.hcl`. Secret files mount under `/run/secrets/` or another path that the
  configuration names.
- `hune health` is the health command, because the image has no `curl` (section 14).

### 17.6 Compose example: volume

This example is the LOCAL use case: one instance, a mounted folder of dashboards, mode `open`. Mode `open` has no
authentication, so the example publishes the main port on `127.0.0.1` only. It does not publish the ops port (12.6): the
health check runs inside the container. A real deployment puts a proxy that checks the
login in front of Hune, for example with the `auth_request` module of nginx (15.15).

Prepare the directory once:

```sh
install -d -m 0750 -o 65532 -g 65532 dashboards
install -d -m 0700 secrets
install -m 0400 -o 65532 -g 65532 /path/to/clickhouse_password secrets/clickhouse_password
install -m 0444 /path/to/clickhouse_ca.pem secrets/clickhouse_ca.pem
docker build -t hune:0.2.0 /path/to/hune_0.2.0_linux_amd64
```

`deploy/compose/volume/compose.yaml`:

```yaml
# Hune with a mounted folder of dashboards: one instance, mode open.
# The image comes from a local build of the release archive. It is never pulled or pushed.
name: hune-volume
services:
  hune:
    image: hune:0.2.0
    pull_policy: never
    user: "65532:65532"
    read_only: true
    cap_drop: [ALL]
    security_opt: ["no-new-privileges:true"]
    command: ["serve", "-c", "/etc/hune/config.hcl"]
    volumes:
      - ./config.hcl:/etc/hune/config.hcl:ro
      - ./dashboards:/var/lib/hune/dashboards
    secrets:
      - source: clickhouse_password
        target: clickhouse_password
      - source: clickhouse_ca
        target: clickhouse_ca.pem
    ports:
      - "127.0.0.1:8080:8080"
    healthcheck:
      test: ["CMD", "/hune", "health", "-c", "/etc/hune/config.hcl"]
      interval: 10s
      timeout: 3s
      start_period: 20s
      retries: 3
    stop_grace_period: 30s
    restart: unless-stopped
secrets:
  clickhouse_password:
    file: ./secrets/clickhouse_password
  clickhouse_ca:
    file: ./secrets/clickhouse_ca.pem
```

`deploy/compose/volume/config.hcl` starts from the file of 13.16. It changes the secret paths, the ClickHouse host,
`shutdown_grace`, `max_request_bytes` and `queue_wait`. It has each key of 13.8 to 13.12a that applies, and no other key
(G09):

```hcl
server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "20s"
  max_request_bytes       = 4259840
  max_result_memory_bytes = 536870912
}

log {
  level = "info"
}

telemetry {
  enabled = false
}

storage {
  volume "main" {
    path            = "/var/lib/hune/dashboards"
    primary         = true
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access {
  mode = "open"
}

clickhouse "prod" {
  label              = "Production"
  url                = "https://clickhouse.example.internal:8443"
  user               = "hune"
  password_file      = "/run/secrets/clickhouse_password"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "60s"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "10s"

  tls {
    ca_file     = "/run/secrets/clickhouse_ca.pem"
    server_name = "clickhouse.example.internal"
  }
}
```

`stop_grace_period` (30 s) is `server.shutdown_grace` (20 s) plus `SHUTDOWN_DRAIN` (5 s) plus 5 s, the minimum of 14.13.
`server.idle_timeout` (75 s) is above the keep-alive time of 60 s of an nginx upstream, as 12.6 asks. `max_request_bytes`
is the document limit of 4 MiB plus 64 KiB, the lowest value of 13.8. The mode `open` forbids
`max_concurrent_per_subject` (13.14, C13).

At a deployment behind an nginx that exists, the same file differs in these values. The compose service joins the network of nginx and publishes no port.

| Key | Value |
|---|---|
| `server.base_path` | `"/dashboards/"` |
| `primary` of the volume | absent during the parallel run (a read-only store); `true` from T (15.15) |
| `clickhouse "prod".url` | `"https://<clickhouse host>:8443"` |
| `clickhouse "prod".database` | `"<database>"` |
| `clickhouse "prod".user` | the account of 19.6 |
| `clickhouse "prod".max_result_rows`, `.max_result_bytes` | the values of the M0 measure (19.13) |

### 17.7 Compose example: PostgreSQL

This example shows several instances on one PostgreSQL. It is NOT a high-availability setup: the one PostgreSQL is a single
point of failure. The reference for high availability is CloudNativePG (17.8). The example uses no TLS to PostgreSQL on the
private compose network (`sslmode = "disable"`), so the start gives the warning W5 of 13.15.

The example uses the two roles of 5.25. `serve` stops at start with a role that is a superuser or that owns the tables
(5.29, step 6). The superuser `postgres` creates the roles at the first start of the database. The owner role
`hune_owner` runs `migrate-store`. The service `grant` then gives the rights of 5.25 to the application role `hune_app`.

Prepare the directory as in 17.6, with three more secret files: `secrets/pg_super_password`, `secrets/pg_owner_password`
and `secrets/pg_app_password`. The user 999 of the image `postgres` and the user 65532 of Hune read them. So these three
files have the mode 0444. The directory `secrets/` has the mode 0700 (`install -d -m 0700 secrets`, 17.6), so that no
other user of the host reads them. The service `migrate` mounts only `pg_owner_password`, because `migrate-store` reads
only the values of the block `storage` (13.18).

`deploy/compose/postgres/compose.yaml`:

```yaml
# Hune with PostgreSQL: two instances, one PostgreSQL and a small load balancer.
# NOT high availability: the one PostgreSQL is a single point of failure.
name: hune-postgres
services:
  postgres:
    image: postgres:18.4-trixie
    environment:
      POSTGRES_DB: hune
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD_FILE: /run/secrets/pg_super_password
    secrets: [pg_super_password, pg_owner_password, pg_app_password]
    configs:
      - source: roles_sh
        target: /docker-entrypoint-initdb.d/10-roles.sh
    volumes:
      - pgdata:/var/lib/postgresql
    healthcheck:
      test: ["CMD", "pg_isready", "-U", "postgres", "-d", "hune"]
      interval: 5s
      timeout: 3s
      retries: 20
    restart: unless-stopped

  migrate:
    image: hune:0.2.0
    pull_policy: never
    user: "65532:65532"
    read_only: true
    cap_drop: [ALL]
    command: ["migrate-store", "-c", "/etc/hune/migrate.hcl"]
    volumes:
      - ./migrate.hcl:/etc/hune/migrate.hcl:ro
    secrets:
      - source: pg_owner_password
        target: pg_owner_password
    depends_on:
      postgres:
        condition: service_healthy
    restart: "no"

  grant:
    image: postgres:18.4-trixie
    entrypoint: ["sh", "-c"]
    command:
      - >-
        PGPASSWORD="$$(cat /run/secrets/pg_super_password)"
        psql -v ON_ERROR_STOP=1 -h postgres -U postgres -d hune
        -c "GRANT SELECT ON hune_schema TO hune_app"
        -c "GRANT SELECT, INSERT, UPDATE, DELETE ON hune_folders, hune_dashboards, hune_versions TO hune_app"
    secrets: [pg_super_password]
    depends_on:
      migrate:
        condition: service_completed_successfully
    restart: "no"

  hune-a: &hune
    image: hune:0.2.0
    pull_policy: never
    user: "65532:65532"
    read_only: true
    cap_drop: [ALL]
    security_opt: ["no-new-privileges:true"]
    command: ["serve", "-c", "/etc/hune/config.hcl"]
    volumes:
      - ./config.hcl:/etc/hune/config.hcl:ro
    secrets:
      - source: pg_app_password
        target: pg_app_password
      - source: clickhouse_password
        target: clickhouse_password
      - source: clickhouse_ca
        target: clickhouse_ca.pem
    healthcheck:
      test: ["CMD", "/hune", "health", "-c", "/etc/hune/config.hcl"]
      interval: 10s
      timeout: 3s
      start_period: 20s
      retries: 3
    stop_grace_period: 30s
    depends_on:
      grant:
        condition: service_completed_successfully
    restart: unless-stopped

  hune-b: *hune

  lb:
    image: nginx:1.28-alpine
    configs:
      - source: lb_conf
        target: /etc/nginx/conf.d/default.conf
    ports:
      - "127.0.0.1:8080:8080"
    depends_on:
      hune-a:
        condition: service_healthy
      hune-b:
        condition: service_healthy
    restart: unless-stopped

configs:
  roles_sh:
    content: |
      #!/bin/sh
      set -eu
      psql -v ON_ERROR_STOP=1 --username postgres --dbname hune \
        -v owner_pw="$$(cat /run/secrets/pg_owner_password)" \
        -v app_pw="$$(cat /run/secrets/pg_app_password)" <<'SQL'
      CREATE ROLE hune_owner LOGIN PASSWORD :'owner_pw';
      CREATE ROLE hune_app LOGIN PASSWORD :'app_pw';
      GRANT CONNECT ON DATABASE hune TO hune_owner, hune_app;
      GRANT CREATE, USAGE ON SCHEMA public TO hune_owner;
      GRANT USAGE ON SCHEMA public TO hune_app;
      SQL
  lb_conf:
    content: |
      upstream hune {
        server hune-a:8080;
        server hune-b:8080;
      }
      server {
        listen 8080;
        location / {
          proxy_pass http://hune;
          proxy_set_header Host $$http_host;
          proxy_set_header X-Forwarded-For $$proxy_add_x_forwarded_for;
          proxy_read_timeout 120s;
        }
      }

secrets:
  pg_super_password:
    file: ./secrets/pg_super_password
  pg_owner_password:
    file: ./secrets/pg_owner_password
  pg_app_password:
    file: ./secrets/pg_app_password
  clickhouse_password:
    file: ./secrets/clickhouse_password
  clickhouse_ca:
    file: ./secrets/clickhouse_ca.pem

volumes:
  pgdata: {}
```

`$$` is the compose form of one `$`. The load balancer sends `$http_host`, which keeps the port of the browser, so the
`Origin` check of 12.7 passes. The image `postgres` runs the script `10-roles.sh` only at the first start of an empty data
volume. `deploy/compose/postgres/config.hcl` is the file of 17.6 with this `storage` block:

```hcl
storage {
  postgres {
    host              = "postgres"
    port              = 5432
    database          = "hune"
    user              = "hune_app"
    password_file     = "/run/secrets/pg_app_password"
    sslmode           = "disable"
    max_connections   = 8
    connect_timeout   = "5s"
    statement_timeout = "10s"
    poll_interval     = "5s"
    primary           = true
    history_keep      = 100
  }
}
```

`deploy/compose/postgres/migrate.hcl` is a complete file too (5.25). It equals `config.hcl`, except two keys of
`storage.postgres`: `user = "hune_owner"` and `password_file = "/run/secrets/pg_owner_password"`.

### 17.8 Kubernetes example manifests

The files of `deploy/kubernetes/` are documentation: an example of the DISTRIBUTED use case on K3s. The CI checks them with
`kubeconform` (G08), but no test applies them.

The reference for high availability is a CloudNativePG cluster that the operator already runs. The example uses the cluster
`postgresql` in the namespace `data`. That cluster has CloudNativePG 1.30, PostgreSQL
18.4, three instances, and a synchronous quorum with `dataDurability: required`.

The example holds one Deployment, one Job and the CloudNativePG objects, with the ConfigMap and the Service that they
need. It holds no PodDisruptionBudget, no NetworkPolicy and no backup job: these depend on the cluster of the operator.
The daily `hune export` of 5.41 is a job of the operator (`hune export -c <file> --to <empty dir>`, 5.38).

| File | Content |
|---|---|
| `10-config.yaml` | The ConfigMap `hune-config-0-4-0` with two complete files (5.25): `config.hcl` (application role) and `migrate.hcl` (owner role). Its name holds the version. |
| `20-migrate-job.yaml` | The Job of `hune migrate-store` for one version, with `migrate.hcl`. Its name holds the version. |
| `30-deployment.yaml` | The Deployment (2 replicas) and the Service `hune` (ports 8080 and 9090). |
| `cnpg/database.yaml` | The CloudNativePG `Database` resource, in the namespace `data`. |

Each version has its own ConfigMap, because a new version can add, remove or rename keys (13.26). The Deployment of a
version names the ConfigMap of the same version. So a rollback to the old Deployment also reads the old file.

The image never comes from a registry. The operator builds it from the release archive and imports it on each node. The
Deployment uses `imagePullPolicy: Never`:

```sh
docker build -t hune:0.2.0 /path/to/hune_0.2.0_linux_amd64
docker save hune:0.2.0 | sudo k3s ctr images import -
```

The secrets never enter a file of the repository. The operator makes them once:

```sh
kubectl create namespace hune
for role in owner app; do
  pw="$(openssl rand -base64 30)"
  kubectl -n data create secret generic "hune-${role}-role" --type=kubernetes.io/basic-auth \
    --from-literal=username="hune_${role}" --from-literal=password="$pw"
  kubectl -n hune create secret generic "hune-pg-${role}" --from-literal=password="$pw"
done
kubectl -n data get secret postgresql-ca -o jsonpath='{.data.ca\.crt}' | base64 -d > ca.crt
kubectl -n hune create secret generic hune-pg-ca --from-file=ca.crt
rm ca.crt
kubectl -n hune create secret generic hune-clickhouse \
  --from-file=password=/path/to/clickhouse_password --from-file=ca.crt=/path/to/clickhouse_ca.pem
kubectl -n hune create secret generic hune-edge --from-file=jwks.json=/path/to/edge-jwks.json
kubectl -n hune create secret generic hune-pdp \
  --from-file=token=/path/to/pdp_token --from-file=ca.crt=/path/to/pdp_ca.pem
```

The two roles of 5.25 go in the `spec` of the `Cluster` of the operator. Neither role is a superuser:

```yaml
spec:
  managed:
    roles:
      - name: hune_owner
        ensure: present
        login: true
        passwordSecret:
          name: hune-owner-role
      - name: hune_app
        ensure: present
        login: true
        passwordSecret:
          name: hune-app-role
```

`cnpg/database.yaml`:

```yaml
apiVersion: postgresql.cnpg.io/v1
kind: Database
metadata:
  name: hune
  namespace: data
spec:
  cluster:
    name: postgresql
  name: hune
  owner: hune_owner
```

The owner of the database owns its schema `public`, so `hune_owner` can create the tables of Hune. After the first
`migrate-store`, the operator gives the rights of 5.25 to `hune_app`, on the primary:

```sh
primary="$(kubectl -n data get pod -l cnpg.io/cluster=postgresql,cnpg.io/instanceRole=primary -o name)"
kubectl -n data exec "$primary" -c postgres -- psql -v ON_ERROR_STOP=1 -d hune \
  -c "GRANT SELECT ON hune_schema TO hune_app" \
  -c "GRANT SELECT, INSERT, UPDATE, DELETE ON hune_folders, hune_dashboards, hune_versions TO hune_app"
```

`10-config.yaml` holds the mode `authzen` behind the ProcurAuth edge. Its key `config.hcl` starts from the file of 13.17,
with the telemetry off, and names the application role. Its key `migrate.hcl` holds the same text, except two lines of
`storage.postgres`: `user = "hune_owner"` and `password_file = "/etc/hune/secrets/pg-owner/password"`. The file of the
repository holds both keys in full. The key `config.hcl`:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: hune-config-0-4-0
  namespace: hune
data:
  config.hcl: |
    server {
      listen                  = "0.0.0.0:8080"
      ops_listen              = "0.0.0.0:9090"
      base_path               = "/"
      max_connections         = 1024
      idle_timeout            = "75s"
      shutdown_grace          = "20s"
      max_request_bytes       = 4259840
      max_result_memory_bytes = 536870912
    }

    log {
      level = "info"
    }

    telemetry {
      enabled = false
    }

    storage {
      postgres {
        host              = "postgresql-rw.data.svc"
        port              = 5432
        database          = "hune"
        user              = "hune_app"
        password_file     = "/etc/hune/secrets/pg-app/password"
        sslmode           = "verify-full"
        max_connections   = 8
        connect_timeout   = "5s"
        statement_timeout = "10s"
        poll_interval     = "5s"
        primary           = true
        history_keep      = 100

        tls {
          ca_file     = "/etc/hune/secrets/pg-ca/ca.crt"
          server_name = "postgresql-rw.data.svc"
        }
      }
    }

    access {
      mode = "authzen"

      identity {
        header           = "X-Identity-Token"
        issuer           = "https://portal.example.internal"
        audience         = "dashboards"
        public_keys_file = "/etc/hune/secrets/edge/jwks.json"
        max_age          = "5m"
        clock_skew       = "30s"
      }

      pdp {
        url             = "https://pdp.procurauth.svc:8443"
        token_file      = "/etc/hune/secrets/pdp/token"
        ca_file         = "/etc/hune/secrets/pdp/ca.crt"
        plain_http      = false
        timeout         = "500ms"
        cache_ttl       = "30s"
        max_connections = 4
      }
    }

    clickhouse "prod" {
      label                      = "Production"
      url                        = "https://clickhouse.example.internal:8443"
      user                       = "hune"
      password_file              = "/etc/hune/secrets/clickhouse/password"
      database                   = "analytics"
      connect_timeout            = "5s"
      max_execution_time         = "60s"
      max_result_rows            = 100000
      max_result_bytes           = 33554432
      max_query_size             = 1048576
      max_concurrent             = 16
      queue_wait                 = "10s"
      max_concurrent_per_subject = 6

      tls {
        ca_file     = "/etc/hune/secrets/clickhouse/ca.crt"
        server_name = "clickhouse.example.internal"
      }
    }
```

`20-migrate-job.yaml` (the name holds the version, so each version has its own Job). `migrate-store` uses only the block
`storage`, so the Job mounts only the PostgreSQL secrets. This needs the rule of 13.18 by which a subcommand reads only
the `_env` and `_file` values of the blocks that it uses:

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: hune-migrate-0-4-0
  namespace: hune
spec:
  backoffLimit: 0
  ttlSecondsAfterFinished: 86400
  template:
    metadata:
      labels:
        app.kubernetes.io/name: hune-migrate
    spec:
      restartPolicy: Never
      automountServiceAccountToken: false
      securityContext:
        runAsNonRoot: true
        runAsUser: 65532
        runAsGroup: 65532
        fsGroup: 65532
        seccompProfile:
          type: RuntimeDefault
      containers:
        - name: migrate
          image: hune:0.2.0
          imagePullPolicy: Never
          args: ["migrate-store", "-c", "/etc/hune/migrate.hcl"]
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: ["ALL"]
          volumeMounts:
            - { name: config, mountPath: /etc/hune/migrate.hcl, subPath: migrate.hcl, readOnly: true }
            - { name: pg-owner, mountPath: /etc/hune/secrets/pg-owner, readOnly: true }
            - { name: pg-ca, mountPath: /etc/hune/secrets/pg-ca, readOnly: true }
      volumes:
        - { name: config, configMap: { name: hune-config-0-4-0 } }
        - { name: pg-owner, secret: { secretName: hune-pg-owner, defaultMode: 0440 } }
        - { name: pg-ca, secret: { secretName: hune-pg-ca, defaultMode: 0440 } }
```

`30-deployment.yaml`. `terminationGracePeriodSeconds` (30 s) is `server.shutdown_grace` (20 s) plus `SHUTDOWN_DRAIN`
(5 s) plus 5 s, the minimum of 14.13:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hune
  namespace: hune
  labels:
    app.kubernetes.io/name: hune
spec:
  replicas: 2
  revisionHistoryLimit: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 0
      maxSurge: 1
  selector:
    matchLabels:
      app.kubernetes.io/name: hune
  template:
    metadata:
      labels:
        app.kubernetes.io/name: hune
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "9090"
        prometheus.io/path: /metrics
    spec:
      automountServiceAccountToken: false
      terminationGracePeriodSeconds: 30
      securityContext:
        runAsNonRoot: true
        runAsUser: 65532
        runAsGroup: 65532
        fsGroup: 65532
        seccompProfile:
          type: RuntimeDefault
      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: kubernetes.io/hostname
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels:
              app.kubernetes.io/name: hune
      containers:
        - name: hune
          image: hune:0.2.0
          imagePullPolicy: Never
          args: ["serve", "-c", "/etc/hune/config.hcl"]
          ports:
            - { name: http, containerPort: 8080 }
            - { name: ops, containerPort: 9090 }
          readinessProbe:
            httpGet: { path: /readyz, port: ops }
            periodSeconds: 5
            failureThreshold: 2
          livenessProbe:
            httpGet: { path: /livez, port: ops }
            periodSeconds: 10
            failureThreshold: 3
          resources:
            requests: { cpu: 100m, memory: 128Mi }
            limits: { memory: 1Gi }
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities:
              drop: ["ALL"]
          volumeMounts:
            - { name: config, mountPath: /etc/hune/config.hcl, subPath: config.hcl, readOnly: true }
            - { name: pg-app, mountPath: /etc/hune/secrets/pg-app, readOnly: true }
            - { name: pg-ca, mountPath: /etc/hune/secrets/pg-ca, readOnly: true }
            - { name: clickhouse, mountPath: /etc/hune/secrets/clickhouse, readOnly: true }
            - { name: edge, mountPath: /etc/hune/secrets/edge, readOnly: true }
            - { name: pdp, mountPath: /etc/hune/secrets/pdp, readOnly: true }
      volumes:
        - { name: config, configMap: { name: hune-config-0-4-0 } }
        - { name: pg-app, secret: { secretName: hune-pg-app, defaultMode: 0440 } }
        - { name: pg-ca, secret: { secretName: hune-pg-ca, defaultMode: 0440 } }
        - { name: clickhouse, secret: { secretName: hune-clickhouse, defaultMode: 0440 } }
        - { name: edge, secret: { secretName: hune-edge, defaultMode: 0440 } }
        - { name: pdp, secret: { secretName: hune-pdp, defaultMode: 0440 } }
---
apiVersion: v1
kind: Service
metadata:
  name: hune
  namespace: hune
spec:
  selector:
    app.kubernetes.io/name: hune
  ports:
    - { name: http, port: 8080, targetPort: http }
    - { name: ops, port: 9090, targetPort: ops }
```

The order of a rollout:

1. Build the image and import it on each node.
2. Make the namespace and the secrets. Apply the two roles, `cnpg/database.yaml` and `10-config.yaml`.
3. Apply `20-migrate-job.yaml`. Wait:
   `kubectl -n hune wait --for=condition=complete --timeout=5m job/hune-migrate-0-4-0`.
4. Give the rights of 5.25 to `hune_app` (the `psql` command above).
5. Apply `30-deployment.yaml`. Wait: `kubectl -n hune rollout status deployment/hune`.

The edge routes the prefix of Hune to port 8080 of the Service `hune`. Prometheus reads `/metrics` on its port 9090; the
edge never routes to this port. The CloudNativePG backups are the backup of the database. `hune export` (5.38), from a
job of the operator, is the portable backup.

A failover of CloudNativePG lasts 1 to 2 minutes in this setup. During this time, reads come from the catalog and
saves fail with 503; the editor keeps the draft (section 10).

### 17.9 Supply chain

- `Cargo.lock` is committed. Every build and every test uses `--locked`.
- Each direct dependency has an exact pin (`=x.y.z`), `default-features = false`, and a comment of one line with its reason.
- `procurauth-pep` and `procurauth-core` are git dependencies of `https://github.com/ProcurAuth/procurauth`. Their `rev` is
  one full commit of 40 hex characters, and they have `default-features = false`. The job of 16.12 uses the same commit.
- `crates/hclconf/` is written in Hune and is not a copy. Its `PROVENANCE` file says so, gives the
  comparison with another HCL engine and records the edition.
- `web/js/vendor/chdash/` and `web/css/vendor/chdash/` hold a copy of the ClickHouse Dash chart engine at a recorded
  commit, with the MIT notice. The lock file `web/js/vendor/chdash/chdash.lock.json` holds the SHA-256 of each file (9.24). `tools/update-chdash.sh` prints the upstream differences for a manual
  update. Nothing updates the copy automatically.
- The repository commits one directory under `vendor/`: the local copy of `mongodb` (8b.2). The file `vendor/mongodb.lock.json` holds the checksum of the original crate and the SHA-256 of each file, and `vendor/mongodb.patch` holds the edits. After an edit, `tools/check-vendor-lock.sh --mongodb-update` writes both again, and the job `policy` checks them. The build of 17.2 makes `target/cargo-vendor/` in stage 1 and builds offline in stage 2. Nothing goes to the upstream repository of the driver.
- `rust-toolchain.toml` names one exact version. `tests/images.lock` pins every image by digest.

The MongoDB source uses two crates from crates.io:

- `mongodb` =3.10.0 (Apache-2.0), the official driver, as a dependency of `crates/hune-source` behind the cargo feature `mongo-driver`. It has no default features. The features are `compat-3-3-0`, `bson-3` and `rustls-tls`. The build uses the patched copy `vendor/mongodb/` (8b.2), and `deps.lock.txt` lists it as `mongodb 3.10.0 (vendor)`.
- `bson` =3.1.0 (MIT), the BSON types of the driver. It reads no environment variable, so it is not a patched copy.
- Two crates of the graph have a licence outside the `allow` list of `deny.toml`: `webpki-roots` (CDLA-Permissive-2.0) and `tiny-keccak` (CC0-1.0, a crate of the build). `deny.toml` gives each crate its own exception, and the list stays the same. `webpki-roots` is also a ban with the exception `wrappers = ["mongodb"]`: a test proves that the driver trusts no root other than `tls.ca_file`.

`deny.toml`:

```toml
[graph]
targets = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"]

[advisories]
version = 2
yanked = "deny"
ignore = [
  { id = "RUSTSEC-2026-0387", reason = "Hune only verifies ES256 signatures with public keys; remove when procurauth-core moves to p256 0.14" },
]

[licenses]
version = 2
allow = ["Apache-2.0", "MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]
confidence-threshold = 0.93
exceptions = [
  { allow = ["CDLA-Permissive-2.0"], name = "webpki-roots" },
  { allow = ["CC0-1.0"], name = "tiny-keccak" },
]

[bans]
multiple-versions = "warn"
wildcards = "deny"
allow-wildcard-paths = true
deny = [
  { name = "openssl" }, { name = "openssl-sys" }, { name = "native-tls" },
  { name = "aws-lc-rs" }, { name = "aws-lc-sys" },
  { name = "reqwest" }, { name = "sqlx" }, { name = "axum" },
  { name = "rustls-native-certs" }, { name = "rustls-platform-verifier" },
  { name = "webpki-roots", wrappers = ["mongodb"] },
  { name = "hickory-resolver" }, { name = "hickory-proto" }, { name = "aws-config" }, { name = "aws-sigv4" },
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = ["https://github.com/ProcurAuth/procurauth"]
```

The bans hold these rules:

- one TLS stack, rustls with `ring`;
- no HTTP client that reads proxy variables;
- no CA store of the system and no bundled roots;
- no axum and no sqlx;
- no SRV resolver and no AWS authentication for the MongoDB driver (8b.4).

`allow-wildcard-paths = true` accepts a dependency with no version from a crate of the workspace to another crate of
the workspace or to a ProcurAuth crate. Such a dependency has a path or a pinned git commit. Each crate of the workspace has `publish = false`. A registry
dependency with the version `*` still fails.

The list `ignore` of `[advisories]` has one advisory, which the project accepted on 2026-11-22. RUSTSEC-2026-0387 is in `crypto-bigint` 0.5, through `p256` 0.13 of `procurauth-core`. Hune only verifies ES256 signatures with public keys, and only the tests sign. Remove the entry when `procurauth-core` moves to `p256` 0.14.

A new licence, an exception or an ignored advisory needs a change of `deny.toml` in a reviewed commit.

`deps.lock.txt` holds one line `<name> <version>` for each crate of the resolved graph of the two targets, outside the
workspace, sorted in byte order (`LC_ALL=C`). The `policy` job compares it with this output:

```sh
for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  cargo metadata --locked --format-version 1 --filter-platform "$t" \
    | jq -r '(.resolve.nodes | map(.id)) as $ids
             | .packages[] | select(.id as $i | $ids | index($i)) | select(.source != null)
             | "\(.name) \(.version)"'
done | LC_ALL=C sort -u
```

The script `tests/ci/policy.sh` also lists the packages whose manifest is under `vendor/`, with the tag ` (vendor)` after the version. The line `mongodb 3.10.0 (vendor)` shows that the crate is a local copy. The command `tests/ci/policy.sh --print-deps` prints the list. The byte order makes the list equal on the host and in the builder container. The rule `wrappers = ["mongodb"]` of `deny.toml` names the package, so it works for the copy.

So each new crate, and each new version, is a visible line in the diff of a commit. The reviewer checks it.

### 17.10 Versioning and the changelog

Hune uses Semantic Versioning 2.0.0. The first release is 0.1.0. The release 1.0.0 comes after the exit criteria of M7
(section 18).

The contract of a version has six parts. They are the command line, the configuration keys, the document format, the
store schema, the metric names and the log events. The HTTP API is not in the contract, because the front end ships in the
same binary (section 6).

| Change | Before 1.0.0 | From 1.0.0 |
|---|---|---|
| A key, a subcommand, a metric or an event is removed or renamed | minor | major |
| A new `format_version` (stop, `migrate-documents`, start) | minor | major |
| A store migration that an old binary cannot read | minor | major |
| A new key, subcommand, metric, event or additive store migration | minor | minor |
| A fix with no change of the contract | patch | patch |

`CHANGELOG.md` uses ASD-STE100. Its rules:

- The file starts with the section `## Unreleased`. Then each version follows, newest first, as `## <version> - <YYYY-MM-DD>`.
- A section has the groups `Added`, `Changed`, `Removed`, `Fixed` and `Security`, in this order. An empty group is left out.
  A release section can end with the group `Known limits`.
- A commit can change the contract, the behaviour, a dependency or a deployment file. Such a commit adds its line under
  `## Unreleased`. The reviewer refuses a change without its line.
- The line of a change that breaks the contract starts with `BREAKING:`. It names the update step of 17.11 that it needs.
- The release commit renames `## Unreleased` to the version and the date, and adds a new empty `## Unreleased`. The release
  notes are that section (17.3).

### 17.11 Update procedure

Read the section of `CHANGELOG.md` for each version between the current and the new version. A `BREAKING:` line names a
step below.

The runbooks of 14.18 to 14.20 give the operation steps. This paragraph gives the delivery view.

Volume, compose:

1. Make a backup: `hune export -c config.hcl --to <dir>` (5.38), or copy the dashboard folder.
2. Build the new image from the new release archive (17.5).
3. When the new version has a new `format_version`, stop Hune. Run `hune migrate-documents -c config.hcl` with the new
   image, read its dry run, then run it with `--write` (4.42, 14.20).
4. Change the image tag in the compose file. Run `docker compose up -d --no-deps hune`.
5. Check: `/readyz` gives 200, `hune version` prints the new version, and the start log holds the configuration.

PostgreSQL, compose or Kubernetes:

1. Make a backup: `hune export` to a directory, and check the last backup of CloudNativePG.
2. Build and import the new image (17.8).
3. When the new version has a new store schema, scale the Deployment to 0 (or stop the compose instances). Then run
   `hune migrate-store` with the new image and `migrate.hcl`, as the Job of 17.8 (14.19). An old binary refuses the new
   schema. Then give to the application role the grants that the `CHANGELOG.md` names for this migration (5.25).
4. When the new version has a new `format_version`, scale the Deployment to 0 and run `hune migrate-documents` as in the
   volume case (4.42).
5. Set the new image in the compose file or the Deployment, and apply it. Check as in step 5 above, on each instance.

No step migrates down. A return to the old version after a migration of the store or of the documents needs a new store.
Start the old version on a new empty store, and run `hune import` of the backup of step 1. Without a migration, the old
image tag is enough.
