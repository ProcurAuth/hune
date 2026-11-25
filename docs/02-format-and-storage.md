# Part 2: Document format and storage

Part 2 of the Hune specification (see `01-overview.md`).

## 4. Document format

### 4.1 Principle

Hune stores each dashboard in its own JSON format: format 3. The format is closed. A document with an unknown member, a duplicate member or a value of a wrong type is invalid.

The concepts of the format are those of the Grafana v2 schema (`dashboard.grafana.app/v2`). These are sections of rows, a grid of 24 columns and the 7 panel types. They also include field defaults and overrides, thresholds, mappings, units, variables and macros. Thus the importer of section 15 maps each Grafana member to one member of format 3. Hune does not read Grafana JSON as a dashboard: only `hune import-grafana` reads it.

The format is not Grafana JSON. A member that Grafana does not have is optional, as `placement` of a variable is (4.7a). A document that does not use such a member has the form that it had before. The import of Grafana never makes such a member, so it stays lossless.

Format 2 added the panel member `transparent` to format 1 (4.13). Format 3 adds six features to format 2 (4.5a, 4.6, 4.15, 4.18, 4.19). The table of 4.42 lists them. `hune-core` holds the types, the parser, the validator and the writer of the canonical form. The server, the importer, the checks of the editor and the tests use this one implementation.

### 4.2 Types and general rules

| Term | Rule |
|---|---|
| document | One JSON object (RFC 8259) in UTF-8 |
| string | A JSON string. It MUST NOT contain U+0000. |
| text | A string with no control character (U+0001 to U+001F and U+007F), unless the table of the member permits one |
| characters | Unicode scalar values, counted after the JSON escapes are decoded |
| bytes | UTF-8 bytes, counted after the JSON escapes are decoded |
| integer | A JSON number with digits and an optional minus sign only: no fraction, no exponent |
| number | Any JSON number. The parser converts it to the nearest IEEE 754 double. The double MUST be finite. |
| boolean | `true` or `false` |
| null | `null`, only where the table says "or null" |
| enum | A string from a closed list |

1. Each member of a table MUST be in its object. The exceptions are the property set of 4.18, the optional members of 4.15 and the optional members of 4.26 to 4.30. The member `placement` of a variable is optional too (4.7a). The member `repeat` of a panel and of a section is optional too (4.13a). The members `extra_queries` of `data`, and `ref` and `hide` of a query, are optional too (4.15a). The members `data`, `transforms` and `fields` are not in a panel of the type `text` (4.33a).
2. A member with no value takes `null`, only where the table permits it.
3. A member that the table does not list makes the document invalid (`unknown_member`).
4. A member name that occurs two times in one object makes the document invalid (`duplicate_member`).
5. The reader accepts the members of an object in any order. The canonical form (4.38) writes them in the order of the table.
6. A tagged object has a member `kind` that selects its other members. A panel uses the member `type` for the same function.
7. The document holds no storage data: no folder, no version, no etag, no author. The store keeps these values (section 5).

### 4.3 Identifiers

| Identifier | Rule | Unique in |
|---|---|---|
| `uid` of a dashboard | `^[a-z0-9][a-z0-9-]{0,39}$` | The store (all folders) |
| Folder id | `^[a-z0-9][a-z0-9-]{0,39}$` | The store |
| Source id (each member `source`) | `^[a-z0-9][a-z0-9-]{0,39}$` | The configuration (a block `clickhouse "<id>"`, `postgres "<id>"` or `mongodb "<id>"`, section 13) |
| Panel `id` | Integer 1 to 2147483647 | The document |
| Variable `name` | `^[A-Za-z_][A-Za-z0-9_]{0,63}$`, and not `__` as the first two characters | The document |
| `ref` of a query (4.15a) | `^[A-Za-z][A-Za-z0-9_]{0,15}$`. Absent in `data.query`: `A`. | The panel |

1. A uid and a folder id have lower-case letters only. Thus a case-insensitive file system cannot merge two ids. The 13 real uids of the reference Grafana fit this rule.
2. The client always gives the uid, in the member `uid` of the document that it creates. The server never makes a uid. A uid that exists gives `Exists` (5.6). The page of the editor makes the uid of a new dashboard (10.7). The importer keeps or makes the uid of each Grafana dashboard (section 15).
3. The folder id does not change after the folder is created. The folder title can change (5.4).
4. The folder id `general` has no special rule in the store. The importer puts the Grafana root dashboards in it (section 15).
5. The format does not check that a source id names a block of the configuration. Thus a document is valid on every instance. The document does not hold the kind of a source: the member `query` is the same for each kind. Section 7 checks the source at save and at run, with the dialect of its kind (7.19).
6. A panel id does not change when the document is saved. The editor gives a new panel the highest id of the document plus 1 (section 10).
7. A variable name that starts with `__` is reserved for the built-in names of section 7.

### 4.4 Durations and time texts

The grammars use ABNF (RFC 5234):

```abnf
duration  = count unit
count     = %x31-39 *8DIGIT              ; 1 to 999999999, the first digit is not 0
unit      = "ms" / "s" / "m" / "h" / "d"  ; m = minute
time-text = relative / absolute
relative  = "now" [ "-" count tunit ] [ "/" tunit ]
tunit     = "s" / "m" / "h" / "d" / "w" / "M" / "y"  ; m = minute, M = month
absolute  = "0" / ( %x31-39 *12DIGIT )   ; epoch milliseconds, at most 4294967295999
```

1. A duration has one unit only. The text `1m30s` is invalid.
2. Each member that holds a duration gives its range in its table.
3. An absolute time is in epoch milliseconds (UTC). Its maximum is 4294967295999 (2106-02-07T06:28:15.999Z), the bound of section 7 (7.3).
4. The browser resolves a relative time to epoch milliseconds (section 9). The server receives absolute values only (section 7).

| Text | Valid | Reason |
|---|---|---|
| `now` | yes | |
| `now-6h` | yes | |
| `now/d` | yes | |
| `now-1d/d` | yes | |
| `1731100000000` | yes | |
| `now-0h` | no | `count` starts at 1 |
| `now+1h` | no | only `-` |
| `now-6H` | no | unit `H` is unknown |
| `01731100000000` | no | first digit 0 |
| `4294967296000` | no | above the maximum |
| `90s` as a duration | yes | |
| `1.5m` as a duration | no | `count` is an integer |

### 4.5 Top-level object

| Member | Type | Rule |
|---|---|---|
| `format_version` | integer | `3` (4.42) |
| `uid` | text | 4.3. It MUST equal the location of the document. |
| `title` | text | 1 to 256 characters |
| `description` | text | 0 to 4096 characters |
| `tags` | array of text | 0 to 32 items; each item 1 to 64 characters; no two equal items |
| `time` | object | 4.6 |
| `cursor_sync` | enum | `off`: each chart has its own cursor. `crosshair`: the cursor of one time chart shows on all time charts of the dashboard. |
| `links` | array of objects | 0 to 16 (`MAX_DASHBOARD_LINKS`) dashboard links (4.5a). The member is necessary. An empty array means "no link". |
| `variables` | array of objects | 0 to 64 variables (4.7) |
| `annotations` | array of objects | Optional. 1 to 8 (`MAX_ANNOTATIONS`) annotations (4.5b). Absent means "none". The canonical form leaves out an empty array. |
| `sections` | array of objects | 1 to 50 sections (4.12) |

The location of a document is the file stem in a volume (5.9) and the column `uid` in PostgreSQL (5.24). The validation takes the expected uid as an argument. A difference gives the error `uid_mismatch` at the pointer `/uid`.

The member `annotations` follows `variables` and comes before `sections` in the canonical form (4.38). A document with an empty array is valid, and its canonical form has no member `annotations`.

### 4.5a Dashboard links

A dashboard link is a link in the toolbar row of the dashboard (9.8a). It is a tagged object. The member `kind` is `link` or `dashboards`. The panel links and the data links of 4.23 are not dashboard links.

Kind `link`: a link to one URL.

| Member | Type | Rule |
|---|---|---|
| `kind` | enum | `link` |
| `title` | text | 1 to 256 characters |
| `url` | text | 1 to 2048 bytes: a URL template (4.23) |
| `target_blank` | boolean | `true`: the link opens a new tab with `rel="noopener noreferrer"` |
| `include_vars` | boolean | `true`: the page adds the selection of each variable to the URL (9.8a) |
| `keep_time` | boolean | `true`: the page adds the time range to the URL (9.8a) |

Kind `dashboards`: a link to each dashboard that has all the tags.

| Member | Type | Rule |
|---|---|---|
| `kind` | enum | `dashboards` |
| `title` | text | 0 to 256 characters. With `as_dropdown` = `true`, 1 to 256 characters. |
| `tags` | array of text | 0 to 8 (`MAX_LINK_TAGS`) items. Each item has 1 to 64 characters. No two items are equal. An empty array selects each dashboard that the viewer can see. |
| `as_dropdown` | boolean | `true`: one menu with the title, and one item for each dashboard. `false`: one link for each dashboard. |
| `target_blank` | boolean | As in the kind `link` |
| `include_vars` | boolean | As in the kind `link` |
| `keep_time` | boolean | As in the kind `link` |

1. A member of the other kind gives `unknown_member`.
2. The URL rule of 4.23 applies to `url`. The prefix of the template MUST pass the check, and a bad prefix gives `bad_format`.
3. The array has the order that the page shows.
4. A dashboard link stores no uid and no title of another dashboard. The kind `dashboards` selects by tag only, so a rename or a delete of a dashboard changes no link.
5. A link of the kind `link` to another dashboard of Hune holds the full path with the base path (6.1), for example `/dashboards/d/<uid>`. A URL that starts with `/` is relative to the host, not to the base path.

### 4.5b Annotations

An annotation is a saved query that gives time marks for the time series panels (9.19d). It is the Grafana "annotation query". The page never sends its SQL: it names the annotation by its index (6.21a). The source of an annotation can be a ClickHouse, a PostgreSQL or a MongoDB source.

| Member | Type | Rule |
|---|---|---|
| `name` | text | 1 to 256 characters. It MUST be unique in the document. It is the label of the annotation in the editor and in the notes of the page. |
| `source` | text | A source id (4.3). The source MUST have the kind `clickhouse`, `postgres` or `mongodb` (8.2). The query uses the language of the kind of its source (rule 3). |
| `sql` | array of text | The lines of the query (4.16): SQL for a `clickhouse` or a `postgres` source, the query text of 7.28 for a `mongodb` source. |
| `color` | text | A colour of 4.19: a token, a hex colour or an `rgb` colour. The marks of the annotation use it. |
| `enabled` | boolean | `false`: the page runs no query for the annotation. Optional. Default `true`. The canonical form leaves out `true`. |
| `panels` | array of integers | Optional. 1 to 200 (`MAX_ANNOTATION_PANELS`) panel ids. The annotation draws on these panels only. Absent: it draws on each time series panel. |

1. The members come in the order of the table (4.38). `name`, `source`, `sql` and `color` are necessary. A member that the table does not list gives `unknown_member`.
2. The query has no `format`, no `min_interval`, no `max_data_points` and no time override. The frame always has the format `table`, and the range is the range of the dashboard (9.19d).
3. The query can use the placeholders and the macros of its language as the query of a `query` variable does: 7.5 to 7.13 for `clickhouse`, 7.19 to 7.26 for `postgres`, and 7.27 to 7.36 for `mongodb`. The macros that need the interval give `not_permitted` at save (7.18 item 5, 7.26, 7.36 item L9). An annotation is not a panel, so no interval exists for it.
4. `panels` holds each id at most one time (`not_unique` at the second item). Each id MUST name a panel of the document (`bad_reference` at `/annotations/<i>/panels/<k>`). A panel that is not a `timeseries` is valid in the list and gets no mark. The array keeps the order that the author wrote.
5. A panel `reuse` and each copy of a `repeat` use the id of their panel. A new panel made by "Duplicate" has a new id, so a list `panels` does not name it.
6. An annotation has no field `id` and no link. The text of its marks is data from the source. It is never markup (9.19d).
7. The columns of the result and the limits of the marks are in 7.37. A document is valid when the SQL is valid. The check of the columns happens at run time, because the source gives them.
8. The Grafana built-in item "Annotations & Alerts" has no form here. Hune has no alert and no stored annotation (15.6).
9. The kind of the source is a fact of the configuration, not of the document. So the format validation does not check it. The save checks of 7.18 check the query with the language of the kind. A kind that a later version adds to 8.2 is not valid for an annotation until the specification adds it. The save refuses it with `not_permitted`, and the route of 6.21a refuses it again at run time.

Example (a deploy marker and a maintenance region on the time series 2 and 5):

```json
"annotations": [
  {
    "name": "Deploys",
    "source": "prod",
    "sql": [
      "SELECT deployed_at AS time, concat('v', version) AS text, 'prod,deploy' AS tags",
      "FROM ops.deploys",
      "WHERE $__timeFilter(deployed_at)"
    ],
    "color": "red",
    "panels": [2, 5]
  }
],
```

The fixture `tests/fixtures/dashboards/annotations.json` holds 8 annotations, a name of 256 characters and each form of colour. The files `tests/fixtures/dashboards/invalid/f25_*.json` hold one error each (16.3).

### 4.6 Member `time`

| Member | Type | Rule |
|---|---|---|
| `from` | text | A time text (4.4): the default start of the range |
| `to` | text | A time text (4.4): the default end of the range |
| `timezone` | text | `browser`: the time zone of the browser. `utc`: UTC. Else an IANA zone name (below), such as `Europe/Paris`. |
| `refresh` | text or null | `null`: no automatic refresh. Else one value of `REFRESH_CHOICES` (4.35): `10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`, `2h` or `1d`. Another value gives `bad_format`. |

1. When `from` and `to` are both absolute, `from` MUST be less than `to`.
2. The URL values `from`, `to` and `refresh` replace these defaults in the browser (section 9).
3. `REFRESH_CHOICES` is the list `refresh_choices` of `GET /api/session` (section 6). The importer maps another Grafana value to the next larger choice, with the class `changed` (section 15).
4. The time zone changes the display of times only. The macros use absolute times (section 7).
5. A zone name has this grammar. The format checks the grammar only: it holds no list of zones.

```abnf
zone-name = area "/" place [ "/" place ]
area      = ALPHA *( ALPHA / DIGIT / "_" / "+" / "-" )
place     = ALPHA *( ALPHA / DIGIT / "_" / "+" / "-" )
```

6. A zone name has 3 to 64 bytes (`MAX_TIMEZONE_BYTES`). A text that breaks the grammar gives `bad_format`. The values `browser` and `utc` are not names: the text `UTC` has no `/`, so it is invalid.
7. The browser checks the name with `Intl.DateTimeFormat` (9.9). A name that the browser does not know gives a notice, and the page uses the zone `browser`. The stored value does not change.

| Text | Valid | Reason |
|---|---|---|
| `browser`, `utc` | yes | The two fixed values |
| `Europe/Paris` | yes | |
| `America/Argentina/Buenos_Aires` | yes | Two places |
| `Etc/GMT+5` | yes | `+` is in the grammar |
| `europe/paris` | yes | The grammar has no case rule. The browser decides. |
| `UTC`, `GMT` | no | No `/` |
| `Europe/` and `/Paris` | no | An empty part |
| `Europe/New York` | no | A blank |

### 4.7 Variables: members of all kinds

| Member | Type | Rule |
|---|---|---|
| `kind` | enum | `query` (4.8), `custom` (4.9), `text` (4.10), `interval` (4.10a), `constant` (4.10b) or `adhoc` (4.10c) |
| `name` | text | 4.3 |
| `label` | text or null | 1 to 256 characters. `null`: the bar shows the name. |
| `hide` | enum | `none`: the bar shows the label and the control. `label`: the bar shows the control only. `variable`: the bar shows nothing, and the value still applies. |

The members of the kind follow these four members, in the order of the table of the kind. The optional member `placement` follows them (4.7a). A dashboard references a variable in its SQL as `$name`, `${name}` or `${name:format}` (section 7).

The kinds `query` and `custom` have the optional member `allow_custom_value` (4.8, 4.9). A variable is open when this member is `true`. Else it is closed. A closed variable takes only an option as its value. An open variable also takes a value that a person types. The kind `text` is always free, so it has no such member (4.10).

The kind `interval` is a closed list of interval texts. The kind `constant` is a text of the document, and a person never changes it. A `constant` MUST have `hide` = `variable` (4.10b). Both kinds are author text for the server (7.7).

### 4.7a Member `placement` of a variable

A variable can have a place in the grid. The page then shows the control of the variable in a frame at that place, and the variables bar does not show it (9.10). A variable with no `placement` stays in the variables bar.

| Member | Type | Rule |
|---|---|---|
| `placement` | object, optional | The place of the control. It is the last member of a variable of each kind. Absent: the control is in the variables bar. |

The object `placement` has these members:

| Member | Type | Rule |
|---|---|---|
| `section` | integer | 0 to 2,147,483,647: the index of a section in `sections` (4.12), from 0. It MUST name a section of the document (`bad_reference` at `section`). |
| `x` | integer | 0 to 23: the first column (4.14) |
| `y` | integer | 0 to 10,000: the first row unit, from the top of the section (4.14) |
| `w` | integer | 1 to 24: the width in columns. `x + w` MUST be 24 or less (`out_of_range` at `w`). |
| `h` | integer | 1 to 200: the height in row units |
| `title` | text, optional | 1 to 256 characters: the title of the frame. Absent: the frame shows the label of the variable, as the bar does (4.7). |

1. A variable has one member `placement` at most, so it has one place at most.
2. A variable with `hide` = `variable` MUST NOT have a `placement` (`not_permitted` at `placement`). That value means "show nothing". With `hide` = `label`, the control has no label, and the frame shows `title` only when it is set.
3. A placement MUST NOT overlap a panel of the same section. It MUST NOT overlap the placement of an earlier variable of the same section (`overlap` at `placement` of the later variable). The rectangles are those of 4.14. A placement and a panel of two different sections can use the same place.
4. A variable of the kinds `query`, `custom`, `text` and `interval` can have a placement. A `constant` has `hide` = `variable`, so it has none (rule 2). An `adhoc` variable has none either (4.10c, rule 4). The placement changes no rule of 4.7 to 4.11.
5. A placement is not a panel. It has no id, no query and no frame of data, and it does not count in the 200 panels of the document (4.12).
6. The control adds no way to put a value in a query. The page sends the selection in the request as the bar does, and the server never reads a placement (7.7, 7.11). Section 7 does not change.
7. The placement is part of its variable. A rename of the variable keeps it, and a delete of the variable removes it. Thus no placement can name a variable that does not exist.
8. The editor keeps the index `section` valid. A move of a section changes the index of the placements in it. A delete of a section removes its placements, so their variables return to the variables bar (10.6).
9. A document with no placement has the form that it had before. The canonical form of a variable writes `placement` last, and writes `title` last in it. It writes neither when they are absent (4.38).
10. A person can edit a document by hand and give a section index that does not exist. The validation refuses it at save with `bad_reference`. Thus a stored document never holds such a placement.
11. A placement MUST NOT name a section that has a member `repeat` (4.13a). The validation gives `not_permitted` at `placement/section`. The page would show the control one time for each copy of the section, and a variable has one control only (9.10).

Example of a variable with a placement:

```json
{
  "kind": "text",
  "name": "min_weight",
  "label": "Minimum weight",
  "hide": "none",
  "value_type": "number",
  "default": "0",
  "placement": {
    "section": 1,
    "x": 12,
    "y": 0,
    "w": 8,
    "h": 2
  }
}
```

The fixture `tests/fixtures/dashboards/placements.json` holds three placements and two variables with no placement. The files `tests/fixtures/dashboards/invalid/k_place_*.json` hold one error each (16.3).

### 4.8 Variable kind `query`

| Member | Type | Rule |
|---|---|---|
| `source` | text | A source id (4.3) |
| `sql` | array of text | SQL lines (4.16). Section 7 gives the run of the query and the options that it gives. |
| `multi` | boolean | `true`: the viewer can select more than one value |
| `include_all` | boolean | `true`: the control has the item "All" |
| `all_value` | text or null | 1 to 1024 bytes: the SQL text that "All" gives (section 7). MUST be `null` when `include_all` is `false`. `null` with "All": the list of all options (section 7). |
| `refresh` | enum | `load`: the options load with the dashboard. `time_change`: the options load with the dashboard and again after each change of the time range. |
| `sort` | enum | The order of the options: `none` (the order of the query), `alpha_asc`, `alpha_desc`, `num_asc`, `num_desc`, `alpha_ci_asc`, `alpha_ci_desc`, `natural_asc`, `natural_desc` (`ci`: case-insensitive; `natural`: the numbers in the texts compare as numbers) |
| `regex` | text or null | A pattern of the Grafana form (4.24), or `null` |
| `default` | array of text | 4.11 |
| `allow_custom_value` | boolean, optional | `true`: the variable is open (4.7). `false`, or absent: the variable is closed. The writer leaves out `false` (4.38). |

Section 9 gives how the browser applies `regex` and `sort` to the options, with the rules of Grafana.

For a `query` variable, the member is a rule of the page and nothing more. It gives no promise of security. The page keeps the value of a closed variable among the options (9.10). The server does not compare a value with the options. A value that is not an option is free text for the server, as for a `text` variable. The properties P, PG and PM (7.11, 7.24, 7.35) apply to it as to each other value.

The viewer can already read the options (6.21). The viewer can also choose the time range and the values of the parent variables. Thus a check on the server would add no protection. It would also give a lever for denial of service, because a check costs one source query or more for each request. A person who needs a bound writes it in the SQL of the panel, so that the source enforces it. The example keeps a value among the names of the last 3 days:

```sql
SELECT count() FROM events
WHERE service = '${service}'
  AND service IN (SELECT DISTINCT service FROM events WHERE timestamp >= now() - INTERVAL 3 DAY)
```

### 4.9 Variable kind `custom`

| Member | Type | Rule |
|---|---|---|
| `options` | array of objects | 1 to 10,000 options |
| `multi` | boolean | As 4.8 |
| `include_all` | boolean | As 4.8 |
| `all_value` | text or null | As 4.8 |
| `default` | array of text | 4.11 |
| `allow_custom_value` | boolean, optional | `true`: the variable is open (4.7). `false`, or absent: the variable is closed. The writer leaves out `false` (4.38). |

An option has these members:

| Member | Type | Rule |
|---|---|---|
| `text` | text | 1 to 256 characters: the text that the control shows |
| `value` | text | 1 to 1024 bytes: the value that the SQL gets. No two options have the same value. |

A closed `custom` variable has options that are author text: they come from the document, as the SQL does. The server refuses each value that is not one of these options (7.7). This refusal is a security boundary, because an option can stand in a quoted name, and in MongoDB it can start with `$`.

An open `custom` variable has a different contract. Each value of it is free text for the server, also a value that is an option. The server treats it as a `query` variable at save and at run (7.7, 7.8). The SQL cannot name it in a quoted name. In code, a value of it can be a number only.

A typed value has 1 to 1,024 bytes and no control character. The list "All" that the server builds from the options stays author text.

### 4.10 Variable kind `text`

| Member | Type | Rule |
|---|---|---|
| `value_type` | enum | `text`: free text. `number`: a decimal number. |
| `default` | text | 0 to 1024 bytes. With `number`, it MUST match `^-?[0-9]+(\.[0-9]+)?$`. |

A text value is free text, so it can only become a literal in the SQL (section 7).

A `text` variable has no member `allow_custom_value`: it is always free. A document that has the member on a `text` variable gives `unknown_member`. A `text` value has 0 to 1,024 bytes. A typed value of an open variable has 1 to 1,024 bytes.

### 4.10a Variable kind `interval`

| Member | Type | Rule |
|---|---|---|
| `intervals` | array of text | 1 to `MAX_INTERVALS` items (4.35). Each item is an interval text (below). No two items are equal (`not_unique`). |
| `auto` | boolean | `true`: the control has the option "Auto" before the items. Its value is `$__auto`. |
| `auto_count` | integer | 1 to 1,000: the step count of "Auto" |
| `auto_min` | text | The smallest interval of "Auto". It MUST be one of the 30 texts of the table of 7.4a (`bad_format`). |
| `default` | text or null | An item of `intervals`, or `$__auto` when `auto` is `true`. `null`: the first option of the control. |

An interval text matches `^[1-9][0-9]{0,8}(ms|s|m|h|d|w|y)$`. The units are milliseconds, seconds, minutes, hours, days, weeks and years of 365 days. The interval MUST be 365 days or less (`out_of_range`). The unit `M` of Grafana and a text with a sign, a space or a decimal point are not permitted (`bad_format`). A list with no item gives `out_of_range`. A list with more than `MAX_INTERVALS` items gives `too_large`.

1. The options of the control are "Auto" (only with `auto` = `true`), then the items in the array order. The control has one choice. It has no `multi` and no "All".
2. The value of an item is its text. The value of "Auto" is `$__auto`. The text of "Auto" in the control is `Auto`.
3. The members `auto_count` and `auto_min` have no effect with `auto` = `false`, but they stay in the document. This keeps the form closed and the canonical form unique.
4. A `default` that is not an option gives `not_permitted` at `default`.
5. The server never reads `default` to run a query. The page sends the selection (6.20).
6. The SQL names the variable as `$name`, `${name}` or `${name:format}`. 7.8 gives the text that the SQL gets.
7. A `repeat` cannot name an `interval` variable, because it has no `multi` (4.13a). A `$__conditionalAll` cannot name it (7.10).

Example, with no placement:

```json
{
  "kind": "interval",
  "name": "bucket",
  "label": "Bucket",
  "hide": "none",
  "intervals": ["1m", "10m", "1h", "1d"],
  "auto": true,
  "auto_count": 30,
  "auto_min": "10s",
  "default": "$__auto"
}
```

The fixture `tests/fixtures/dashboards/variable_kinds.json` holds two `interval` variables and two `constant` variables. The files `tests/fixtures/dashboards/invalid/f27_*.json` hold one error each (16.3).

### 4.10b Variable kind `constant`

| Member | Type | Rule |
|---|---|---|
| `value` | text | 0 to 1,024 bytes, with no control character (U+0000 to U+001F and U+007F). An empty text is permitted. |

1. A `constant` MUST have `hide` = `variable` (`not_permitted` at `hide`). The bar never shows it, and a person has no control to change it. It has no `placement` (4.7a, rule 2).
2. The `label` of a `constant` is a note for the editor. The page never shows it in the dashboard.
3. The value does not come from the URL, from a request or from a default. It is always the `value` of the document (7.7, 9.5).
4. The server treats the value as author text (7.7, 7.8). This is safe because the value is a part of the document. A person who can write the document can also write the same text in the SQL. The viewer cannot change it. The account of the source and its `readonly` profile are the boundary of the SQL (7.12), for the value and for the SQL text alike.
5. The value is not read again after the expansion (7.11, step 3). A value with `$x` or `$__from` stays text.
6. A `constant` has no parent and no `default`. A `query` variable can name it, and a panel can name it. It has no `multi`, no "All" and no options.
7. A `repeat` cannot name it (4.13a). A `$__conditionalAll` cannot name it (7.10).
8. The display text of a placeholder of a `constant` in a title is its value (9.13).

### 4.10c Variable kind `adhoc`

An `adhoc` variable lets a viewer add filters to the panels of one ClickHouse source. A filter is a key, an operator and a value. The SQL of a panel takes the filters only through the macro `$__adhocFilters` (7.10a). The kind joins format 3 as a new value of `kind` (4.42, rule 3).

| Member | Type | Rule |
|---|---|---|
| `source` | text | A source id (4.3). The source MUST be a `clickhouse` source of the configuration (7.18, item 17). |
| `keys` | array of text | 1 to `MAX_ADHOC_KEYS` (64) items. Each item matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$` (`bad_format`). No two items are equal (`not_unique`). Each item MUST be in the list `clickhouse "<id>".adhoc_columns` of the source (7.18, item 18). |

1. A document has one `adhoc` variable for each source at most. A second `adhoc` variable with the same `source` gives `not_unique` at its `source`.
2. A key is a plain column name. A dotted name, a map access and a quoted name are not keys: this is a documented limit.
3. The variable has no default, no option, no `multi` and no "All". The document never holds a filter. The filters of a viewer come from the address (9.5) and from the control of the bar (9.10a).
4. The variable has no `placement` (`not_permitted` at `placement`). Its control is always in the variables bar (9.10a).
5. A `repeat` cannot name it, and `$__conditionalAll` cannot name it (4.13a, 7.10).
6. A placeholder of 7.6 that names the variable is a document error (7.18, item 21). Only the macro `$__adhocFilters` reads the filters.
7. The page offers a key only when the configured list of the source holds it (6.14). The server checks each key again at each run (7.7a).
8. The canonical form writes the members `kind`, `name`, `label`, `hide`, `source` and `keys`, in this order (4.38).

Example:

```json
{
  "kind": "adhoc",
  "name": "filters",
  "label": "Filters",
  "hide": "none",
  "source": "prod",
  "keys": ["service", "host", "env"]
}
```

The fixture `tests/fixtures/dashboards/adhoc.json` holds two `adhoc` variables of two sources and one panel with the macro. The test `crates/hune-core/tests/adhoc_filters.rs` holds one refusal for each rule.

### 4.11 Default values of `query` and `custom` variables

1. `default` is an array of 0 to 10,000 items. Each item is a text of 1 to 1024 bytes.
2. With `multi = false`, the array has 0 or 1 item.
3. The item `$__all` means "All". It is permitted only with `include_all = true`, and then it is the only item.
4. For a closed `custom` variable, each other item MUST be the `value` of one option.
5. For an open `custom` variable, each other item is a text of 1 to 1,024 bytes with no control character (`bad_format`). It need not be an option.
6. For `query`, an item can be absent from the options, because the options come from a query at run time. Section 9 gives the selection in that case. A closed variable drops such an item at run time. An open variable keeps it as a typed value (9.10).
7. An empty array means "the first option at run time" (section 9).
8. A URL value `var-<name>` replaces the default (section 9).

The kind `interval` has its own `default` (4.10a): a text or `null`, not an array. The kind `constant` has no `default`.

### 4.12 Sections

A section is a header of rows and its panels. A dashboard without rows has one section with `title: null`. This is the `RowsLayout` of Grafana v2, and it also maps the row panels of Grafana v1.

| Member | Type | Rule |
|---|---|---|
| `title` | text or null | 1 to 256 characters. `null`: the section has no header. |
| `collapsed` | boolean | `true`: the section shows its header only. MUST be `false` when `title` is `null`. |
| `panels` | array of objects | 0 to 200 panels (4.13) |
| `repeat` | object, optional | 4.13a. The page shows one copy of the section for each value of a variable. It is the last member of a section. Absent: the section does not repeat. |

1. Only the first section can have `title: null`.
2. The document has at most 200 panels in all its sections.
3. The member `grid.y` of a panel is relative to the top of its section.
4. The browser does not run the queries of a collapsed section. The exception is a panel whose frame another panel reuses (section 9).
5. A section can repeat (4.13a). A repeat makes copies in the page only. The document holds one section. The count of 50 sections and the count of 200 panels (rule 2) are the counts of the stored document.

### 4.13 Panels: members of all types

| Member | Type | Rule |
|---|---|---|
| `id` | integer | 4.3 |
| `type` | enum | `table`, `timeseries`, `stat`, `barchart`, `piechart`, `xychart`, `bargauge`, `text`, `state_timeline`, `status_history`, `heatmap`, `histogram`, `gauge` (4.33e), `logs` (4.33f), `geomap` (4.33g) |
| `title` | text | 0 to 256 characters |
| `description` | text | 0 to 4096 characters |
| `transparent` | boolean | `true`: the panel has no fill and no edge at rest (9.11). `false`: the panel has the fill and the edge of the theme. The member is necessary, and it comes after `description`. |
| `grid` | object | 4.14 |
| `data` | object | 4.15 |
| `transforms` | array of objects | 0 to 16 transforms (4.25) |
| `fields` | object | 4.17 |
| `options` | object | The options of the type: 4.27 to 4.33g |
| `links` | array of objects | 0 to 16 links (4.23) in the header of the panel |
| `repeat` | object, optional | 4.13a. The page shows one copy of the panel for each value of a variable. It is the last member of a panel. Absent: the panel does not repeat. |

A panel of the type `text` has no query. It has none of the members `data`, `transforms`, `fields` and `repeat`, and the members of its `options` are in 4.33a. The other types have the members of the table above.

The stored `title` and `description` of a panel, and the `title` of a section (4.12), are plain text. They can hold
placeholders of variables, such as `$env`, `${env}` and `${env:text}`. The page replaces them when it shows the text
(9.13). The stored text does not change, and the result is text, never HTML.

### 4.13a Member `repeat` of a panel and of a section

A panel or a section can repeat. The page then shows one copy for each value of one variable of the dashboard. This is the function of Grafana with the same name. The copies are not stored. The document holds one panel or one section, and the page makes the copies (9.8c). A document that has no `repeat` has the form that it had before.

The object `repeat` of a panel has these members:

| Member | Type | Rule |
|---|---|---|
| `variable` | text | The name of a variable of the document (4.3). Rule 1 gives the kinds that can repeat. |
| `direction` | enum | `h`: the copies share the width of the section, in rows. `v`: the copies are one under the other, with the width of the panel. |
| `max_per_row` | integer, optional | 1 to 24. It is valid with `direction` = `h` only. Absent: 4, which is the value of Grafana. |

The object `repeat` of a section has one member:

| Member | Type | Rule |
|---|---|---|
| `variable` | text | As for a panel. |

1. The variable. An unknown name gives `bad_reference` at `repeat/variable`. The variable MUST have the kind `query` or `custom`, and its `multi` MUST be `true` (`not_permitted` at `repeat/variable`). A `text` variable has one value, so it cannot repeat. A variable with `hide` = `variable` can be the variable of a repeat, and so can a variable that has a `placement`. A repeat reads the selection only.
2. The values. The copies are the values of the selection that are options of the variable. A value that is not an option makes no copy. Such a value is a value of the URL, or a typed value of an open variable.
   - With the item "All", each option is a value. The member `all_value` has no effect on a repeat.
   - The order of the copies is the order of the options of the variable (9.10). The order of the selection does not count.
3. The limits. The keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` set them (13.8). The constants `MAX_REPEAT_PER_PANEL`, `MAX_REPEAT_PER_SECTION` and `MAX_REPEAT_PER_DASHBOARD` of 4.35 are the defaults of these keys.
   - A repeat of a panel makes `server.repeat_max_per_panel` copies at most. A repeat of a section makes `server.repeat_max_per_section` copies at most.
   - All the repeats of one dashboard make `server.repeat_max_per_dashboard` copies of panels at most.
   - The count depends on the selection, so the validation cannot check it. The page checks it (9.8c).
   - Above a limit, nothing repeats and the page shows a message. A selection with no value shows a message too.
   - The page never cuts a list of values to make it fit.
4. A panel that repeats. It MUST have the member `data.query`. A panel with `data.reuse` MUST NOT have `repeat` (`not_permitted` at `repeat`). The member `max_per_row` with `direction` = `v` gives `not_permitted` at `repeat/max_per_row`. The canonical form writes `max_per_row` as the document gives it. It never adds the default value.
5. The direction `h`. The copies take the full width of the section. The members `grid.x` and `grid.w` of the panel do not change the view.
   - The panel MUST NOT share a row unit with another panel or with a placement of its section (`overlap` at `repeat/direction`).
   - Each other rectangle of 4.14 starts at the row `y + h` of this panel or below it. Or it ends at the row `y` of this panel or above it.
   - The editor writes `x` = 0 and `w` = 24 when the person chooses `h` (10.3).
6. The direction `v`. The copies have the `x` and `w` of the panel, and they are one under the other. The panels below move down (9.8c). A panel beside the copies does not move.
7. A section that repeats. Its `title` MUST NOT be `null` (`not_permitted` at `repeat`), because copies with no header cannot be told apart.
   - It MUST NOT hold a panel that has `repeat` (`not_permitted` at the `repeat` of that panel).
   - It MUST NOT hold a panel with `data.reuse` (`not_permitted` at `data/reuse` of that panel).
   - A `placement` MUST NOT name it (4.7a, rule 11).
   - Thus a repeat has one level only. A repeat in a repeat would multiply the copies, and the count of queries would have no simple bound.
8. Reuse. The source of a `reuse` (4.15) MUST NOT be a panel that has `repeat`. It MUST NOT be a panel of a section that has `repeat` either (`bad_reference` at `data/reuse/panel`). Such a source would have more than one frame.
9. Rename and delete. The editor changes `repeat.variable` when a person renames the variable. It removes the member `repeat` of each panel and section that names a variable that the person deletes (10.3b). A stored document that names a variable that does not exist is invalid (rule 1).
10. The server. The server never reads `repeat`. A copy sends the ordinary request of 6.20 with one value for the variable. The property P (7.11), the rules PG (7.24) and PM (7.35), and the admission of 8.8 apply to each copy as to each other panel. Section 7 and section 8 do not change.
11. The format. `repeat` is an optional member. It joins format 3 with no new version and no migration. 4.42 gives the effects on an older binary.

Examples of the member:

```json
"repeat": { "variable": "host", "direction": "h", "max_per_row": 3 }
```

```json
"repeat": { "variable": "region", "direction": "v" }
```

```json
"repeat": { "variable": "host" }
```

The first is a panel that shares its width in rows of 3. The second is a panel with copies one under the other. The third is a section.

The fixture `tests/fixtures/dashboards/repeats.json` holds a panel with `h`, a panel with `v` and a section with a repeat. The files `tests/fixtures/dashboards/invalid/f23_*.json` hold one error each (16.3).

### 4.14 Member `grid`

The grid has 24 columns. A row unit has the height of 30 px plus the gap of 8 px, as in Grafana (section 9).

| Member | Type | Rule |
|---|---|---|
| `x` | integer | 0 to 23: the first column |
| `y` | integer | 0 to 10,000: the first row unit, from the top of the section |
| `w` | integer | 1 to 24: the width in columns. `x + w` MUST be 24 or less. |
| `h` | integer | 1 to 200: the height in row units |

Two panels of one section MUST NOT overlap. Two panels overlap when the rectangles `[x, x + w) x [y, y + h)` have a common point. A placement of a variable (4.7a) follows the same rule.

### 4.15 Member `data`

The object `data` has one of two forms. The first form has the member `query` and, optional, the member `extra_queries` (4.15a). The second form has the member `reuse` only.

`query`: the panel runs its own SQL.

| Member | Type | Rule |
|---|---|---|
| `source` | text | A source id (4.3) |
| `format` | enum | `table`: the frame stays as the source gives it. `time_series`: long-to-wide (section 7). `logs`: the frame stays as the source gives it, and the page reads it as log lines (7.16e). Only a panel of the type `logs` permits `logs` (4.33f). |
| `sql` | array of text | SQL lines (4.16) |
| `min_interval` | text or null | A duration from `1ms` to `1d`: the smallest interval of the interval rule (section 7). `null`: 1 ms. |
| `max_data_points` | integer or null | 1 to 10,000: the number of points of the interval rule. `null`: the width of the plot in CSS pixels (section 7). |
| `time_from` | text, optional | A relative time text that starts with `now-` or `now/` (below). The panel range starts at this time and ends at `now`. Absent: the panel uses the dashboard range. |
| `time_shift` | text, optional | A shift (below). The panel moves its range back by this time. Absent: no shift. |
| `hide_time_override` | boolean, optional | `true`: the title bar of the panel has no note about `time_from` and `time_shift` (9.8b). Default `false`. |
| `ref` | text, optional | The name of the query in the panel (4.15a, rule 2). Absent: `A`. |
| `hide` | boolean, optional | `true`: the query does not run (4.15a, rule 3). Default `false`. |

The three optional members are the time override of the panel. The grammar of the two texts is:

```abnf
time-from  = "now" ( "-" count tunit [ "/" tunit ] / "/" tunit )
time-shift = count tunit
```

The members `min_interval`, `max_data_points`, `time_from`, `time_shift` and `hide_time_override` of `data.query` belong to the panel. They apply to each query of the panel (4.15a, rule 4).

`count` and `tunit` are the rules of 4.4. The text `now` alone is not a `time_from`, because it gives an empty range. A text with `+`, an absolute time and a text with a variable are invalid (`bad_format`). The shift `1d/d` is invalid too.

| Text | Valid | Meaning |
|---|---|---|
| `time_from` = `now-6h` | yes | From 6 hours ago to now |
| `time_from` = `now/d` | yes | From the start of the day to now ("today so far") |
| `time_from` = `now-1d/d` | yes | From the start of yesterday to now |
| `time_from` = `6h` | no | The format needs `now-6h`. The importer writes it (15.6). |
| `time_shift` = `1d` | yes | The range moves back by one day |
| `time_shift` = `1w` | yes | The range moves back by one week |
| `time_shift` = `-1d` | no | The member holds no sign |

1. The browser computes the panel range (9.8b), so the server and section 7 do not change. The server gets absolute times only (7.3).
2. An optional member that has its default value is not written (4.38, rule 6). The value `false` of `hide_time_override` is valid in the input, and the canonical form leaves it out.
3. A member `hide_time_override` with no `time_from` and no `time_shift` is valid. It has no effect.

`reuse`: the panel uses the frame of another panel of the same dashboard. The server runs no query for it.

| Member | Type | Rule |
|---|---|---|
| `panel` | integer | The id of another panel of the document whose `data` holds `query` |
| `with_transforms` | boolean | `true`: the transforms of the source panel apply first, then the transforms of this panel. `false`: only the transforms of this panel apply. |

1. A `reuse` MUST NOT name its own panel.
2. A `reuse` MUST name a panel with `query`. A chain of `reuse` is invalid (`bad_reference`). A text panel has no `data`, so a `reuse` that names it gives `bad_reference` at `data/reuse/panel` (4.33a).
3. The source panel can be in another section.
4. A `reuse` has no time member. It uses the frame of its source, so it uses the range of its source, with the time override of its source (9.8b).
5. A `reuse` MUST NOT name a panel that has `repeat`, or a panel of a section that has `repeat` (4.13a, rule 8). A panel with `reuse` MUST NOT have `repeat` and MUST NOT be in a section that has `repeat`.
6. A `reuse` shows all the visible queries of its source (9.23). A panel of the type `barchart`, `xychart` or `logs` with a `reuse` MUST NOT name a source that has `extra_queries` (`bad_reference` at `data/reuse/panel`). Such a panel draws one frame only.

A panel on a `postgres` source uses the same member `query`. The dialect of its SQL is the dialect of the kind of its source (7.19). Thus the format has no kind member, and `format_version` does not change (4.42).

A panel on a `mongodb` source uses the same member `query`, and the same member `sql`. The lines of `sql` hold one JSON text: the query object of 7.29. The query is a pipeline or a find specification, not SQL text, but it fits the array of lines of 4.16. Thus the format has no new member and no new member kind.

- The change needs no new member and no new `format_version`.
- The member `format` has the same two values.
- The collection and the database are in the JSON text, not in the document.
- A variable of the kind `query` on a `mongodb` source also uses `sql` (7.14).

### 4.15a Several queries in one panel

A panel can have more than one query. This is the function of Grafana with the targets A, B and C. The first query is `data.query`. The other queries are the items of the optional array `data.extra_queries`. The page runs each visible query and shows one result (9.8d, 9.22a). A document that has no `extra_queries` has the form that it had before.

An item of `extra_queries` has these members:

| Member | Type | Rule |
|---|---|---|
| `ref` | text | Necessary. The name of the query (rule 2). |
| `source` | text | A source id (4.3). Each query has its own source, so one panel can read from several sources. |
| `format` | enum | `table` or `time_series`, as in 4.15. The value `logs` gives `not_permitted`, because a panel with `extra_queries` is never a logs panel (rule 7, 4.33f). |
| `sql` | array of text | SQL lines (4.16). For a `mongodb` source, one JSON text (4.15). |
| `hide` | boolean, optional | `true`: the query does not run (rule 3). Default `false`. |

An item has no other member. The members `min_interval`, `max_data_points`, `time_from`, `time_shift` and `hide_time_override` stay in `data.query` and apply to all the queries (rule 4). An item with one of them gives `unknown_member`.

1. The array. `extra_queries` has 1 to 7 items (`MAX_QUERIES_PER_PANEL` minus 1). An empty array gives `out_of_range` at `data/extra_queries`, and 8 items or more give `out_of_range` at the same pointer. The editor removes the member when the last extra query leaves. It never writes an empty array.
2. The ref. A `ref` follows the grammar of 4.3. It is unique in the panel. The ref of `data.query` is `A` when its `ref` is absent.
   - A second use of a ref gives `not_unique` at the `ref` of the second query in array order.
   - The order of the refs does not matter, so `data.query` can have the ref `C`.
   - The legend and the matcher `query` (4.17) use the ref as a name.
3. The member `hide`. A hidden query is saved and checked as the other queries. It does not run. The page sends no request for it. The server refuses a request for it (6.20). A panel with all queries hidden shows "No data" (9.8d).
4. The panel members. The interval rule (7.4), `max_data_points`, the time override (4.15, 9.8b) and `hide_time_override` come from `data.query` for each query of the panel. This is the rule of Grafana: it applies the interval, the maximum of points and the time shift to all the targets of a panel. The `format` and the `source` are the members of each query.
5. The cap on one panel. A panel has 8 queries at most: `data.query` and 7 items (`MAX_QUERIES_PER_PANEL`, 4.35). Rule 1 checks it.
6. The cap on a document. A document has 400 queries at most in all its panels (`MAX_QUERIES`, 4.35). A hidden query counts. The check gives `too_large` at `/sections`, as the cap of 200 panels does.
7. The types. A panel of the type `table`, `timeseries`, `stat`, `bargauge`, `piechart`, `state_timeline`, `status_history`, `heatmap` or `histogram` can have `extra_queries`. A panel of the type `barchart`, `xychart`, `logs` or `geomap` MUST NOT have them (`not_permitted` at `data/extra_queries`). Each of these four types draws one frame. A panel of the type `text` has no `data`.
   - A `data` that has `reuse` and `extra_queries` gives `not_permitted` at `data/extra_queries`.
   - The page applies a different rule to each type that can have them (9.22a). A `heatmap` joins the frames on time, as a time series does. A `histogram` collects the fields of the frames one after the other, as a bar gauge does.
8. The repeat. A panel with `repeat` MUST NOT have `extra_queries`, and a panel of a section with `repeat` MUST NOT have them either (`not_permitted` at `data/extra_queries`). A copy would start one request for each query, so the number of requests of a repeat would be the product of two counts. With this rule, a copy is one request, and the bound of 9.8c does not change.
9. The reuse. A `reuse` panel shows the frames of all the visible queries of its source (4.15, rule 6, and 9.23). A query inside a panel cannot be a `reuse` of another panel. The data of a query is always a source and an SQL text.
10. The server. The server runs one query for each request. A request names the panel and, for a panel with `extra_queries`, the ref (6.20). Section 7 and section 8 do not change.
    - Each query has the checks of a panel query of today.
    - These are the property P (7.11), the rules PG (7.24) and PM (7.35), and the statement check of its source kind.
    - They also include the admission of 8.8 and the question of the PDP.
11. The save. The checks of 7.18 apply to each query, and the sources of all queries need `source.query` at save (11.21). A hidden query counts, because a save of SQL is the power to run it.
12. The format. `extra_queries`, `ref` and `hide` are optional members. They join format 3 with no new version and no migration. 4.42 gives the effects on an older binary.

Examples:

```json
"data": {
  "query": {
    "source": "prod",
    "format": "time_series",
    "sql": ["SELECT $__timeInterval(ts) AS t, avg(v) AS cpu FROM m GROUP BY t ORDER BY t"],
    "min_interval": null,
    "max_data_points": null
  },
  "extra_queries": [
    {
      "ref": "B",
      "source": "prod",
      "format": "time_series",
      "sql": ["SELECT $__timeInterval(ts) AS t, avg(v) AS mem FROM n GROUP BY t ORDER BY t"]
    }
  ]
}
```

```json
"data": {
  "query": { "source": "prod", "format": "table", "sql": ["SELECT 1 AS n"], "min_interval": null, "max_data_points": null, "ref": "C", "hide": true },
  "extra_queries": [
    { "ref": "A", "source": "conf", "format": "table", "sql": ["SELECT 2 AS n"], "hide": true }
  ]
}
```

The first panel has two queries on one source. The page joins them on time. The second panel has two hidden queries on two sources, and its first query has the ref `C`. It shows "No data".

The fixture `tests/fixtures/dashboards/queries.json` holds four panels. The first is a time series with 3 queries, and the second is a table with 2 queries. The third is a stat with a hidden query. The fourth has a first query with `ref` and `hide`. The files `tests/fixtures/dashboards/invalid/f24_*.json` hold one error each (16.3).

### 4.16 SQL as an array of lines

An SQL text is an array of lines, so that a git diff of one changed line shows one line.

1. The array has 1 line or more.
2. A line is a string. It MUST NOT contain U+000A (line feed) or U+000D (carriage return). It MAY contain U+0009 (tab). It MUST NOT contain another control character.
3. The SQL text is the lines joined with one U+000A between two lines. No line end follows the last line.
4. The joined text MUST have at most 262,144 bytes (`MAX_SQL_BYTES`). The validation adds the lengths of the lines before it checks a line. A text above the limit gives one error `too_large` at the member `sql`, and its lines get no other check.
5. The format checks rules 1 to 4 and the rule of 4.34 on cycles of variables only. Section 7 gives the check of the statement at save, at import and at run. For a `mongodb` source, the joined text is a JSON text, and 7.28 to 7.36 give its checks.

Example: the lines `["SELECT 1", "FROM t"]` give the SQL text `SELECT 1` U+000A `FROM t` (15 bytes).

### 4.17 Member `fields`

| Member | Type | Rule |
|---|---|---|
| `defaults` | object | A property set (4.18). It applies to each field of the frame. |
| `overrides` | array of objects | 0 to 512 overrides |

An override has these members:

| Member | Type | Rule |
|---|---|---|
| `match` | object | The matcher (table below) |
| `set` | object | A property set (4.18) with 1 member or more |

The matcher has these members:

| Member | Type | Rule |
|---|---|---|
| `by` | enum | `name` (Grafana `byName`), `regex` (Grafana `byRegexp`), `names` (Grafana `byNames`) or `query` (Grafana `byFrameRefID`) |
| `value` | text | Only with `name`, `regex` or `query`. With `name`: 1 to 1024 bytes. A field matches when its name or its base display name (9.13) equals this text exactly. With `regex`: a pattern of the Grafana form (4.24), applied to the base display name (9.13). With `query`: a ref of the grammar of 4.3. A field matches when it comes from the query with this ref (9.14). |
| `names` | array of text | Only with `names`. 1 to 64 (`MAX_MATCHER_NAMES`) names of 1 to 1024 bytes each. An empty array gives `out_of_range`, and more than 64 names give `too_large`. A name matches a field in the same way as `value` with `name`. |
| `mode` | enum | Only with `names`. `include`: the field matches when one of the names matches it. `exclude`: the field matches when no name matches it. |

A matcher with `names` has the members `by`, `names` and `mode` in this order, and it has no `value`. A matcher with
`name`, `regex` or `query` has the members `by` and `value`, and it has no `names` and no `mode`. Another member is an
`unknown_member` error. The matcher with `exclude` is the form that Grafana writes when a viewer clicks a series in the
legend (15.6).

A `value` with `by` = `query` that breaks the grammar gives `bad_format` at `value`. The matcher does not check that the ref exists. A ref that no query has matches no field, as a `name` that no field has.

1. For each field, the browser starts with `defaults`.
2. It then applies each override whose matcher matches the field, in the order of the array.
3. For each property, the last override that sets it gives the value, as in Grafana.
4. A property that no set gives takes the built-in behaviour of section 9.
5. The display name of a field follows the rule of Grafana, and section 9 states it. The rule uses the property `display_name`, the field name and the label rule after long-to-wide. After `grouping_to_matrix`, the first column has the name `<row>\<column>`.

### 4.18 Property set

A property set is the only object of the format whose members are all optional. An absent member means "not set". A member MUST NOT be `null`. The table gives the canonical order and the Grafana property id that the importer maps. It also gives the number of uses in the 332 measured overrides.

| Member | Type | Rule | Grafana property id | Measured uses | Panel types |
|---|---|---|---|---|---|
| `unit` | text | A unit (4.20) | `unit` | 92 | all |
| `decimals` | integer | 0 to 20 | `decimals` | 113 | all |
| `min` | number | The low end of the scale (thresholds, colours, gauges) | `min` | 56 | all |
| `max` | number | The high end of the scale | `max` | 26 | all |
| `color` | object | 4.19 | `color` | 39 | all |
| `thresholds` | object | 4.21 | `thresholds` | 70 | all |
| `mappings` | array of objects | 1 to 256 mappings (4.22) | `mappings` | 36 | all |
| `links` | array of objects | 1 to 16 data links (4.23) | `links` | 20 | all |
| `display_name` | text | 1 to 256 characters: the name that the panel shows for the field | `displayName` | not measured | all |
| `no_value` | text | 1 to 256 characters: the text that the panel shows when the field has no value (section 9). Only in `fields.defaults`. | `noValue` | 12 panels | all |
| `width` | integer | 10 to 4000: the column width in CSS pixels | `custom.width` | 239 | `table` |
| `align` | enum | `auto`, `left`, `center`, `right` | `custom.align` | 132 | `table` |
| `cell` | object | The cell type (table below) | `custom.cellOptions` (`custom.displayMode` in older files) | 161 | `table` |
| `filterable` | boolean | `true`: the column header has a value filter (section 9) | `custom.filterable` | not measured | `table` |
| `hidden` | boolean | `true`: the table does not show the column | `custom.hidden` | not measured | `table` |
| `footer` | object | The calculations that the footer of the table shows for the column (below) | `custom.footer` | not measured | `table` |
| `inspect` | boolean | `true`: each cell of the column has the button "Inspect value" (section 9) | `custom.inspect` | 1 `true` in the 12 tables of the plugin dashboards | `table` |
| `hide_from` | object | Where the series does not show (below) | `custom.hideFrom` | 1 in the sample | all |
| `axis_placement` | enum | The side of the y axis of the series: `auto`, `left`, `right` or `hidden` (below) | `custom.axisPlacement` | not measured | `timeseries` |
| `axis_label` | text | 1 to 256 characters: the title of the y axis of the series | `custom.axisLabel` | not measured | `timeseries`, `barchart` |
| `soft_min` | number | The soft low end of the y axis. The axis goes lower only when the data does. | `custom.axisSoftMin` | not measured | `timeseries`, `barchart` |
| `soft_max` | number | The soft high end of the y axis. The axis goes higher only when the data does. | `custom.axisSoftMax` | not measured | `timeseries`, `barchart` |
| `axis_grid` | boolean | `false`: the value axis has no grid lines. `true`: the value axis has its grid lines (below) | `custom.axisGridShow` | 2 `false` in the 2 bar charts of the plugin dashboards | `barchart` |
| `draw` | enum | `line`, `bars` or `points`: the style of the series. Only in an override. | `custom.drawStyle` | not measured | `timeseries` |
| `line_width` | integer | 0 to 10 (CSS pixels). Only in an override. | `custom.lineWidth` | not measured | `timeseries` |
| `fill_opacity` | integer | 0 to 100 (percent). Only in an override. | `custom.fillOpacity` | not measured | `timeseries` |
| `line_dash` | enum | `solid`, `dash` or `dot`: the dash of the line | `custom.lineStyle` | not measured | `timeseries` |
| `show_points` | enum | `auto`, `always` or `never`. Only in an override. | `custom.showPoints` | not measured | `timeseries` |
| `point_size` | integer | 1 to 40 (CSS pixels). Only in an override. | `custom.pointSize` | not measured | `timeseries` |
| `transform` | enum | `none`, or `negative_y`: the page draws the values with the opposite sign | `custom.transform` | not measured | `timeseries` |
| `line_dash_array` | array of integers | 1 to 8 integers from 0 to 100, one of them above 0: the dash of the line in device pixels (below) | `custom.lineStyle.dash` | not measured | `timeseries` |
| `axis_centered_zero` | boolean | `true`: the y axis has 0 in its middle (9.19) | `custom.axisCenteredZero` | not measured | `timeseries`, `barchart` |
| `axis_width` | integer | 0 to 500: the width of the y axis in CSS pixels (9.19) | `custom.axisWidth` | 1 (3) in the 55 time series of the plugin dashboards | `timeseries`, `barchart` |
| `axis_border` | boolean | `true`: the y axis has a line and ticks (9.19) | `custom.axisBorderShow` | not measured | `timeseries`, `barchart` |
| `axis_color` | enum | `text`: the colour of the labels. `series`: the colour of the first series of the scale (9.19) | `custom.axisColorMode` | not measured | `timeseries`, `barchart` |

1. The members `width`, `align`, `cell`, `filterable`, `hidden`, `footer` and `inspect` are permitted only in a panel of type `table` (`not_permitted`).
2. When one set has both `min` and `max`, `min` MUST be less than `max`.
3. The member `no_value` is permitted only in `fields.defaults`. In an override `set`, it gives `not_permitted`.
4. The members `footer` and `hide_from` are optional, as every member of a property set. The member `hide_from` is permitted in every panel type. It has an effect in the types that draw series (9.14).
5. A member of the column "Panel types" in a panel of another type gives `not_permitted`. The members `axis_placement`, `line_dash` and `transform` are permitted in `fields.defaults` and in an override `set`. The members `line_dash_array`, `axis_centered_zero`, `axis_width`, `axis_border` and `axis_color` are permitted in both places too.
6. The members `draw`, `line_width`, `fill_opacity`, `show_points` and `point_size` are permitted in an override `set` only. In `fields.defaults` they give `not_permitted`, because the panel options of 4.28 hold the default values of these five.
7. When one set has both `soft_min` and `soft_max`, `soft_min` MUST be less than `soft_max` (`out_of_range`). The members `soft_min` and `soft_max` do not depend on `min` and `max`: 9.19 gives the rule of the axis.

The object `cell` has these members:

| Member | Type | Rule |
|---|---|---|
| `type` | enum | `auto`: the text of the value. `color_text`: the text takes the colour of the value (4.19). `color_background`: the cell background takes the colour of the value. `gauge`: the cell draws a bar from `min` to `max`, with the colours of the thresholds. `json`: the cell shows the value as compact JSON text. |
| `mode` | enum or null | With `color_background`: `gradient` or `basic`. With `gauge`: `basic`, `gradient` or `lcd`. With `auto`, `color_text` and `json`: `null`. |

The 161 measured cell types are 64 `auto`, 9 `color-background` and 88 not classified. M0 classifies the 88 (section 19). The types `gauge` and `json` are in format 3. The importer gives the class "unsupported" to another Grafana cell type (section 15).

The object `footer` has one member:

| Member | Type | Rule |
|---|---|---|
| `reducers` | array of enum | 1 to 8 (`MAX_FOOTER_REDUCERS`) items, no two equal. Each item is a value of `reduce.calc` (4.26). Each item gives one footer row (9.20). |

The member `inspect` is an optional member of format 3. `true` gives each cell of the column a button that shows the whole value in a drawer (9.20). `false` and an absent member give no button. A document with no `inspect` member does not change.

The object `hide_from` has the members `legend`, `tooltip` and `viz`. Each one is a boolean, and each one is necessary.

- `legend`: `true` removes the series from the legend table and from the legend list.
- `tooltip`: `true` removes the series from the tooltip.
- `viz`: `true` means the chart does not draw the series.

Section 9 gives the exact behaviour (9.14, 9.19).

These members are in a property set (format 3):

- `axis_placement`: `auto` lets the page choose the side by the unit of the series (9.19). `left` and `right` put the series on that side. `hidden` draws no axis for the scale of the series, and the series keeps its scale. A `barchart` has one value axis, so it has no `axis_placement`.
- `axis_label`, `soft_min` and `soft_max` apply to the y axis of the series. In a `barchart`, they apply to the value axis.
- `axis_grid` is an optional member of format 3. It has the three states of the Grafana property `axisGridShow`: absent (Auto), `true` (On) and `false` (Off). A `barchart` has one value axis, so Auto and On draw the same grid lines. Off removes the grid lines of the value axis. The labels of the axis stay (9.19). A document with no `axis_grid` member does not change.
- `draw`, `line_width`, `fill_opacity`, `show_points` and `point_size` give the style of one series. The panel options of 4.28 give the style of the other series.
- `line_dash`: `solid` is a plain line. `dash` has dashes of 10 px and gaps of 10 px. `dot` has dots that are 10 px apart.
- `transform`: `negative_y` draws the value with the opposite sign. The values of the data do not change.
- Section 9 gives the exact behaviour (9.14, 9.19).

The members below are optional members of format 3. A document with none of these members does not change.

- `line_dash_array` is the dash of the line, as `lineStyle.dash` of Grafana. It gives the length of a dash, then of a gap, and so on, in device pixels. An empty array and an array of zeros give `out_of_range`. More than 8 items give `too_large`.
- The array applies when `line_dash` is `dash` or `dot`, or when the set has no `line_dash`: then the line has dashes. With `solid`, the array has no effect.
- A set that has `line_dash` or `line_dash_array` replaces both members of an earlier set (9.14). This is the rule of Grafana, where an override replaces the whole `lineStyle`.
- `axis_centered_zero`, `axis_width`, `axis_border` and `axis_color` give the look of the y axis of the series. In a `barchart`, they apply to the value axis. Section 9 gives the exact behaviour (9.19).

### 4.19 Colours

A colour is one of these values:

1. One of the 32 tokens. Each hue `red`, `orange`, `yellow`, `green`, `blue` and `purple` gives five tokens: `super-light-<hue>`, `light-<hue>`, `<hue>`, `semi-dark-<hue>` and `dark-<hue>`. The two other tokens are `text` and `transparent`.
2. A hex colour: `#rgb`, `#rrggbb` or `#rrggbbaa`. The hex digits can be lower case or upper case.
3. A colour `rgb(R, G, B)` or `rgba(R, G, B, A)`. R, G and B are integers from 0 to 255, without a `0` before another digit. A is `0`, `1`, or `0.` followed by 1 to 3 digits. One space follows each comma.

Section 9 gives the value of each token. The light theme can draw a token with another shade (9.16), but the stored value stays the token. A hex colour or an `rgb` colour has the same value in both themes. The mode `palette` stores no colour. The page chooses the colour (9.16).

The object `color` of a property set has these members:

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `fixed`: one colour. `thresholds`: the colour of the threshold step of the value (4.21). `palette`: the one series palette of ClickHouse Dash, by series order (9.16). The page has no other series palette. The 10 `continuous_` modes (below). The value `palette_by_name` is not a mode: it gives `bad_format`. The message names the mode `palette`. |
| `fixed` | text or null | A colour with `fixed`. `null` with the other modes. |

The 10 continuous modes give a colour from a gradient, by the place of the value between `min` and `max` (9.16). The values are:

| Value | Stops of the gradient (tokens of this section, and `panel`) | Grafana mode |
|---|---|---|
| `continuous_gr_yl_rd` | `green`, `yellow`, `red` | `continuous-GrYlRd` |
| `continuous_rd_yl_gr` | `red`, `yellow`, `green` | `continuous-RdYlGr` |
| `continuous_bl_yl_rd` | `dark-blue`, `super-light-yellow`, `dark-red` | `continuous-BlYlRd` |
| `continuous_yl_rd` | `super-light-yellow`, `dark-red` | `continuous-YlRd` |
| `continuous_bl_pu` | `blue`, `purple` | `continuous-BlPu` |
| `continuous_yl_bl` | `super-light-yellow`, `dark-blue` | `continuous-YlBl` |
| `continuous_blues` | `panel`, `dark-blue` | `continuous-blues` |
| `continuous_reds` | `panel`, `dark-red` | `continuous-reds` |
| `continuous_greens` | `panel`, `dark-green` | `continuous-greens` |
| `continuous_purples` | `panel`, `dark-purple` | `continuous-purples` |

Each mode is valid in every panel type, as `thresholds` is. `color.fixed` is `null` with each of these modes.

The stop `panel` is the colour of the panel background: `#181B1F` in the dark theme and `#FFFFFF` in the light theme. It is not a colour value of the document. The stops are measured on Grafana 12.4.2. Section 9 (9.16) gives the interpolation.

### 4.20 Units

| Unit | Display |
|---|---|
| `none` | The number with no unit |
| `short` | The number scaled by 1000 with the suffixes of Grafana `short` (section 9) |
| `percent` | A value from 0 to 100, followed by `%` |
| `percentunit` | A value from 0 to 1, multiplied by 100, followed by `%` |
| `ms` | Milliseconds, scaled to larger time units (section 9) |
| `s` | Seconds, scaled to larger time units (section 9) |
| `bytes` | Bytes, scaled by 1024 (IEC units) |
| `decbytes` | Bytes, scaled by 1000 (SI units) |
| `suffix:<text>` | The number, then `<text>` exactly. `<text>` has 1 to 32 characters and can start with a space (`suffix: pts`). |
| `prefix:<text>` | `<text>` exactly, then the number. `<text>` has 1 to 32 characters. |
| `ns` | Nanoseconds, scaled to larger time units (9.15) |
| `µs` | Microseconds, scaled to larger time units (9.15). The id has the character U+00B5 (micro sign) and `s`. |
| `m` | Minutes, scaled to larger time units (9.15) |
| `h` | Hours, scaled to larger time units (9.15) |
| `d` | Days, scaled to weeks and years (9.15) |
| `dtdurationms` | A duration in milliseconds, written in words such as `1 hour, 2 minutes` (9.15) |
| `dtdurations` | A duration in seconds, written in words (9.15) |
| `bits` | Bits, scaled by 1024 (IEC units) |
| `bps` | Bits per second, scaled by 1000 (SI units) |
| `Bps` | Bytes per second, scaled by 1000 (SI units) |
| `binBps` | Bytes per second, scaled by 1024 (IEC units) |
| `reqps` | Requests per second, scaled by 1000 with the suffixes `K`, `M`, `B`, `T` |
| `ops` | Operations per second, scaled as `reqps` |
| `rps` | Reads per second, scaled as `reqps` |
| `iops` | I/O operations per second, scaled as `reqps` |
| `locale` | The number in the number format of the language of the browser |
| `dateTimeAsIso` | An epoch time in milliseconds, as `YYYY-MM-DD HH:mm:ss` in the zone of the dashboard |
| `dateTimeFromNow` | An epoch time in milliseconds, as the time from now, such as `2 hours ago` |
| `bool_yes_no` | `Yes` for a true value, `No` for a false value |
| `currencyUSD` | The number with the prefix `$`, scaled by 1000 |
| `currencyEUR` | The number with the prefix `€`, scaled by 1000 |
| `string` | The number as a text, with no rounding and no scale (9.15) |
| `bool` | `False` for 0, `True` for another number (9.15) |
| `bool_on_off` | `Off` for 0, `On` for another number (9.15) |
| `kbytes` | Kibibytes, scaled by 1024 (IEC units) |
| `mbytes` | Mebibytes, scaled by 1024 (IEC units) |
| `gbytes` | Gibibytes, scaled by 1024 (IEC units) |
| `tbytes` | Tebibytes, scaled by 1024 (IEC units) |
| `deckbytes` | Kilobytes, scaled by 1000 (SI units) |
| `decmbytes` | Megabytes, scaled by 1000 (SI units) |
| `decgbytes` | Gigabytes, scaled by 1000 (SI units) |
| `dectbytes` | Terabytes, scaled by 1000 (SI units) |
| `decbits` | Bits, scaled by 1000 (SI units) |
| `KBs` | Kilobytes per second, scaled by 1000 (SI units) |
| `MBs` | Megabytes per second, scaled by 1000 (SI units) |
| `GBs` | Gigabytes per second, scaled by 1000 (SI units) |
| `Kbits` | Kilobits per second, scaled by 1000 (SI units) |
| `Mbits` | Megabits per second, scaled by 1000 (SI units) |
| `Gbits` | Gigabits per second, scaled by 1000 (SI units) |
| `rowsps` | Rows per second, scaled as `reqps` |
| `rowspm` | Rows per minute, scaled as `reqps` |
| `recps` | Records per second, scaled as `reqps` |
| `cps` | Counts per second, scaled as `reqps` |
| `wps` | Writes per second, scaled as `reqps` |
| `opm` | Operations per minute, scaled as `reqps` |
| `reqpm` | Requests per minute, scaled as `reqps` |
| `rpm` | Reads per minute, scaled as `reqps` |
| `dthms` | A duration in seconds, as `hh:mm:ss` (9.15) |
| `dtdhms` | A duration in seconds, as `hh:mm:ss` with the days before it, such as `1143 d 02:50:32` (9.15) |
| `hertz` | Hertz, scaled by 1000 (SI units) |
| `sishort` | The number with the SI prefix of its scale and no unit, such as `1.23 k` (9.15) |

Another unit is invalid (`bad_format`). M0 measures the units of the real panels. Before release 0.1.0, M0 can add Grafana unit ids to this table (section 19). The 21 units after `suffix:` and `prefix:` are in format 3. Each one has the id of the Grafana unit of the same kind, so the importer maps it as it is (15.6).

The last 30 units, from `string` to `sishort`, are the last group of the list. The first three show a text or a state. The others show the sizes, the rates, the counts and the durations of the system tables of ClickHouse. They are new values of `unit`, so they join with no change of `format_version` (4.42, rule 3). Each one has the id of the Grafana unit, so the importer maps it as it is (15.6). The other Grafana ids and the forms `si:`, `count:`, `currency:`, `time:` and `bool:` stay invalid.

### 4.21 Thresholds

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `absolute`: a step value is a value of the field. `percentage`: a step value is a percent of the range from `min` to `max` (section 9). |
| `steps` | array of objects | 1 to 32 steps |

A step has these members:

| Member | Type | Rule |
|---|---|---|
| `value` | number or null | The first step MUST have `null` (the base). Each other step MUST have a number greater than the number of the step before it. |
| `color` | text | A colour (4.19) |

A value takes the colour of the last step whose value is at most the value. A value below the second step takes the colour of the base.

### 4.22 Mappings

A mapping changes the text or the colour of a value. The browser applies the first mapping of the array that matches the value (section 9). Each mapping is a tagged object.

| Kind | Members after `kind` | Match rule |
|---|---|---|
| `value` | `value`, `text`, `color` | The display text of the raw value equals `value` (0 to 1024 bytes) |
| `range` | `from`, `to`, `text`, `color` | The number is at least `from` and at most `to`. `from` and `to` are numbers or `null` (no bound). |
| `regex` | `pattern`, `text`, `color` | The pattern (4.24) matches the display text. `text` can hold `$1` to `$9` (the capture groups). |
| `special` | `match`, `text`, `color` | `match`: `null`, `nan`, `null_nan`, `true`, `false` or `empty` (an empty text) |

1. `text` is a text of 1 to 256 characters, or `null` (the text does not change).
2. `color` is a colour (4.19), or `null` (the colour does not change).
3. `text` and `color` MUST NOT be both `null`.
4. In a `range`, `from` and `to` MUST NOT be both `null`. When both are numbers, `from` MUST be at most `to`.

### 4.23 Links

A link is a panel link (member `links` of the panel) or a data link (property `links` of a field).

| Member | Type | Rule |
|---|---|---|
| `title` | text | 1 to 256 characters |
| `url` | text | 1 to 2048 bytes: a URL template with tokens `${...}` |
| `target_blank` | boolean | `true`: the link opens a new tab with `rel="noopener noreferrer"` |

1. The prefix of `url` is its text before the first `${`, or all its text without a token.
2. The prefix MUST be empty, or it MUST start with `/`, `?`, `#`, `http://` or `https://`. It MUST NOT start with `//` or with `/\`, because a browser reads `/\` as `//`.
3. Section 9 gives the tokens, their values and the URL encoding of each value.
4. The browser checks the final URL again after it replaces the tokens (section 12).

### 4.24 Regular expressions

Five members hold a pattern. A pattern has one of two forms:

| Form | Members | Text | Meaning |
|---|---|---|---|
| Plain | matcher `pattern` (4.25) | The pattern | A match can start at any position. The anchors `^` and `$` limit it. |
| Grafana | variable `regex` (4.8), override `match.value` with `by = "regex"` (4.17), mapping `pattern` (4.22), reduce `fields` (4.26) | `/<body>/<flags>`, or a plain text | The form of the Grafana function `stringToJsRegex`. `/<body>/<flags>` adds no anchor: a match can start at any position, unless the body has `^` and `$`. A plain text `t` is an exact match: the pattern `^t$`. |

The Grafana `byRegexp` matcher and the Grafana stat field option use `stringToJsRegex` (the default option of `byRegexp` is `/.*/`). Thus `/^Total$/` and the plain text `Total` both match the field `Total` only.

These rules apply to each pattern:

1. A pattern has 1 to 1024 bytes.
2. In the Grafana form, a text that starts with `/` has a body up to its last `/`. The flags after it are 0 to 4 letters from `g`, `i`, `m` and `s`, each one at most one time.
3. The body, or the plain pattern, MUST be in the common subset below. `hune-core` checks it with the validation of 4.36 (`bad_format`).
4. The browser uses the same subset. It compiles the text with `new RegExp(body, flags)` (9.10, 9.14, 9.17, 9.21), and it never adds the flag `u`.
5. Section 9 gives how the browser applies each form.

The common subset:

| Rule | Reason |
|---|---|
| The text compiles with the default syntax of the Rust `regex` crate. | A pattern that the server accepts is valid. |
| An escape is one of `\d`, `\D`, `\w`, `\W`, `\b`, `\B`, `\t`, `\n`, `\r`, `\f`, `\v`, `\x` with two hex digits, or `\` before one ASCII punctuation character. | The other escapes differ between the engines: `\A`, `\z`, `\p`, `\P`, `\s` and `\S` (JavaScript `\s` also matches Unicode white space), `\u`, octal escapes. Write `[ \t\n\r\f\v]` for white space. |
| No POSIX class `[[:name:]]`. In a class, no `&&`, `--`, `~~` and no `[` without `\`. | Rust reads these as class operations; JavaScript reads them as text. |
| No group `(?` other than `(?:` and `(?<name>`. A name has ASCII letters, digits and `_`, and does not start with a digit. | No inline flag, no lookaround, no back reference. |
| A repetition in braces is `{m}`, `{m,}` or `{m,n}`. | JavaScript reads other braces as text. |
| The classes `\d`, `\w` and `\b` are ASCII classes. | JavaScript without the flag `u` reads them as ASCII. |

The browser runs each pattern. The server only checks it. Section 16 checks that each pattern of the fixtures compiles in both engines.

### 4.25 Transforms

The browser applies the transforms of a panel in the order of the array (section 9). A transform has 13 kinds. The kinds `filter_by_value` and `grouping_to_matrix` come first below. The 11 kinds of 4.25a follow them.

Kind `filter_by_value` (Grafana `filterByValue`, 36 uses):

| Member | Type | Rule |
|---|---|---|
| `kind` | enum | `filter_by_value` |
| `mode` | enum | `include`: keep the rows that pass. `exclude`: remove the rows that pass. |
| `match` | enum | `any`: a row passes when one filter passes. `all`: a row passes when all filters pass. |
| `filters` | array of objects | 1 to 32 filters |

A filter has these members:

| Member | Type | Rule |
|---|---|---|
| `field` | text | 1 to 1024 bytes: the display name of the field |
| `matcher` | object | A tagged object (table below) |

| Matcher `kind` | Members after `kind` | Passes when the value of the field |
|---|---|---|
| `equal` | `value`: number or text | equals `value` |
| `not_equal` | `value`: number or text | does not equal `value` |
| `greater` | `value`: number | is greater than `value` |
| `greater_or_equal` | `value`: number | is at least `value` |
| `lower` | `value`: number | is less than `value` |
| `lower_or_equal` | `value`: number | is at most `value` |
| `range` (Grafana `between`) | `from`: number, `to`: number | is between `from` and `to`, with the bound rule of Grafana (section 9). `from` MUST be at most `to`. |
| `regex` | `pattern`: text | matches the pattern (4.24) |
| `is_null` | none | is null |
| `is_not_null` | none | is not null |

Section 9 gives the comparison rules of Grafana for each matcher.

Kind `grouping_to_matrix` (Grafana `groupingToMatrix`, 18 uses):

| Member | Type | Rule |
|---|---|---|
| `kind` | enum | `grouping_to_matrix` |
| `column_field` | text | 1 to 1024 bytes: the field whose values become the columns |
| `row_field` | text | 1 to 1024 bytes: the field whose values become the rows |
| `value_field` | text | 1 to 1024 bytes: the field whose values fill the cells |
| `empty_value` | enum | The value of a cell without data: `empty` (an empty text), `null`, `true` or `false` |

The first column of the result has the name `<row_field>\<column_field>`. The columns come in the order of first appearance (section 9).

No kind exists for the Grafana transforms `prepareTimeSeries` (1 use) and `labelsToFields` (1 use). The importer maps each one to a built-in behaviour and gives it the class "changed" (section 15):

1. `prepareTimeSeries` in the mode "multi-frame" becomes the long-to-wide of `format = "time_series"`.
2. `labelsToFields` after long-to-wide becomes `format = "table"`.

### 4.25a The eleven more transforms

These kinds join format 3 with no new version (4.42). Each kind has the member `kind` first, then the members of its table, in the order of the table. Each member is necessary. A member of another kind gives `unknown_member`.

A field name in these tables is a current name (9.21a, rule 1). It has 1 to 1,024 bytes. The kinds `merge` and `join_by_field` use `name` instead (9.21a, rules 6 and 7).

An array of names has 0 to `MAX_TRANSFORM_ITEMS` items (4.35). No two items are equal (`not_unique` at the second item). An array with more items than its bound gives `too_large`. An array with fewer items than its minimum gives `out_of_range`.

Kind `organize` (Grafana `organize`):

| Member | Type | Rule |
|---|---|---|
| `hide` | array of text | The fields that the transform removes |
| `order` | array of text | The fields that come first, in this order. The other fields follow in their order. |
| `rename` | array of objects | 0 to `MAX_TRANSFORM_ITEMS` items. Each item has `field` (text) and `name` (text, 1 to 256 characters). No two items have the same `field`. |

Kind `calculate_field` (Grafana `calculateField`):

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `binary`, `reduce_row`, `unary` or `index` |
| `alias` | text or null | 1 to 256 characters: the name of the new field. `null`: the default name (9.21a, rule 3). |
| `replace_fields` | boolean | `true`: the result holds the first time field and the new field only. `false`: the new field follows the other fields. |
| one object | object | The object of the mode: `binary`, `reduce`, `unary` or `index`. The object of another mode gives `unknown_member`. |

The object `binary` has `left` and `right`, which are operands, and `operator` (`add`, `subtract`, `multiply` or `divide`). An operand is an object with `kind` first. With `kind` = `field`, it has `field` (text). With `kind` = `number`, it has `value`, a JSON number. The two operands MUST NOT both be numbers.

The object `reduce` has `reducer` (one value of `reduce.calc` in 4.26) and `include`. The array `include` holds names. An empty array means all fields of type number. The object `unary` has `operator` (`abs`, `exp`, `ln`, `round`, `floor`, `ceil` or `percent`) and `field` (text). The object `index` has `as_percentile` (boolean).

Kind `sort_by` (Grafana `sortBy`):

| Member | Type | Rule |
|---|---|---|
| `sort` | array of objects | 1 to 8 sort keys, as in 4.27: `field` (text) and `desc` (boolean). The first key sorts first. No two keys are equal. |

Kind `limit` (Grafana `limit`):

| Member | Type | Rule |
|---|---|---|
| `count` | integer | 0 to 1,000,000: the number of rows to keep |
| `end` | enum | `first`: keep the first rows. `last`: keep the last rows. |

Kind `merge` (Grafana `merge`) has no member after `kind`.

Kind `reduce` (Grafana `reduce`):

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `series_to_rows`: one row for each field. `reduce_fields`: one field for each field and reducer, with one row. |
| `reducers` | array of enum | 1 to 18 values of `reduce.calc` (4.26), no two equal |

Kind `partition_by_values` (Grafana `partitionByValues`):

| Member | Type | Rule |
|---|---|---|
| `fields` | array of text | 1 to 4 field names, no two equal: the key fields |
| `keep_fields` | boolean | `true`: the key fields stay in each new frame |

Kind `join_by_field` (Grafana `joinByField`, also called `seriesToColumns`):

| Member | Type | Rule |
|---|---|---|
| `field` | text or null | 1 to 1,024 bytes: the `name` of the key field. `null`: the first field of the kind `time` of each frame. |
| `mode` | enum | `outer`: all keys. `inner`: the keys that all frames have. |

Kind `filter_fields_by_name` (Grafana `filterFieldsByName`):

| Member | Type | Rule |
|---|---|---|
| `include` | object | `names` (array of names) and `pattern` (a pattern of the Grafana form, 4.24, or null). Both empty: all fields pass. |
| `exclude` | object | The same two members. Both empty: no field is removed. |

Kind `rename_by_regex` (Grafana `renameByRegex`):

| Member | Type | Rule |
|---|---|---|
| `regex` | text | 1 to 1,024 bytes: a pattern of the Grafana form (4.24) |
| `rename_pattern` | text | 0 to 256 characters: the replacement text of the JavaScript `replace`. `$1` to `$99` are groups. |

Kind `convert_field_type` (Grafana `convertFieldType`):

| Member | Type | Rule |
|---|---|---|
| `conversions` | array of objects | 1 to 16 items. Each item has `field` (text) and `to` (`number`, `text`, `time` or `boolean`). No two items have the same `field`. |

The members that Grafana has and that these kinds leave out are in 15.6. Section 9 gives the rules of each kind (9.21a). The fixtures `tests/fixtures/dashboards/transforms_more.json` (one transform of each kind) and `tests/fixtures/dashboards/invalid/f28_*.json` (one error each) are in 16.3.

### 4.26 Panel options: shared objects

The member `options` of a panel is a closed object for each type (4.27 to 4.33e). All its members MUST be in it. The tables give the Grafana source of each member for the importer. Section 15 gives the value map of each enum whose Grafana values differ. M0 measures the options of the real panels (section 19). Before release 0.1.0, M0 can remove an option that no real panel uses.

Some members are optional. They are `legend.sort_by`, `legend.sort_desc`, `tooltip.sort`, `tooltip.hide_zeros`, the option `thresholds_style` of `timeseries` and `barchart`, the option `justify` of `stat` and the option `wrap` of `table`. The options `fill_opacity`, `line_width` and `gradient_mode` of `barchart` and `gradient_mode`, `log_base`, `span_nulls_ms` and `bar_alignment` of `timeseries` are optional too. An absent optional member means its default value. The writer leaves out an optional member that has its default value (4.38, rule 6). An input that holds such a member with its default value is valid, and its canonical form does not hold the member.

The nine optional members of a `gauge` are in 4.33e. Its member `reduce` is necessary.

The two types of 4.33b use the objects `legend` and `tooltip` with fewer members. The rules are in 4.33b. They have no `reduce` object, because they draw each sample and not a reduced value.

A `heatmap` uses the object `tooltip` with fewer members (4.33c). It has no `legend` object, because its legend is a colour bar and the member `show_legend` turns it on and off. A `histogram` uses the objects `legend` and `tooltip` with all their members, as a `barchart` does.

Object `legend`:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `show` | boolean | `true`: the panel shows a legend | `options.legend.showLegend`; `displayMode = "hidden"` gives `false` |
| `placement` | enum | `bottom` or `right` | `options.legend.placement` |
| `mode` | enum | `list`: one line of series names. `table`: one row for each series, with the values of `values`. | `options.legend.displayMode` (`list`, `table`) |
| `values` | array of enum | 0 to 5 items, no two equal. Each item is a value of `reduce.calc` in the table below (the 18 values of `calc`, with `total` for the Grafana `sum`, 9.19). In a `piechart`: 0 to 2 items from `value`, `percent`. | `options.legend.calcs`; in a `piechart`, `options.legend.values` |
| `sort_by` | enum, optional | The column that sorts the legend table. Default: absent, which keeps the series order. Outside a `piechart`: `name` or one of the 18 values of `reduce.calc` in the table below (19 values in all). In a `piechart`: `name`, `value` or `percent`. The column does not need to be in `values`. | `options.legend.sortBy`, a column title (15.6) |
| `sort_desc` | boolean, optional | `true`: the sort goes from the largest to the smallest value, or from `z` to `a` for `name`. Default `false`. | `options.legend.sortDesc` |

Object `tooltip`:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `mode` | enum | `single`: the series under the cursor. `multi`: all series. `hidden`: no tooltip. | `options.tooltip.mode`; `none` gives `hidden` |
| `sort` | enum, optional | The order of the rows of a `multi` tooltip. `none`: the series order. `asc`: the lowest value first. `desc`: the highest value first. Default `none`. | `options.tooltip.sort` (`none`, `asc`, `desc`) |
| `hide_zeros` | boolean, optional | `true`: a `multi` tooltip has no row for a value that equals 0. Default `false`. | `options.tooltip.hideZeros` |

Object `reduce`:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `calc` | enum | `last`, `last_not_null`, `first`, `first_not_null`, `min`, `max`, `mean`, `total`, `count`, `median`, `std_dev`, `variance`, `range`, `diff`, `diff_percent`, `delta`, `distinct_count`, `change_count` (9.19 gives each one) | `options.reduceOptions.calcs`, an array of one item (section 15 gives the class of another length) |
| `fields` | text or null | `null`: each field of type number. Else a pattern of the Grafana form (4.24) on the display names. | `options.reduceOptions.fields`; the empty text gives `null` |
| `values` | boolean | `true`: one item for each row, with no reduction. `false`: one item for each field, reduced with `calc`. | `options.reduceOptions.values` |

### 4.27 Options of `table`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `show_header` | boolean | `true`: the table shows its header row | `options.showHeader` |
| `sort` | array of objects | 0 to 8 sort keys: the first sort. The viewer can change it (section 9). | `options.sortBy` |
| `wrap` | boolean, optional | `true`: a long cell text wraps to more lines and the row grows (9.20). `false`: one line with an ellipsis. Default `false`. | `fieldConfig.defaults.custom.wrapText` (15.6) |

A sort key has the members `field` (text, 1 to 1024 bytes: a display name) and `desc` (boolean). The column filter is the property `filterable` of 4.18.

### 4.28 Options of `timeseries`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `draw` | enum | `line`, `bars`, `points` | `custom.drawStyle` |
| `interpolation` | enum | `linear`, `smooth`, `step_before`, `step_after` | `custom.lineInterpolation` |
| `line_width` | integer | 0 to 10 (CSS pixels) | `custom.lineWidth` |
| `fill_opacity` | integer | 0 to 100 (percent) | `custom.fillOpacity` |
| `show_points` | enum | `auto`, `always`, `never` | `custom.showPoints` |
| `point_size` | integer | 1 to 40 (CSS pixels) | `custom.pointSize` |
| `span_nulls` | boolean | `false`: a null value breaks the line. `true`: the line joins the values on both sides of a null. | `custom.spanNulls` |
| `stacking` | enum | `none`, `normal`, `percent` | `custom.stacking.mode` |
| `y_scale` | enum | `linear`, `log` (the base of a log scale is `log_base`) | `custom.scaleDistribution.type` (15.6) |
| `legend` | object | 4.26 | `options.legend` |
| `tooltip` | object | 4.26 | `options.tooltip` |
| `thresholds_style` | enum, optional | How the thresholds show in the plot: `off`, `line`, `dashed`, `area`, `line_area`, `dashed_area`. Default `off`. | `custom.thresholdsStyle.mode`; `line+area` gives `line_area`, `dashed+area` gives `dashed_area` (15.6) |
| `gradient_mode` | enum, optional | The gradient of the fill and of the line (9.19): `none`, `opacity`, `hue`, `scheme`. Default `none`. | `custom.gradientMode` |
| `log_base` | integer, optional | `2` or `10`: the base of the log scale of `y_scale` = `log`. Default `10`. | `custom.scaleDistribution.log` (15.6) |
| `span_nulls_ms` | integer, optional | 1 to 1,000,000,000,000: the null threshold in ms. A run of `null` values joins when the values on its two sides are less than this apart (9.19). No default. | `custom.spanNulls` as a number (15.6) |
| `bar_alignment` | enum, optional | `before`, `center`, `after`: the place of a bar against its time (9.19). Default `center`. | `custom.barAlignment` (15.6) |

The members `gradient_mode`, `log_base`, `span_nulls_ms` and `bar_alignment` are optional members of format 3. A document with no such member does not change. `log_base` has an effect only when `y_scale` is `log`. `span_nulls_ms` has an effect only when `span_nulls` is `false`, and not on a series that draws bars. `bar_alignment` has an effect only on bars.

These options apply to all series of the panel. An override can give one series another `draw`, `line_width`, `fill_opacity`, `show_points` or `point_size` (4.18). Some values need code of Hune around the copied chart engine, for example the legend `placement` `right` and the legend `mode` for each panel. Section 9 (9.19, 9.24) says how the page draws them.

### 4.29 Options of `stat`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `reduce` | object | 4.26 | `options.reduceOptions` |
| `color_mode` | enum | `none`, `value` (the text takes the colour), `background` (the box takes the colour) | `options.colorMode` |
| `graph_mode` | enum | `none`, `area` (a small area chart under the value; it needs a time field) | `options.graphMode` |
| `text_mode` | enum | `auto`, `value`, `value_and_name`, `name`, `none` | `options.textMode` |
| `orientation` | enum | `horizontal`: the items are one below the other, as in Grafana. `vertical`: the items are side by side. `auto`: side by side when the panel is wider than it is high, else one below the other. | `options.orientation` |
| `justify` | enum, optional | `auto`: the text of an item is centred when the item shows no name, else it is left-aligned. `center`: the text is always centred. Default `auto`. | `options.justifyMode` |
| `percent_change` | boolean, optional | `true`: each item that has a calculation also shows the change from the first value to the last value, in percent of the first value (9.19). Default `false`. | `options.showPercentChange` |
| `percent_change_color` | enum, optional | The colour of the change. `standard`: green for a rise, red for a fall or for no change. `inverted`: red for a rise, green for a fall, red for no change. `same_as_value`: the colour of the value. Default `standard`. | `options.percentChangeColorMode` |
| `title_size` | integer, optional | 1 to 200 (CSS pixels): the font size of the name. When the member is absent, the page fits the size (9.19). | `options.text.titleSize` |
| `value_size` | integer, optional | 1 to 200 (CSS pixels): the font size of the value. When the member is absent, the page fits the size (9.19). | `options.text.valueSize` |

The members `percent_change`, `percent_change_color`, `title_size` and `value_size` are optional members of format 3. A document with no such member does not change. The canonical writer leaves out `percent_change` = `false` and `percent_change_color` = `standard` (4.38, rule 6).

### 4.30 Options of `barchart`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `orientation` | enum | `vertical`, `horizontal`, `auto`. `auto` draws horizontal bars when the plot is narrower than it is high, else vertical bars (9.19). | `options.orientation`; an absent member is `auto`, the Grafana default |
| `x_field` | text or null | 1 to 1024 bytes: the display name of the category field. `null`: the first field of type text or time (section 9). | `options.xField` |
| `stacking` | enum | `none`, `normal`, `percent` | `options.stacking` |
| `show_values` | enum | `auto`, `always`, `never` | `options.showValue` |
| `bar_width` | integer | 10 to 100: the bar width in percent of its slot | `options.barWidth` (multiplied by 100) |
| `group_width` | integer | 10 to 100: the group width in percent of its slot | `options.groupWidth` (multiplied by 100) |
| `x_tick_rotation` | integer | -90 to 90 (degrees) | `options.xTickLabelRotation` |
| `legend` | object | 4.26 | `options.legend` |
| `tooltip` | object | 4.26 | `options.tooltip` |
| `thresholds_style` | enum, optional | As in 4.28. A line or an area shows on the value axis. | `custom.thresholdsStyle.mode` (15.6) |
| `fill_opacity` | integer, optional | 0 to 100 (percent): the opacity of the fill of each bar. Default 80. | `custom.fillOpacity` |
| `line_width` | integer, optional | 0 to 10 (CSS pixels): the width of the line around each bar. Default 1. | `custom.lineWidth` |
| `gradient_mode` | enum, optional | The gradient of the fill and of the line of the bars (9.19): `none`, `opacity`, `hue`, `scheme`. Default `none`. | `custom.gradientMode` |

The members `fill_opacity` and `line_width` are optional members of format 3. A document with no such member does not change. These two values apply to all the bars of the panel. An override cannot give one series another value. The member `gradient_mode` is an optional member of format 3 too, with the rule of 4.28.

### 4.31 Options of `piechart`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `pie_type` | enum | `pie`, `donut` | `options.pieType` |
| `reduce` | object | 4.26 | `options.reduceOptions` |
| `labels` | array of enum | 0 to 3 items, no two equal: `name`, `value`, `percent` | `options.displayLabels` |
| `legend` | object | 4.26, with the `values` of a `piechart` | `options.legend` |
| `tooltip` | object | 4.26 | `options.tooltip` |

### 4.32 Options of `xychart`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `x_field` | text or null | 1 to 1024 bytes: the display name of the x field. `null`: the first field of type number. | `options.series[].x` |
| `y_fields` | array of text | 0 to 32 display names. An empty array: each other field of type number. | `options.series[].y` |
| `draw` | enum | `points`, `lines`, `points_and_lines` | `options.series[].show`, or `custom.show` (15.6) |
| `point_size` | integer | 1 to 40 (CSS pixels) | `custom.pointSize` |
| `line_width` | integer | 0 to 10 (CSS pixels) | `custom.lineWidth` |
| `legend` | object | 4.26 | `options.legend` |
| `tooltip` | object | 4.26 | `options.tooltip` |

### 4.33 Options of `bargauge`

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `reduce` | object | 4.26 | `options.reduceOptions` |
| `orientation` | enum | `horizontal`, `vertical` | `options.orientation` |
| `display_mode` | enum | `basic`, `gradient`, `lcd` | `options.displayMode` |
| `show_unfilled` | boolean | `true`: the empty part of each bar shows | `options.showUnfilled` |

### 4.33a Panel type `text`

A panel of the type `text` shows a text that a person writes in Markdown. It has no query and no data frame. The text is data. The page shows it as DOM nodes and never as HTML (9.19b). This is the text panel of Grafana, in its mode `markdown` only.

The members of a panel of this type are:

| Member | Type | Rule |
|---|---|---|
| `id`, `type`, `title`, `description`, `transparent`, `grid` | as 4.13 | `type` is `text`. |
| `options` | object | The closed object below. |
| `links` | array of objects | 0 to 16 links in the header of the panel (4.23). |
| `data`, `transforms`, `fields`, `repeat` | none | A text panel MUST NOT have these members. The validation gives `not_permitted` at the member. |

The object `options` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `mode` | enum | Necessary. The only value is `markdown`. | `options.mode` (15.6) |
| `content` | array of text | Necessary. 0 to `MAX_TEXT_LINES` lines. Rule 2 gives the form of a line. | `options.content` (15.6) |

1. The canonical order. The members of the panel come in the order of the table above (4.38). The order of `options` is `mode`, then `content`.
2. The lines of `content`. The form is the form of the SQL lines (4.16).
   - A line MUST NOT contain U+000A or U+000D. It MAY contain U+0009. It MUST NOT contain another control character.
   - The text is the lines joined with one U+000A between two lines. No line end follows the last line.
   - An empty array is an empty text.
3. The limits (4.35). A line has `MAX_TEXT_LINE_BYTES` bytes at most. The array has `MAX_TEXT_LINES` lines at most. The joined text has `MAX_TEXT_BYTES` bytes at most. Above a limit the validation gives `too_large` at `options/content` or at `options/content/<i>`.
4. The mode. A value other than `markdown` gives `bad_format` at `options/mode`. The values `html` and `code` of Grafana are not in the format. A mode `html` would need a sanitiser for a list of tags, and the rule of the format says that HTML is text. The importer maps `code` to a code block (15.6).
5. The Markdown is not checked. The validation reads lines only. It does not parse Markdown. The page reads Markdown, so a text that breaks a limit of the page is a valid document, and the page shows a message (9.19b). The server never renders the text and never reads it for another purpose.
6. A reuse. A `reuse` MUST NOT name a text panel (4.15, rule 2). A text panel MUST NOT repeat (4.13a, rule 4). The messages of the errors say that "a text panel has no query".
7. The placeholders of variables. The content can hold placeholders (9.19b). The validation checks none of them, and the save gives no warning for them.
8. The format. The type `text` is a new value of the enum `type`. It joins format 3 with no new version and no migration (4.42).

Example of a valid panel:

```json
{
  "id": 7,
  "type": "text",
  "title": "Notes",
  "description": "",
  "transparent": false,
  "grid": { "x": 0, "y": 0, "w": 12, "h": 8 },
  "options": {
    "mode": "markdown",
    "content": [
      "# Runbook",
      "",
      "Region: $region. See [the wiki](https://wiki.example/runbook)."
    ]
  },
  "links": []
}
```

The fixture `tests/fixtures/dashboards/text_panel.json` holds this panel and the boundary cases of rule 3. The files `tests/fixtures/dashboards/invalid/f29_*.json` hold one error each (16.3).

### 4.33b Panel types `state_timeline` and `status_history`

A panel of the type `state_timeline` or `status_history` draws the state of each value field of its frame over time. It has the members of 4.13, with `data`, `transforms` and `fields`. The query can have the format `table` or `time_series`. The frame needs one field of the kind `time`. 9.19c gives the rules of the data and of the picture.

The object `options` of a `state_timeline` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `merge_values` | boolean | `true`: consecutive samples with the same state make one segment. `false`: each sample is one segment. | `options.mergeValues` |
| `show_values` | enum | `auto`, `always` or `never`: the text in a segment (9.19c). | `options.showValue` |
| `align_values` | enum | `left`, `center` or `right`: the place of the text in a segment. | `options.alignValue` |
| `row_height` | integer | 10 to 100: the share of the plot height that the bars take, in percent. | `options.rowHeight` (times 100) |
| `line_width` | integer | 0 to 10 (CSS pixels): the line around a segment. | `custom.lineWidth` |
| `fill_opacity` | integer | 0 to 100 (percent): the opacity of the fill. | `custom.fillOpacity` |
| `span_nulls` | boolean | `false`: a sample with no value ends the segment. `true`: the segment goes on over such a sample. | `custom.spanNulls` |
| `show_labels` | boolean | `true`: the name of each row shows at the left of the plot. | `custom.axisPlacement` (15.6) |
| `legend` | object | 4.26, with the rules below | `options.legend` |
| `tooltip` | object | 4.26, with the rules below | `options.tooltip` |

The object `options` of a `status_history` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `show_values` | enum | `auto`, `always` or `never`: the text in a cell | `options.showValue` |
| `row_height` | integer | 10 to 100: the share of the plot height that the rows take, in percent | `options.rowHeight` (times 100) |
| `col_width` | integer | 10 to 100: the width of a cell, in percent of the step between two samples | `options.colWidth` (times 100) |
| `line_width` | integer | 0 to 10 (CSS pixels): the line around a cell | `custom.lineWidth` |
| `fill_opacity` | integer | 0 to 100 (percent) | `custom.fillOpacity` |
| `show_labels` | boolean | `true`: the name of each row shows at the left of the plot | `custom.axisPlacement` (15.6) |
| `legend` | object | 4.26, with the rules below | `options.legend` |
| `tooltip` | object | 4.26, with the rules below | `options.tooltip` |

1. The canonical order. The members of `options` come in the order of the table of the type (4.38). Each member is necessary. A member of the other type gives `unknown_member`.
2. The legend. The members `show` and `placement` work as in 4.26. The member `mode` MUST be `list`. The member `values` MUST be an empty array. The members `sort_by` and `sort_desc` MUST be absent. Else the validation gives `not_permitted` at the member.
3. The tooltip. The member `mode` works as in 4.26. The member `sort` MUST be absent or `none`. The member `hide_zeros` MUST be absent or `false`. Else the validation gives `not_permitted` at the member.
4. The field settings. The members `unit`, `decimals`, `min`, `max`, `color`, `thresholds`, `mappings`, `display_name` and `hide_from` of 4.18 apply to a row. The member `links` and the member `no_value` are valid, and they have no effect on these types.
5. The style members of a series (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`, `axis_placement`, `axis_label`, `soft_min` and `soft_max`) are not for these types. In a property set they give `not_permitted` (4.18, rule 5). The options above hold `line_width` and `fill_opacity` for the whole panel.
6. The colour. A property set without `color` gives the mode `continuous_gr_yl_rd` to a `state_timeline` and the mode `thresholds` to a `status_history` (9.14). The validation does not check this default.
7. The limits. The format has no limit of its own for these types. The page has the cap `MAX_TIMELINE_SEGMENTS` (9.19c). A document that breaks the cap is valid, and the page shows a message.
8. The two types are separate, because the options, the defaults and the caps differ. One type with a `mode` would need rules of the kind "this member is valid with this mode only" for 5 members. The import maps one Grafana id to one type (15.6).

The fixtures `tests/fixtures/dashboards/state_timeline.json` and `status_history.json` hold the boundary cases of the options. The files `tests/fixtures/dashboards/invalid/f30_*.json` hold one error each (16.3).

### 4.33c Panel type `heatmap`

A panel of the type `heatmap` draws a grid of cells, with a time bucket for each column and a value bucket for each row. The colour of a cell shows its count. The panel has the members of 4.13, with `data`, `transforms` and `fields`. The query can have the format `table` or `time_series`. 9.19e gives the rules of the data, of the bucket maths and of the picture.

The object `options` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `calculate` | boolean | `false`: the frame is already bucketed: each field of the kind number is a row of cells. `true`: the page buckets the values of the number fields (9.19e). | `options.calculate` |
| `x_buckets` | object | The time buckets of `calculate` = `true`. The members are below. | `options.calculation.xBuckets` |
| `y_buckets` | object | The value buckets of `calculate` = `true`. The members are below. | `options.calculation.yBuckets` |
| `y_axis` | enum | `left` or `hidden`: the place of the value axis. | `options.yAxis.axisPlacement` (15.6) |
| `y_reverse` | boolean | `true`: the small values are at the top. | `options.yAxis.reverse` |
| `y_min` | number or null | The lowest value of the axis. `null`: the lowest edge of the cells. | `options.yAxis.min` |
| `y_max` | number or null | The highest value of the axis. `null`: the highest edge of the cells. | `options.yAxis.max` |
| `color` | object | The colour scale. The members are below. | `options.color` |
| `cell_gap` | integer | 0 to 25 (CSS pixels): the gap between two cells. | `options.cellGap` |
| `hide_le` | number or null | A cell with a count at or below this number is not drawn. `null`: no such rule. | `options.filterValues.le` (15.6) |
| `hide_ge` | number or null | A cell with a count at or above this number is not drawn. `null`: no such rule. | `options.filterValues.ge` |
| `show_legend` | boolean | `true`: the colour bar shows under the plot. | `options.legend.show` |
| `tooltip` | object | 4.26, with the rules below. | `options.tooltip` |

The object `x_buckets` has these members:

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `count`: the page makes about `value` buckets in the time range. `size`: each bucket has the length `value`. |
| `value` | integer or null | With `count`: 1 to 1,000, or `null` for 20 buckets. With `size`: a length in milliseconds, from 1 to 31,536,000,000 (365 days). |

The object `y_buckets` has these members:

| Member | Type | Rule |
|---|---|---|
| `mode` | enum | `count`: about `value` buckets between the lowest and the highest value. `size`: each bucket has the width `value`. |
| `value` | number or null | Scale `linear` and `count`: an integer from 1 to 1,000, or `null` for 10 buckets. Scale `linear` and `size`: a number above 0. Scale `log2` or `log10`: an integer from 1 to 16 (the buckets for each power of the base), or `null` for 1. |
| `scale` | enum | `linear`, `log2` or `log10`: the scale of the value axis and of the buckets. |

The object `color` has these members. All of them are necessary, also when the mode does not use them.

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `mode` | enum | `scheme`: the colours of a named scheme. `opacity`: one colour with a changing opacity. | `color.mode` |
| `scheme` | enum | `oranges`, `blues`, `greens`, `greys`, `purples`, `reds`, `yl_or_rd`, `rd_yl_gn`, `spectral` or `viridis` (9.19e). | `color.scheme` (15.6) |
| `fill` | text | A colour of 4.19. The colour of the mode `opacity`. | `color.fill` |
| `scale` | enum | `exponential` or `linear`: the rule of the opacity in the mode `opacity`. | `color.scale` |
| `exponent` | number | 0.1 to 2, with one decimal at most. The exponent of the scale `exponential`. | `color.exponent` |
| `steps` | integer | 2 to 128: the number of colours of the scale. | `color.steps` |
| `reverse` | boolean | `true`: the scale goes from the last colour to the first colour. | `color.reverse` |
| `min` | number or null | The count that has the first colour. `null`: the lowest count that shows. | `color.min` |
| `max` | number or null | The count that has the last colour. `null`: the highest count that shows. | `color.max` |

1. The canonical order. The members of `options`, `x_buckets`, `y_buckets` and `color` come in the order of their tables (4.38). Each member is necessary. A member of another type gives `unknown_member`.
2. The log scale. A `y_buckets` with `scale` = `log2` or `log10` MUST have `mode` = `size`. Else the validation gives `not_permitted` at `y_buckets/mode`.
3. The colour range. When `color.min` and `color.max` are both numbers, `min` MUST be lower than `max`. Else the validation gives `out_of_range` at `color/max`.
4. The tooltip. The member `mode` works as in 4.26. The member `sort` MUST be absent or `none`. The member `hide_zeros` MUST be absent or `false`. Else the validation gives `not_permitted` at the member.
5. The field settings. The members `unit`, `decimals` and `display_name` of 4.18 apply to the values of the frame. They are the y values with `calculate` = `true`, and the counts with `calculate` = `false`. The other members of 4.18 are valid and have no effect, because the colours come from `color`.
6. The style members of a series (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`, `axis_placement`, `axis_label`, `soft_min` and `soft_max`) are not for this type. In a property set they give `not_permitted` (4.18, rule 5).
7. The limits. The format limits are in the tables. The page has the caps `MAX_BUCKET_VALUES` and `MAX_HEAT_CELLS` (9.19e). A document that breaks a cap is valid, and the page shows a message.
8. The defaults of a new panel are in 9.19e, section 13.

The fixture `tests/fixtures/dashboards/heatmap.json` holds the boundary cases of the options. The files `tests/fixtures/dashboards/invalid/f31_heatmap_*.json` hold one error each (16.3).

### 4.33d Panel type `histogram`

A panel of the type `histogram` draws bars. Each bar is the count of the values in one bucket. The panel has the members of 4.13, with `data`, `transforms` and `fields`. The query can have the format `table` or `time_series`. 9.19f gives the rules.

The object `options` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `bucket_count` | integer | 1 to 1,000. The page makes about this number of buckets when `bucket_size` is `null`. | `options.bucketCount` |
| `bucket_size` | number or null | A number above 0: the width of a bucket. `null`: the page chooses the width (9.19f). | `options.bucketSize` |
| `bucket_offset` | number | Any finite number. The edges of the buckets are `bucket_offset` plus a whole number of widths. | `options.bucketOffset` |
| `combine` | boolean | `true`: one bar series for all the fields. `false`: one bar series for each field. | `options.combine` |
| `stacking` | enum | `none`, `normal` or `percent`. | `custom.stacking.mode` |
| `line_width` | integer | 0 to 10 (CSS pixels): the line around a bar. | `custom.lineWidth` |
| `fill_opacity` | integer | 0 to 100 (percent): the opacity of the fill of a bar. | `custom.fillOpacity` |
| `legend` | object | 4.26. | `options.legend` |
| `tooltip` | object | 4.26. | `options.tooltip` |

1. The canonical order. The members of `options` come in the order of the table (4.38). Each member is necessary. A member of another type gives `unknown_member`.
2. The field settings. The members `unit`, `decimals`, `color`, `display_name`, `min`, `max` and `hide_from` of 4.18 apply to a field. The members `thresholds`, `mappings`, `links` and `no_value` are valid and have no effect.
3. The style members of a series (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`, `axis_placement`, `axis_label`, `soft_min` and `soft_max`) are not for this type. In a property set they give `not_permitted` (4.18, rule 5). The options above hold `line_width` and `fill_opacity` for the whole panel.
4. The colour. A property set without `color` gives the mode `palette` (9.14): the series palette, one slot for each field.
5. The limits. The format has no limit of its own besides the ranges above. The page has the caps `MAX_BUCKET_VALUES` and `MAX_HIST_BUCKETS` (9.19f). A document that breaks a cap is valid, and the page shows a message.
6. The gradient mode of Grafana has no member. The importer lists it (15.6).

The fixture `tests/fixtures/dashboards/histogram.json` holds the boundary cases of the options. The files `tests/fixtures/dashboards/invalid/f31_histogram_*.json` hold one error each (16.3).

### 4.33e Panel type `gauge`

A panel of the type `gauge` draws each value on a round scale. It is the classic gauge of Grafana 12.4.2, in which the flag `newGauge` is off. The panel has the members of 4.13, with `data`, `transforms` and `fields`. The query can have the format `table` or `time_series`. 9.19g gives the rules.

The object `options` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `reduce` | object | 4.26. | `options.reduceOptions` |
| `orientation` | enum, optional | `auto`: a grid of gauges. `horizontal`: the gauges are one below the other. `vertical`: the gauges are side by side. Default `auto`. | `options.orientation` |
| `show_threshold_labels` | boolean, optional | `true`: the ends of the scale, the values of the thresholds and the neutral value show around the arc. Default `false`. | `options.showThresholdLabels` |
| `show_threshold_markers` | boolean, optional | `true`: a thin band outside the arc shows the colours of the thresholds. Default `true`. | `options.showThresholdMarkers` |
| `neutral` | number, optional | Any finite number: the filled arc goes from this value to the value of the gauge. Absent: the filled arc starts at the minimum. | `fieldConfig.defaults.custom.neutral` |
| `title_size` | integer, optional | 1 to 200 (CSS pixels): the size of the name under each gauge. Absent: the automatic size of 9.19g. | `options.text.titleSize` |
| `value_size` | integer, optional | 1 to 200 (CSS pixels): the size of the value text. Absent: the automatic size of 9.19g. | `options.text.valueSize` |
| `sizing` | enum, optional | `auto` or `manual`. `manual` reads `min_width` and `min_height`. Default `auto`. | `options.sizing` |
| `min_width` | integer, optional | 0 to 600 (CSS pixels): the least width of a gauge with `sizing` = `manual` and `orientation` = `vertical`. Default 75. | `options.minVizWidth` |
| `min_height` | integer, optional | 0 to 600 (CSS pixels): the least height of a gauge with `sizing` = `manual` and `orientation` = `horizontal`. Default 75. | `options.minVizHeight` |

1. The canonical order. The members of `options` come in the order of the table (4.38). The member `reduce` is necessary. The other nine members are optional. A member of another type gives `unknown_member`.
2. The default values. An absent optional member means its default value. The writer leaves out an optional member that has its default value (4.38, rule 6). A neutral value of 0 is a value, so the writer keeps it.
3. The format. The type `gauge` is a new value of the enum `type`. It joins format 3 with no new version and no migration (4.42, rule 3). A binary that does not know the type refuses the document with `bad_format` at `type`.
4. The field settings. The members `unit`, `decimals`, `min`, `max`, `color`, `thresholds`, `mappings`, `links`, `display_name` and `no_value` of 4.18 apply to a field. The colour mode `thresholds` is the default (9.14).
5. The style members of a series (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`, `axis_placement`, `axis_label`, `soft_min`, `soft_max` and `axis_grid`) are not for this type. In a property set they give `not_permitted` (4.18, rule 5).
6. The members `neutral`, `min_width` and `min_height` hold one value for the whole panel. In Grafana, `neutral` is a setting of a field. The importer reads it in the defaults only (15.6).
7. A hidden effect. With `sizing` = `auto`, the page uses 75 px and does not read `min_width` and `min_height`. With `orientation` = `vertical` it does not read `min_height`, and with `horizontal` it does not read `min_width`. These members stay valid, as in Grafana.

### 4.33f Panel type `logs`

A panel of the type `logs` shows one line of text for each row of its frame. A line has the time, the level and the body. A click on a line opens a details row with the other fields of the row. The panel has the members of 4.13, with `data`, `transforms` and `fields`. 9.19h gives the picture, and 7.16e gives the rules that find the time, the body and the level in the frame.

The query of the panel can have the format `logs`, `table` or `time_series` (4.15). The format `logs` is the logs format of the ClickHouse plugin (its format 2). The server runs it as the format `table`, so the frame stays as the source gives it. The format `logs` is permitted in a panel of the type `logs` only.

The object `options` has these members. Each member is optional: an absent member has its default value.

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `show_time` | boolean, optional | `true`: each line starts with its time (9.19h). Default `false`. | `options.showTime` |
| `wrap` | boolean, optional | `true`: a long line wraps to more rows of text. `false`: a line has one row of text, and the list scrolls to the side. Default `false`. | `options.wrapLogMessage` |
| `details` | boolean, optional | `true`: a click on a line opens its details row. `false`: a line has no details row. Default `true`. | `options.enableLogDetails` |
| `order` | enum, optional | `newest_first`: the newest line is at the top. `oldest_first`: the oldest line is at the top. Default `newest_first`. | `options.sortOrder` (`Descending`, `Ascending`) |

1. The canonical order. The members of `options` come in the order of the table (4.38). The writer leaves out a member that has its default value (4.38, rule 6), so the options of a new panel are `{}`. A member of another type gives `unknown_member`.
2. The format. The value `logs` of `format` in another type of panel gives `not_permitted` at `data/query/format`, or at `data/extra_queries/<i>/format` for an extra query (4.15a).
3. One frame. A logs panel draws one frame. It MUST NOT have `extra_queries` (`not_permitted` at `data/extra_queries`, 4.15a, rule 7). A `reuse` panel of this type MUST name a source panel that has no `extra_queries` (`bad_reference` at `data/reuse/panel`, 4.15, rule 6).
4. The field settings. The members of 4.18 that all types have are valid, and they have no effect on the lines. The style members of a series and the members of a table are not for this type. In a property set they give `not_permitted` (4.18, rule 5).
5. The limits. The format has no limit of its own besides the members above. The page has the caps `MAX_LOG_LINES`, `MAX_LOG_LINE_CHARS` and `MAX_LOG_CHARS` (9.19h, section 4). A frame that passes a cap is valid, and the page shows a note.
6. 15.6 lists the options of Grafana that Hune does not have. Examples are the deduplication, the labels, the font size and the infinite scroll.

### 4.33g Panel type `geomap`

A panel of the type `geomap` draws one layer of markers on a map. A marker is the position of one row of the frame. The panel has the members of 4.13, with `data`, `transforms` and `fields`. The query can have the format `table` or `time_series`. 9.19i gives the rules of the data, of the map and of the tiles.

The document holds no tile source. The tile source comes only from the block `geomap` of the configuration (13.12c). Without the block, the panel draws the markers on a plain background, and the page contacts no host. A member such as `basemap`, `layers` or a URL is `unknown_member` (4.36).

The object `options` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `view` | object | The start view. The members are below. | `options.view` (15.6) |
| `layer` | object | The marker layer. The members are below. | `options.layers` (15.6) |
| `tooltip` | enum | `details`: the card of the marker under the pointer. `none`: the card shows after a click only. | `options.tooltip.mode` |
| `show_zoom` | boolean | `true`: the buttons "Zoom in" and "Zoom out" show at the top left of the map. | `options.controls.showZoom` |
| `mouse_wheel_zoom` | boolean | `true`: the mouse wheel zooms the map. | `options.controls.mouseWheelZoom` |

The object `view` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `mode` | enum | `fixed`: the centre `lat`, `lon` at the zoom `zoom`. `fit`: the page fits the view to the markers, up to the zoom `zoom`. The page uses `fit` only when the server has no tile source (9.19i). | `options.view.id` (15.6) |
| `lat` | number | -90 to 90: the latitude of the centre. | `options.view.lat`, or the centre of a preset |
| `lon` | number | -180 to 180: the longitude of the centre. | `options.view.lon`, or the centre of a preset |
| `zoom` | integer | 0 to 22. With `fit`, the highest zoom. | `options.view.zoom` (15.6) |

The object `layer` has these members:

| Member | Type | Rule | Grafana source |
|---|---|---|---|
| `lat_field` | text or null | 1 to 1024 bytes: the display name of the latitude field. `null`: the first field whose name is `lat` or `latitude` (9.19i). | `location.latitude` |
| `lon_field` | text or null | 1 to 1024 bytes: the display name of the longitude field. `null`: the first field whose name is `lon`, `lng` or `longitude`. | `location.longitude` |
| `size` | integer | 1 to 100 (CSS pixels): the radius of a marker when `size_field` is `null`. | `config.style.size.fixed` |
| `size_min` | integer | 1 to 100 (CSS pixels): the radius of the lowest value of `size_field`. | `config.style.size.min` |
| `size_max` | integer | 1 to 100 (CSS pixels): the radius of the highest value of `size_field`. | `config.style.size.max` |
| `size_field` | text or null | 1 to 1024 bytes: the display name of the field that gives the radius. `null`: each marker has the radius `size`. | `config.style.size.field` |
| `color` | text | A colour of 4.19: the colour of a marker when `color_field` is `null`. | `config.style.color.fixed` |
| `color_field` | text or null | 1 to 1024 bytes: the display name of the field whose colour (9.16) gives the colour of each marker. `null`: each marker has the colour `color`. | `config.style.color.field` |
| `opacity` | integer | 0 to 100 (percent): the opacity of the fill of a marker. | `config.style.opacity` (times 100) |

1. The canonical order. The members of `options`, `view` and `layer` come in the order of their tables (4.38). Each member is necessary. A member of another type gives `unknown_member`.
2. The sizes. `size_max` MUST NOT be less than `size_min`. Else the validation gives `out_of_range` at `layer/size_max`.
3. The frame. A `geomap` draws the frame of one query. It MUST NOT have `extra_queries` (4.15a, rule 7).
4. The field settings. The members `unit`, `decimals`, `mappings` and `display_name` of 4.18 apply to the values of the card. The members `color`, `thresholds`, `min` and `max` of the field `color_field` give the colour of a marker (9.16). The members `min` and `max` of the field `size_field` give the range of the radius. The member `hide_from.tooltip` removes a field from the card. The other members are valid and have no effect.
5. The style members of a series (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`, `axis_placement`, `axis_label`, `soft_min` and `soft_max`) are not for this type. In a property set they give `not_permitted` (4.18, rule 5).
6. The limits. The format has no limit of its own besides the ranges above. The page has the caps of 4.35 for the map (9.19i). A document that breaks a cap is valid, and the page shows a message.
7. The format. The type `geomap` is a new value of the enum `type`. It joins format 3 with no new version and no migration (4.42).

The values of a new panel are in 9.19i, section 8. The test file `crates/hune-core/tests/document_geomap.rs` holds the boundary cases of the options and one error for each rule.

### 4.34 Rules across members

The validation checks these rules after the member checks. Each rule gives its error code (4.37) and the pointer of the error.

| Rule | Code | Pointer |
|---|---|---|
| `uid` equals the expected location | `uid_mismatch` | `/uid` |
| Panel ids are unique in the document | `not_unique` | `id` of the second panel |
| Variable names are unique | `not_unique` | `name` of the second variable |
| The document has at most 8 annotations | `too_large` | `/annotations` |
| Annotation names are unique | `not_unique` | `name` of the second annotation |
| An item of `panels` of an annotation is unique and names a panel of the document. The array has 1 item or more. | `not_unique`, `bad_reference` or `out_of_range` | the item, or `panels` for the empty array |
| An annotation names a source that the configuration declares (7.18, item 9) | `bad_reference` | `source` of the annotation |
| The source of an annotation has the kind `clickhouse`, `postgres` or `mongodb` (7.18) | `not_permitted` | `source` of the annotation |
| The document has at most 200 panels | `too_large` | `/sections` |
| Only the first section has `title: null`; `collapsed` is `false` with `title: null` | `not_permitted` | the member |
| `x + w` is 24 or less | `out_of_range` | `grid/w`, or `placement/w` of a variable |
| No two panels of one section overlap | `overlap` | `grid` of the second panel in array order |
| A placement of a variable (4.7a) does not overlap a panel or an earlier placement of its section | `overlap` | `placement` of the later variable |
| `placement.section` names a section of the document | `bad_reference` | `placement/section` |
| `repeat.variable` names a variable of the document | `bad_reference` | `repeat/variable` |
| The variable of a `repeat` has the kind `query` or `custom` and `multi` = `true` | `not_permitted` | `repeat/variable` |
| `max_per_row` is valid with `direction` = `h` only | `not_permitted` | `repeat/max_per_row` |
| A panel with `data.reuse` has no `repeat`. A section with `repeat` has a `title`, no panel with `repeat` and no panel with `data.reuse`. | `not_permitted` | `repeat` of the panel or of the section, or `data/reuse` of the panel |
| A panel with `repeat` and `direction` = `h` shares no row unit with another panel or a placement of its section | `overlap` | `repeat/direction` |
| A panel of the type `text` has none of the members `data`, `transforms`, `fields` and `repeat` | `not_permitted` | the member |
| The `legend` of a `state_timeline` or a `status_history` has `mode` = `list`, `values` = `[]`, and no `sort_by` and no `sort_desc` | `not_permitted` | the member |
| The `tooltip` of a `state_timeline` or a `status_history` has no `sort` other than `none`, and no `hide_zeros` equal to `true` | `not_permitted` | the member |
| The `y_buckets` of a `heatmap` has `scale` = `log2` or `log10` and `mode` = `count` | `not_permitted` | `y_buckets/mode` |
| The `color` of a `heatmap` has `min` and `max` set, and `min` is not lower than `max` | `out_of_range` | `color/max` |
| The `tooltip` of a `heatmap` has `sort` other than `none`, or `hide_zeros` equal to `true` | `not_permitted` | the member |
| The `x_buckets` or the `y_buckets` of a `heatmap` has a `value` that its `mode` and `scale` do not permit | `out_of_range` | `x_buckets/value` or `y_buckets/value` |
| A `heatmap` has `y_min` or `y_max` at or below 0, and its `y_buckets` has `scale` = `log2` or `log10` | `out_of_range` | `y_min` or `y_max` |
| The source of a `reuse` has no `repeat` and is not in a section with `repeat` | `bad_reference` | `data/reuse/panel` |
| A `placement` names no section that has `repeat` | `not_permitted` | `placement/section` |
| A variable with `hide` = `variable` has no `placement` | `not_permitted` | `placement` |
| A `reuse` names another panel that has `query` | `bad_reference` | `data/reuse/panel` |
| `extra_queries` has 1 to 7 items | `out_of_range` | `data/extra_queries` |
| The refs of the queries of a panel are unique. `data.query` has the ref `A` when `ref` is absent. | `not_unique` | `ref` of the second query in array order |
| `extra_queries` is in a panel with `query`, of a type of 4.15a rule 7, with no `repeat` and not in a section with `repeat` | `not_permitted` | `data/extra_queries` |
| The document has at most 400 queries | `too_large` | `/sections` |
| A `reuse` panel of the type `barchart`, `xychart` or `logs` names a source with no `extra_queries` | `bad_reference` | `data/reuse/panel` |
| The format `logs` is in a panel of the type `logs` only (4.33f) | `not_permitted` | `data/query/format` or `data/extra_queries/<i>/format` |
| A matcher `query` has a `value` of the grammar of 4.3 | `bad_format` | `value` |
| `all_value` is `null` when `include_all` is `false` | `not_permitted` | `all_value` |
| The rules of `default` (4.11) | `not_permitted` or `out_of_range` | the item |
| Custom option values are unique | `not_unique` | the second `value` |
| Threshold steps: the first value is `null`, and each other value is greater than the value before it | `not_permitted` or `out_of_range` | the step |
| Table-only properties in a table panel only | `not_permitted` | the member |
| `no_value` in `fields.defaults` only | `not_permitted` | the member |
| `min` less than `max` in one set | `out_of_range` | `max` |
| `soft_min` is less than `soft_max` in one set | `out_of_range` | `soft_max` |
| The members `draw`, `line_width`, `fill_opacity`, `show_points` and `point_size` are in an override `set` only | `not_permitted` | the member |
| A member of 4.18 for `timeseries` or `barchart` in a panel of another type | `not_permitted` | the member |
| `title` of a dashboard link of the kind `dashboards` has 1 character or more when `as_dropdown` is `true` | `out_of_range` | `title` |
| `cell.mode` is a mode exactly when `cell.type` is `color_background` or `gauge`; `lcd` is permitted with `gauge` only | `not_permitted` or `bad_format` | `cell/mode` |
| `color.fixed` is a colour exactly when `color.mode` is `fixed` | `not_permitted` | `color/fixed` |
| An override `set` has 1 member or more | `missing_member` | `set` |
| A mapping has `text` or `color` | `missing_member` | the mapping |
| A `range` mapping has a bound; `from` at most `to` | `missing_member` or `out_of_range` | the mapping |
| A `range` matcher has `from` at most `to` | `out_of_range` | `to` |
| Absolute `time.from` less than absolute `time.to` | `out_of_range` | `/time/to` |
| A `text` variable with `number` has a decimal `default` | `bad_format` | `default` |
| Items of `tags` (of the document and of a dashboard link), `values`, `labels` and `footer.reducers` are unique | `not_unique` | the second item |
| The `query` variables have no cycle of references. A reference is a placeholder of section 7 in the SQL of a `query` variable that names a variable of the document. A variable that names itself is a cycle. The scan is the name scan of 7.19. | `bad_reference` | `sql` of the first variable of the cycle in array order |
| An item of `intervals` matches the interval text of 4.10a and is 365 days or less | `bad_format`, `out_of_range` | the item |
| `auto_min` is one of the 30 texts of 7.4a | `bad_format` | `auto_min` |
| The `default` of an `interval` is an item, or `$__auto` with `auto` = `true` | `not_permitted` | `default` |
| A `constant` has `hide` = `variable` | `not_permitted` | `hide` |
| A `constant` has no control character in `value` | `bad_format` | `value` |
| A `calculate_field` has exactly the object of its `mode` | `missing_member` or `unknown_member` | the object |
| A `calculate_field` with `mode` = `binary` has two operands that are not both numbers | `not_permitted` | `binary` |
| Two items of one array of 4.25a are equal, or two items of `rename` or `conversions` have the same `field` | `not_unique` | the second item |
| A pattern of 4.25a is not valid (4.24) | `bad_format` | the pattern |
| `limit.count` is 0 to 1,000,000 | `out_of_range` | `count` |

### 4.35 Limits

The limits are constants of format 3 in `hune-core` (`limits.rs`). They are not configuration keys. Thus a document that is valid on one instance is valid on all instances and in both backends.

| Constant | Value | Applies to |
|---|---|---|
| `MAX_DOCUMENT_BYTES` | 4,194,304 (4 MiB) | The input bytes and the canonical bytes of one document |
| `MAX_JSON_DEPTH` | 32 | The depth of objects and arrays; the top object has depth 1 |
| `MAX_SQL_BYTES` | 262,144 (256 KiB) | One joined SQL text (4.16). The longest measured query (71,119 characters) uses 28 percent. |
| `MAX_TEXT_LINES` | 2,000 | Lines of `options.content` of a text panel (4.33a) |
| `MAX_TEXT_LINE_BYTES` | 8,192 | One line of `options.content` |
| `MAX_TEXT_BYTES` | 65,536 (64 KiB) | The joined text of `options.content`. The 200 panels of a document can hold 12.5 MiB of text in all, so the limit of the document (`MAX_DOCUMENT_BYTES`) applies first. |
| `MAX_PANELS` | 200 | Panels in one document |
| `MAX_REPEAT_PER_PANEL` | 24 | Default of the copies of one panel: the values of its repeat variable (4.13a). |
| `MAX_REPEAT_PER_SECTION` | 12 | Default of the copies of one section. Each copy repeats all the panels of the section. |
| `MAX_REPEAT_PER_DASHBOARD` | 120 | Default of the copies of panels that repeat in one dashboard. A panel in a copy of a repeated section counts for each copy of the section. The panels with no repeat do not count. |
| `MAX_QUERIES_PER_PANEL` | 8 | Queries of one panel: `data.query` and at most 7 items of `extra_queries` (4.15a). Grafana has no limit. |
| `MAX_QUERIES` | 400 | Queries in one document, hidden queries included (4.15a). It is twice `MAX_PANELS`. |
| `MAX_SECTIONS` | 50 | Sections in one document |
| `MAX_VARIABLES` | 64 | Variables in one document |
| `MAX_INTERVALS` | 50 | Items of `intervals` in one `interval` variable. Grafana has no limit; its default list has 10 items. |
| `MAX_ADHOC_KEYS` | 64 | Items of `keys` in one `adhoc` variable (4.10c). |
| `MAX_ADHOC_FILTERS` | 16 | Filters of one `adhoc` variable in one query request (7.7a). The server enforces it. |
| `MAX_ANNOTATIONS` | 8 | Annotations in one document (4.5b). One query runs for each annotation at each trigger (9.19d). |
| `MAX_ANNOTATION_PANELS` | 200 | Items of `panels` of one annotation. It equals `MAX_PANELS`. |
| `MAX_ANNOTATION_ROWS` | 1,000 | Rows of the result of one annotation query (7.37). A longer result is an error. The server enforces it. |
| `MAX_ANNOTATIONS_PER_PANEL` | 2,000 | Marks that the annotations of one panel give together (9.19d). The page enforces it. |
| `MAX_ANNOTATION_TITLE_CHARS` | 256 | One title of a mark. It equals `MAX_TITLE_CHARS`. |
| `MAX_ANNOTATION_TEXT_BYTES` | 2,048 | One text of a mark. |
| `MAX_ANNOTATION_TAGS`, `MAX_ANNOTATION_TAG_CHARS` | 8, 64 | Tags of one mark, and the characters of one tag. |
| `MAX_OVERRIDES` | 512 | Overrides in one panel |
| `MAX_TRANSFORMS` | 16 | Transforms in one panel |
| `MAX_FILTERS` | 32 | Filters in one `filter_by_value` |
| `MAX_TRANSFORM_ITEMS` | 64 | Items of each array of 4.25a, except the arrays that have their own bound (`sort`, `reducers`, `fields` of `partition_by_values` and `conversions`) |
| `MAX_TRANSFORM_ROWS` | 200,000 | Rows of one frame after a transform, and rows in all the frames that `merge` or `join_by_field` read. The page enforces it, with an error and no cut (9.21a, rule 13). It equals `MAX_JOIN_ROWS`. |
| `MAX_TRANSFORM_FIELDS` | 2,000 | Fields of one frame after a transform, a long-to-wide (9.22) or a join on time (9.22a). The page enforces it. |
| `MAX_TRANSFORM_CELLS` | the configured value | Cells (fields times rows) of the frame that `join_by_field`, `merge`, `grouping_to_matrix`, a long-to-wide or a join on time makes. The page checks it before it builds the frame (9.21a, rule 13). The value is the key `server.transform_max_cells` (13.8): the default is 10,000,000. A file can set it from 1 to 100,000,000. |
| `MAX_TRANSFORM_FRAMES` | 100 | Frames after a transform. The page enforces it. |
| `MAX_MERGE_STEPS` | 4,000,000 | Steps of one `merge`. A compare of two rows is one step for each field of the result. The page enforces it. |
| `MAX_TITLE_CHARS` | 256 | Each title, label and option text |
| `MAX_DESCRIPTION_CHARS` | 4096 | Each description |
| `MAX_TAGS` | 32 | Tags of one document; each tag has at most 64 characters |
| `MAX_VARIABLE_VALUES` | 10,000 | Items of one `default`; options of one `custom` variable |
| `MAX_VALUE_BYTES` | 1024 | One variable value, option value, `all_value`, field name or matcher value |
| `MAX_LINKS` | 16 | Links of one panel; links of one property set |
| `MAX_DASHBOARD_LINKS` | 16 | Dashboard links of one document (4.5a) |
| `MAX_LINK_TAGS` | 8 | Tags of one dashboard link of the kind `dashboards` |
| `MAX_TIMEZONE_BYTES` | 64 | One zone name of `time.timezone` (4.6) |
| `MAX_URL_BYTES` | 2048 | One link URL template |
| `MAX_MAPPINGS` | 256 | Mappings of one property set |
| `MAX_THRESHOLD_STEPS` | 32 | Steps of one thresholds object |
| `MAX_PATTERN_BYTES` | 1024 | One regular expression |
| `MAX_SORT_KEYS` | 8 | Sort keys of one table |
| `MAX_MATCHER_NAMES` | 64 | Names of one matcher with `by` = `names` |
| `MAX_FOOTER_REDUCERS` | 8 | Items of `footer.reducers` |
| `REFRESH_CHOICES` | `10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `1d` | The values of `time.refresh` (4.6), and `refresh_choices` of section 6 |
| `MAX_Y_FIELDS` | 32 | Items of `y_fields` |
| `MAX_ERRORS` | 200 | Errors in one validation result (4.36), and faults of the checks at save of one document (7.18) |
| `MAX_WARNINGS` | 200 | Warnings in one answer of a write or of `validate_only` (6.13, 7.18). The last item tells that the list stops. |

The page has its own limits for the parser of the text panel. They are constants of `web/js/markdown.js`. They are not in the format, because a document that breaks one of them is valid. 9.19b gives them. A test compares the three rows above with `limits.rs` and with `markdown.js` (16.10a).

The two limits of the queries are constants of the code, not keys. They are limits of the stored document, so the validation checks them. The page has one limit for the join of frames, `MAX_JOIN_ROWS` = 200,000 rows in all the frames of one panel (9.22a). A document that makes a larger join is valid, and the panel shows a message. A test compares the two rows above with `limits.rs`, and `MAX_JOIN_ROWS` with `web/js/queries.js` (16.10a).

The limits `MAX_TRANSFORM_ROWS`, `MAX_TRANSFORM_FIELDS`, `MAX_TRANSFORM_FRAMES` and `MAX_MERGE_STEPS` are constants of the page. They are not limits of the stored document, so the validation does not check them. A test compares the table with the constants of `web/js/transform_more.js` (16.10a).

The map panel has the caps of the table below (9.19i). They are constants of `web/js/geo_model.js`. They are not limits of the stored document, so the validation does not check them. The front test FE-85 compares the table with the constants (16.10a).

| Constant | Value | Applies to |
|---|---|---|
| `MAX_GEO_POINTS` | 50,000 | Rows of the frame of one map. Above it the panel draws nothing and shows a message. |
| `MAX_GEO_TILES_VIEW` | 256 | Tiles of 256 CSS pixels in one view of one map. Above it the panel draws the plain background and a note. |
| `MAX_GEO_TILES_IN_FLIGHT` | 6 | Tile requests in flight on one page, for all the maps together. |
| `GEO_TILE_DELAY_MS` | 200 | The wait in milliseconds after the last pan or zoom, before the first tile request of a view. |
| `MAX_GEO_TILE_CACHE` | 256 | Decoded tiles that one page keeps. The tile that the page used least recently goes first. |

`MAX_TRANSFORM_CELLS` is not a constant. The key `server.transform_max_cells` sets it, and the page gets the value in force from `GET /api/session` (6.10). The page keeps no copy of the number: a test refuses the number 10,000,000 in the code of `web/js/`. Without a valid value in the session, each step that makes cells fails (9.21a, rule 13). One cell takes 9 bytes or more, so the fixed bound of 100,000,000 cells takes 900 MB or more (13.8). `limits.rs` does not hold `MAX_TRANSFORM_CELLS`, because the server never builds a frame.

The limits of an annotation are constants of the code, not keys. The server enforces the three limits of the rows and the texts, and the page enforces `MAX_ANNOTATIONS_PER_PANEL`. A document that has an annotation query with a longer result is valid, and the query gives an error at run time (7.37). A test compares the rows above with `limits.rs`, with the constants of `hune-core` and with `web/js/annotations.js` (16.10a).

The three repeat limits apply to the count of values at run time, not to the stored document. The validation of 4.36 does not check them. The page checks them (9.8c).

The values in the table are the defaults of the keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` (13.8). A file can set each key from 1 to a hard bound: 200 for a panel, 50 for a section and 200 for a dashboard. The bounds are `MAX_PANELS`, `MAX_SECTIONS` and `MAX_PANELS`. The page gets the values in force from `GET /api/session` (6.10).

A test compares the table with `limits.rs` and with the defaults in `web/js/repeat.js` (16.10a). The stored limit of 200 panels (`MAX_PANELS`) does not change.

Section 13 sets the minimum of `server.max_request_bytes` to `MAX_DOCUMENT_BYTES` plus 65,536, that is 4,259,840 bytes.

### 4.36 Validation

`hune-core` validates a document in these steps. The validation is the only function that makes a `CanonicalDocument` (5.2): the rest of the code reads only the typed value.

1. Check the bytes. The input MUST have at most `MAX_DOCUMENT_BYTES`. It MUST be valid UTF-8. It MUST NOT start with a byte order mark (EF BB BF).
2. Parse the JSON (RFC 8259). The text is one object, with only white space after it. The depth MUST be at most `MAX_JSON_DEPTH`. A duplicate member name in one object is an error.
3. When step 1 or step 2 finds an error, stop. The result holds this one error.
4. Read `format_version`. When it is absent or is not the integer 3, stop with one error `format_version`.
5. Check every member against its table. Continue after an error, and collect all errors.
6. Check the rules of 4.34 on the parts that passed step 5.
7. When the list has `MAX_ERRORS` minus 1 errors and one more error occurs, stop and add one error `too_many_errors`. Thus the list has at most `MAX_ERRORS` items.
8. Without an error, write the canonical bytes (4.38). They MUST have at most `MAX_DOCUMENT_BYTES` (`too_large`). Compute the etag (4.39).

The result is a `CanonicalDocument` or the list of errors. The list is in the order of the walk. For each object, the walk takes the members of the table in table order, then the unknown members in input order. The server, the importer, the store tools and the editor (section 10) use the same result.

### 4.37 Form of a validation error

Each error is one object:

| Member | Type | Rule |
|---|---|---|
| `pointer` | string | The JSON pointer (RFC 6901) of the member or the item. `""` for the whole document. |
| `code` | enum | One code of the table below |
| `message` | string | One English sentence: the member, the rule and the value (at most 200 characters of the value) |
| `line` | integer or null | The line (from 1) for the codes `encoding` and `syntax`. Else `null`. |
| `column` | integer or null | The column in characters (from 1) for the codes `encoding` and `syntax`. Else `null`. |

| Code | Cause |
|---|---|
| `encoding` | The bytes are not UTF-8, or they start with a byte order mark |
| `syntax` | A JSON syntax error |
| `too_large` | A size or a count is above its limit |
| `too_deep` | The depth is above `MAX_JSON_DEPTH` |
| `duplicate_member` | A member name occurs two times in one object |
| `unknown_member` | The table does not list the member |
| `missing_member` | A member that MUST be in the object is absent |
| `wrong_type` | The JSON type is not the type of the table |
| `out_of_range` | A number or a length is outside its range |
| `bad_format` | A text does not match its grammar: id, duration, time text, unit, colour, URL or pattern |
| `not_unique` | A value occurs two times where it MUST be unique |
| `bad_reference` | A `reuse` names a wrong panel, a placement names a section that does not exist, a `repeat` names a variable that does not exist, an annotation names a panel that does not exist, or the `query` variables have a cycle, or a `reuse` panel of the type `barchart` or `xychart` names a panel that has `extra_queries` |
| `overlap` | Two panels of one section overlap, a placement overlaps a panel or a placement, or a panel with `repeat` and `direction` = `h` shares a row unit with another panel or a placement |
| `not_permitted` | The value is valid alone but not in this place |
| `uid_mismatch` | The `uid` differs from the location |
| `format_version` | The format version is not the version of this binary |
| `too_many_errors` | The list stopped at `MAX_ERRORS` |

Example: the grid of the third panel has `"w": 30` and an extra member `"heigth": 8`. The validation gives two errors:

```json
[
  {
    "pointer": "/sections/0/panels/2/grid/w",
    "code": "out_of_range",
    "message": "w must be 1 to 24; value 30",
    "line": null,
    "column": null
  },
  {
    "pointer": "/sections/0/panels/2/grid/heigth",
    "code": "unknown_member",
    "message": "member heigth is not permitted in grid; permitted members: x, y, w, h",
    "line": null,
    "column": null
  }
]
```

Section 6 gives the place of this list in the API error `invalid_document`.

### 4.38 Canonical form

The canonical form gives exactly one byte sequence for each document. Both backends store these bytes, and the etag is their hash. The rules are:

1. The bytes are UTF-8 without a byte order mark.
2. An object is `{`, a line end, each member on its own line, then `}` on its own line. A member is `"name": value`. A comma ends each member line except the last.
3. An array is `[`, a line end, each element on its own line, then `]` on its own line. A comma ends each element line except the last.
4. An empty object is `{}`. An empty array is `[]`.
5. Each line starts with two spaces for each level of depth. The top object has level 0.
6. The members come in the order of their table. A property set (4.18) writes only its members that are set. An optional member that has its default value is not written: `legend.sort_by` absent, `legend.sort_desc` false, `tooltip.sort` `none`, `tooltip.hide_zeros` false, `thresholds_style` `off`, `justify` `auto`, `wrap` false, `hide_time_override` false, `gradient_mode` `none`, `log_base` 10 and `bar_alignment` `center`. The member `span_nulls_ms` has no default value: the writer leaves it out when it is absent. The members `time_from` and `time_shift` have no default value: the writer leaves them out when they are absent.
7. A string escapes `"` as `\"` and `\` as `\\`. It escapes U+0008, U+000C, U+000A, U+000D and U+0009 as `\b`, `\f`, `\n`, `\r` and `\t`. It escapes the other characters from U+0000 to U+001F as `\u00xx` in lower-case hex. All other characters are their UTF-8 bytes: no other `\u` escape, and no escape of `/`.
8. An integer is its decimal digits with `-` when it is negative. It has no `+` and no `0` before another digit.
9. A number uses the form of RFC 8785, section 3.2.2.3: the shortest decimal text that gives the same double. An integral value has no fraction. The value `-0` is `0`.
10. The literals are `true`, `false` and `null`.
11. One U+000A follows the last `}`. No other byte follows it.

The member `placement` of a variable is the last member of the variable, and `title` is the last member of `placement`. The writer leaves them out when they are absent (4.7a).

The members of a variable of the kind `interval` come in the order `intervals`, `auto`, `auto_count`, `auto_min`, `default`, then `placement`. The members of the kind `constant` are `value` only. The writer keeps `auto_count` and `auto_min` when `auto` is `false`, and it keeps `default` when it is `null`.

The member `allow_custom_value` of a `query` or `custom` variable follows `default` and comes before `placement`. The writer leaves it out when it is `false` or absent, as it leaves out `hide_time_override` when it is `false`. A document that has `"allow_custom_value": false` is valid, and its canonical form has no such member.

The member `repeat` is the last member of a panel, after `links`, and the last member of a section, after `panels`. In a `repeat` of a panel, `max_per_row` is the last member. The writer leaves out `repeat` when it is absent, and `max_per_row` when it is absent. It does not write the default value 4 for an absent `max_per_row` (4.13a, rule 4).

The member `annotations` comes after `variables` and before `sections`. The writer leaves it out when the array is empty. The members of an annotation come in the order of 4.5b. The writer leaves out `enabled` when it is `true` and `panels` when it is absent. The array `panels` keeps its order. Each line of `sql` is one element of the array on its own line (rule 3).

The member `extra_queries` comes after `query` in `data`. The members `ref` and `hide` are the last members of `data.query`, in this order. The writer leaves out `ref` when its value is `A`. It leaves out `hide` when its value is `false`.

The members of an item of `extra_queries` come in the order `ref`, `source`, `format`, `sql`, `hide`. The writer always writes `ref` in an item, and it leaves out `hide` when it is `false`. The array keeps its order. Each line of `sql` is one element of the array on its own line (rule 3).

A panel of the type `text` has the members `id`, `type`, `title`, `description`, `transparent`, `grid`, `options` and `links`, in this order. The members `data`, `transforms` and `fields` are not in it. Its `options` have `mode`, then `content`. Each line of `content` is one element of the array on its own line (rule 3). An empty `content` is `[]`.

The members of `options` of a `state_timeline` and of a `status_history` come in the order of the tables of 4.33b. The members of the objects `legend` and `tooltip` come in the order of 4.26. The canonical form of a panel with these types has no member that 4.33b does not list.

The members of `options` of a `heatmap` and of a `histogram` come in the order of the tables of 4.33c and 4.33d. The members of `x_buckets`, `y_buckets` and `color` come in the order of their tables. The members of `legend` and `tooltip` come in the order of 4.26. The canonical form of a panel with these types has no member that 4.33c and 4.33d do not list.

The members of each transform come in the order of the tables of 4.25 and 4.25a, with `kind` first. In `calculate_field`, the object of the mode comes last. An operand writes `kind` first. The writer keeps an empty array and `null` as they are. The writer leaves out no member of these kinds, because each member is necessary.

These rules give the output of the pretty formatter of `serde_json` with an indent of two spaces, except rule 9. `hune-core` writes numbers with rule 9 from the digits and the exponent that `ryu` gives.

| Number in the input | Canonical text |
|---|---|
| `1.50` | `1.5` |
| `1e3` | `1000` |
| `-0` | `0` |
| `0.000001` | `0.000001` |
| `1e-7` | `1e-7` |
| `1e21` | `1e+21` |
| `123456789012345678901234567890` | `1.2345678901234568e+29` |

The reader accepts any valid JSON layout. A file in a volume can hold valid bytes that are not canonical, for example after a manual edit. The store then computes the canonical bytes for the etag and the API. The file stays as it is until the next write (5.17).

### 4.39 Etag

1. The etag of a document is the SHA-256 of its canonical bytes, in lower-case hex: 64 characters.
2. Both backends give the same etag for the same document.
3. A document that is not valid has no canonical form. For such a stored document, the store uses the SHA-256 of its raw bytes as its etag (5.17). The document is not served. A write can replace it with `If-Match` of this value.
4. A history version keeps the etag of its document (5.11, 5.24).
5. Section 6 gives the HTTP form of the etag in `ETag` and `If-Match`.

The document of 4.40 has 8,397 canonical bytes and the etag `50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501`.

### 4.40 Worked example

The dashboard `nodes` is in the folder `transit`. In a volume, the file is `<root>/transit/nodes.json`. The text below is its canonical form, byte for byte, followed by one line end.

```json
{
  "format_version": 3,
  "uid": "nodes",
  "title": "Nodes",
  "description": "Weight and status of the nodes.",
  "tags": [
    "chain"
  ],
  "time": {
    "from": "now-24h",
    "to": "now",
    "timezone": "browser",
    "refresh": "1m"
  },
  "cursor_sync": "crosshair",
  "links": [],
  "variables": [
    {
      "kind": "query",
      "name": "node",
      "label": "Node",
      "hide": "none",
      "source": "prod",
      "sql": [
        "SELECT DISTINCT node",
        "FROM analytics.nodes",
        "ORDER BY node"
      ],
      "multi": true,
      "include_all": true,
      "all_value": null,
      "refresh": "time_change",
      "sort": "alpha_asc",
      "regex": null,
      "default": [
        "$__all"
      ]
    },
    {
      "kind": "text",
      "name": "account",
      "label": "Account",
      "hide": "none",
      "value_type": "text",
      "default": ""
    },
    {
      "kind": "custom",
      "name": "env",
      "label": null,
      "hide": "none",
      "options": [
        {
          "text": "Production",
          "value": "prod"
        },
        {
          "text": "Preproduction",
          "value": "preprod"
        }
      ],
      "multi": false,
      "include_all": false,
      "all_value": null,
      "default": [
        "prod"
      ]
    }
  ],
  "sections": [
    {
      "title": null,
      "collapsed": false,
      "panels": [
        {
          "id": 1,
          "type": "timeseries",
          "title": "Weight over time",
          "description": "",
          "transparent": false,
          "grid": {
            "x": 0,
            "y": 0,
            "w": 16,
            "h": 8
          },
          "data": {
            "query": {
              "source": "prod",
              "format": "time_series",
              "sql": [
                "SELECT",
                "  $__timeInterval(ts) AS t,",
                "  node,",
                "  sum(weight) AS weight",
                "FROM analytics.node_weight",
                "WHERE $__timeFilter(ts)",
                "  AND node IN (${node})",
                "GROUP BY t, node",
                "ORDER BY t"
              ],
              "min_interval": "1m",
              "max_data_points": null
            }
          },
          "transforms": [],
          "fields": {
            "defaults": {
              "unit": "suffix: pts",
              "decimals": 2
            },
            "overrides": []
          },
          "options": {
            "draw": "line",
            "interpolation": "linear",
            "line_width": 1,
            "fill_opacity": 10,
            "show_points": "auto",
            "point_size": 5,
            "span_nulls": false,
            "stacking": "none",
            "y_scale": "linear",
            "legend": {
              "show": true,
              "placement": "bottom",
              "mode": "table",
              "values": [
                "mean",
                "last"
              ]
            },
            "tooltip": {
              "mode": "multi"
            }
          },
          "links": []
        },
        {
          "id": 3,
          "type": "stat",
          "title": "Total weight",
          "description": "",
          "transparent": false,
          "grid": {
            "x": 16,
            "y": 0,
            "w": 8,
            "h": 8
          },
          "data": {
            "reuse": {
              "panel": 2,
              "with_transforms": true
            }
          },
          "transforms": [],
          "fields": {
            "defaults": {
              "unit": "suffix: pts",
              "decimals": 0,
              "color": {
                "mode": "thresholds",
                "fixed": null
              },
              "thresholds": {
                "mode": "absolute",
                "steps": [
                  {
                    "value": null,
                    "color": "red"
                  },
                  {
                    "value": 1000000,
                    "color": "green"
                  }
                ]
              }
            },
            "overrides": []
          },
          "options": {
            "reduce": {
              "calc": "total",
              "fields": "/^weight$/",
              "values": false
            },
            "color_mode": "value",
            "graph_mode": "none",
            "text_mode": "auto",
            "orientation": "auto"
          },
          "links": []
        }
      ]
    },
    {
      "title": "Details",
      "collapsed": false,
      "panels": [
        {
          "id": 2,
          "type": "table",
          "title": "Nodes",
          "description": "One row for each node.",
          "transparent": false,
          "grid": {
            "x": 0,
            "y": 0,
            "w": 24,
            "h": 10
          },
          "data": {
            "query": {
              "source": "prod",
              "format": "table",
              "sql": [
                "SELECT node, name, weight, ratio, status",
                "FROM analytics.nodes",
                "WHERE ('${account}' = '' OR account = '${account}')",
                "  AND env = '${env}'",
                "  AND node IN (${node})"
              ],
              "min_interval": null,
              "max_data_points": null
            }
          },
          "transforms": [
            {
              "kind": "filter_by_value",
              "mode": "exclude",
              "match": "any",
              "filters": [
                {
                  "field": "status",
                  "matcher": {
                    "kind": "is_null"
                  }
                }
              ]
            }
          ],
          "fields": {
            "defaults": {
              "align": "auto",
              "filterable": false
            },
            "overrides": [
              {
                "match": {
                  "by": "name",
                  "value": "weight"
                },
                "set": {
                  "unit": "suffix: pts",
                  "decimals": 2,
                  "width": 140
                }
              },
              {
                "match": {
                  "by": "regex",
                  "value": "/^ratio/"
                },
                "set": {
                  "unit": "percent",
                  "thresholds": {
                    "mode": "absolute",
                    "steps": [
                      {
                        "value": null,
                        "color": "green"
                      },
                      {
                        "value": 10,
                        "color": "orange"
                      }
                    ]
                  },
                  "cell": {
                    "type": "color_background",
                    "mode": "basic"
                  }
                }
              },
              {
                "match": {
                  "by": "name",
                  "value": "status"
                },
                "set": {
                  "mappings": [
                    {
                      "kind": "value",
                      "value": "1",
                      "text": "Active",
                      "color": "green"
                    },
                    {
                      "kind": "value",
                      "value": "0",
                      "text": "Offline",
                      "color": "red"
                    }
                  ]
                }
              },
              {
                "match": {
                  "by": "name",
                  "value": "node"
                },
                "set": {
                  "links": [
                    {
                      "title": "Open in the explorer",
                      "url": "https://nodes.example.org/id/${__value.raw}",
                      "target_blank": true
                    }
                  ],
                  "filterable": true
                }
              }
            ]
          },
          "options": {
            "show_header": true,
            "sort": [
              {
                "field": "weight",
                "desc": true
              }
            ]
          },
          "links": []
        }
      ]
    }
  ]
}
```

What the example shows:

1. Three variable kinds. The query variable is multi-value with "All" and loads again after each change of the time range.
2. Two sections. The first section has no header. The second section has the header "Details".
3. Panel 1 runs its own time series query with two macros and a multi-value variable in `IN (...)`.
4. Panel 2 runs a table query. The text variable and the custom variable are inside string literals, and section 7 escapes them as literal content.
5. Panel 3 reuses the frame of panel 2 with its transforms. Panel 3 is in another section, and it comes before panel 2 in the document.
6. Panel 2 has four overrides: three `byName` and one `byRegexp`. They set a unit, decimals and a width. They also set thresholds, a cell type, mappings, a data link and the column filter.
7. The stat panel takes its colour from the thresholds. A total below 1,000,000 pts is red.
8. The `byRegexp` override `/^ratio/` matches each field whose name starts with `ratio`. The stat field pattern `/^weight$/` matches the field `weight` only (4.24).
9. The document has `format_version` 3 and the member `links` with an empty array, after `cursor_sync` (4.5). Each of the three panels has the member `transparent` with the value `false`, after `description` (4.13).

The file `<root>/transit/_folder.json` of the same folder holds these 53 bytes (with the final line end):

```json
{
  "format_version": 1,
  "title": "Transit desk"
}
```

### 4.41 Folder file

A folder has one folder file. In a volume, it is `<root>/<folder id>/_folder.json`. In PostgreSQL, the row of `hune_folders` holds the same data (5.24).

| Member | Type | Rule |
|---|---|---|
| `format_version` | integer | `1` |
| `title` | text | 1 to 256 characters: the title that the list shows |

1. The folder file is closed and uses the canonical form of 4.38.
2. The folder id is the directory name in a volume and the column `id` in PostgreSQL. The file does not repeat it.
3. A folder directory without a valid folder file is an invalid folder (5.10).
4. `hune export` writes the canonical folder file from the row of PostgreSQL (5.38). Thus a volume with canonical files gives the same bytes after a round trip through PostgreSQL.
5. The folder file has its own `format_version`. It stays 1 when the document format changes (4.42, item 11).

### 4.42 Change of `format_version`

1. The binary reads only its own format version. In this version, it is 3.
2. A stored document of another version has one error `format_version`, with the message `not loadable: format N`. The list shows it, and the server does not serve it. The other documents stay served.
3. A change of the document schema raises `format_version` by 1. A new necessary member, or a new rule that makes a valid document invalid, is a change. An optional member, and a new value of an enum, join without a change of the version. A binary that does not know such a member refuses the document with `unknown_member`. A binary that does not know such a value refuses it with `bad_format`. Both refusals are safe.
4. A change comes with a pure migration function from N to N+1 in `hune-core`. It also comes with golden fixtures (input N, output N+1) and with the new tables in this section.
5. To upgrade, stop all `serve` processes. Fix or remove each document of format 1 or 2 that is not valid (item 8). Then run `hune migrate-documents -c <file>`.
6. Without `--write`, the command is a dry run. It prints each uid, its format version and the result of the migration, and writes nothing. It also prints one warning for each document that it does not migrate (item 8).
7. Then run `hune migrate-documents -c <file> --write`. Then start the new binary.
8. With `--write`, the command first migrates and validates in memory each document of an older format that it can migrate (1 or 2). When one such document fails, it prints the errors, writes nothing and exits with code 1. Only a document of such an older format can stop the command, also one that was not valid before the upgrade. A document that is not valid for another reason does not stop it. An example is a document with a syntax fault, or with a format version that the command cannot migrate. The command prints a warning for such a document and leaves it as it is.
9. Then it saves each migrated document with `Store::save` and the kind `migrate`. The call has `If-Match` equal to the current etag, `saved_by` equal to `-` and the message `format N to M`. Thus each document gets one history version. A document that needs two steps gets one version with the message `format 1 to 3`.
10. Older history versions keep their bytes and their format. A read of such a version gives `422 stored_document_invalid` (6.17), and `download=true` gives its bytes. The history view shows it as text (section 10). `Store::restore` refuses it with `NotRestorable` (5.6).
11. The folder file and the history metadata file (5.11) have their own `format_version`. It changes only when their schema changes. Then the same rule applies, and `migrate-documents` rewrites them. The changes of the document format to 2 and to 3 do not change them: they stay at 1.
12. A binary never rewrites a document by itself, and a read never writes.
13. On a volume, `migrate-documents --write` takes the lock of the volume (5.16).

The command exits with code 0 when the run ends without an error. This includes a run in which no document needs a migration. It exits with code 1 after any error.

Format 3 is the second change, and the table below gives its parts. The binary of format 3 keeps the migration from format 1 to format 2 and adds the migration from format 2 to format 3. `migrate-documents` applies them in sequence. A document of format 1 thus becomes a document of format 3 in one run.

| Part | Format 2 | Format 3 |
|---|---|---|
| `format_version` of a document | `2` | `3` |
| Member `links` of the top-level object (4.5, 4.5a) | Not permitted: a document that has it gives `unknown_member`. | Necessary, an array of 0 to 16 dashboard links, after `cursor_sync`. |
| Members `time_from`, `time_shift` and `hide_time_override` of `data.query` (4.15) | Not permitted: `unknown_member`. | Permitted and optional. |
| Members of a property set (4.18): `axis_placement`, `axis_label`, `soft_min`, `soft_max`, `draw`, `line_width`, `fill_opacity`, `line_dash`, `show_points`, `point_size` and `transform` | Not permitted: `unknown_member`. | Permitted and optional. |
| Value of `time.timezone` (4.6) | `browser` or `utc`. A zone name gives `bad_format`. | Also an IANA zone name. |
| Value of `color.mode` (4.19) | `fixed`, `thresholds` or `palette`. Another value gives `bad_format`. | Also the 10 continuous modes. |
| Migration | `migrate_1_to_2` (the first change) | `migrate_2_to_3` in `hune-core` (below). |
| Message of the saved version | `format 1 to 2` | `format 2 to 3`, kind `migrate`, `saved_by` equal to `-` |
| Folder file and history metadata file | `format_version` 1 | No change: `format_version` 1 |
| Golden fixtures | `tests/fixtures/dashboards/format2/` holds the inputs. | `tests/fixtures/dashboards/` holds the outputs. The vectors of the migration join them. |

The function `migrate_2_to_3` is pure: it does no I/O, and it reads no clock and no random value. Its steps are:

1. Check the bytes and parse the JSON (4.36, steps 1 to 3). Read `format_version`. The value MUST be the integer 2. Another value gives one error `format_version` and stops the migration.
2. Set `format_version` to 3.
3. Add the member `links` with the value `[]` to the top-level object. The canonical form puts it after `cursor_sync` (4.38). A document that already has `links` gives `unknown_member` at `/links`, because format 2 has no such member. The migration then writes nothing.
4. Change nothing else. The migration adds no optional member and no panel member. The enum values and the units do not change. A document of format 2 has none of the new members, and the migration does not check for them.
5. Run the validation of format 3 (4.36, steps 4 to 8) on the result. The result holds the canonical bytes and the etag (4.39).

The steps of a migration put their errors into one list with the cap of 4.36, step 7. The list has `MAX_ERRORS` items at most. When the list stops, its last item is `too_many_errors`. An example is a document of format 1 with many panels that have `transparent`. It gives a short list, and `migrate-documents` prints a short list.

Each default of the migration is in step 3: `links` is `[]`. The rules of format 3 only add members and values to the rules of format 2. Thus a document that is valid in format 2 is valid after the migration. A document that is not valid in format 2 gives the errors of format 3 at the same pointers.

The properties of the migration:

- The migration is a function of the bytes. The same bytes give the same canonical bytes, also when the layout of the input is not canonical.
- The result passes `validate` and is equal to its own canonical form. Thus `validate` of the result gives the same bytes and the same etag.
- A second call on the result gives the error `format_version` ("not loadable: format 3"), so the migration never changes a document of format 3. `migrate-documents` does not call the function for such a document. It prints "format 3, no migration needed".
- A document that the migration changes gets exactly one more line (`"links": [],`) and the new version number. The worked example of 4.40 shows this change: its bytes grow by 15.

A binary of format 2 refuses a document of format 3 at step 4 of 4.36, with the error `format_version` ("not loadable: format 3"). It never reaches the member checks, so it never serves such a document. A person can edit a document of format 2 by hand and add a member of format 3. A binary of format 2 refuses it with `unknown_member`. A binary of format 3 refuses it with `format_version`. Both refusals are safe.

The optional member `placement` of a variable (4.7a) joins format 3 with no change of the version. No object gets a necessary member, and no document that was valid becomes invalid. Thus the rule 3 of this section does not ask for a new version.

- A document with no `placement` keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the member refuses a document that has it, with `unknown_member` at `/variables/<i>/placement`.
- The refusal is safe. That binary lists the stored document as not valid and never serves it (5.17).
- The import of Grafana never makes a `placement`. It stays lossless for all that it maps today (section 15).

The optional member `repeat` of a panel and of a section (4.13a) joins format 3 with no new version. No object gets a necessary member, and no valid document becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no `repeat` keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the member refuses a document that has it, with `unknown_member` at `/sections/<i>/repeat` or at `/sections/<i>/panels/<j>/repeat`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- A new panel type is not necessary. The copies are not stored, so no new kind of object exists.
- The import of Grafana makes a `repeat` for the cases of 15.6. A document that the import makes with no repeat has the same bytes as before.
- The server of this version reads and writes the member. It runs no code for it at query time (4.13a, rule 10).

The optional member `annotations` of the top-level object (4.5, 4.5b) joins format 3 with no new version. No object gets a necessary member, and no document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no annotation keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the member refuses a document that has it. It gives `unknown_member` at `/annotations`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of an annotation are necessary in the new object only. No older object gets a necessary member.
- The import of Grafana makes annotations for the cases of 15.6. A document that the import makes with no annotation has the same bytes as before.
- The server of this version reads and writes the member. It runs the saved query of an annotation only through the route of 6.21a.

Three optional members join format 3 with no new version. They are `extra_queries` of `data`, and `ref` and `hide` of a query (4.15a). The value `query` of the member `by` of a matcher (4.17) joins it too. No object gets a necessary member, and no document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no extra query keeps its bytes. Its `data.query` has no `ref` and no `hide`. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the members refuses a document that has them. It gives `unknown_member` at `.../data/extra_queries`, at `.../data/query/ref` or at `.../data/query/hide`. It gives `bad_format` at `.../match/by` for the matcher `query`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of an item of `extra_queries` are necessary in the new object only. No older object gets a necessary member.
- The import of Grafana makes these members for the cases of 15.6. A document that the import makes with one query per panel has the same bytes as before.
- The server of this version reads and writes the members. It runs one query for each request of 6.20.

The value `text` of the member `type` of a panel (4.13, 4.33a) joins format 3 with no new version. A new value of an enum is not a change of the format, and no document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no text panel keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the value refuses a document that has a text panel. It gives `bad_format` at `/sections/<i>/panels/<j>/type`. It can also give `missing_member` for `data`, `transforms` and `fields` of that panel, because its table needs them. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members `options.mode` and `options.content` are necessary in the new type only. No older object gets a necessary member.
- The import of Grafana makes a text panel for the cases of 15.6. A document that the import makes with no text panel has the same bytes as before.
- The server of this version reads and writes the type. It runs no query for a text panel (6.20).

The values `state_timeline` and `status_history` of the member `type` (4.13, 4.33b) join format 3 with no new version. A new value of an enum is not a change of the format. No document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no such panel keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the values refuses a document that has such a panel. It gives `bad_format` at `/sections/<i>/panels/<j>/type`. The panel has `data`, `transforms` and `fields`, so no other error comes from the shared members. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of `options` are necessary in the new types only. No older object gets a necessary member.
- The import of Grafana makes these panels for the cases of 15.6. A document that the import makes with no such panel has the same bytes as before.
- The server of this version reads and writes the types. It runs the ordinary panel query for them (6.20).

The values `heatmap` and `histogram` of the member `type` (4.13, 4.33c, 4.33d) join format 3 with no new version. A new value of an enum is not a change of the format. No document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no such panel keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the values refuses a document that has such a panel. It gives `bad_format` at `/sections/<i>/panels/<j>/type`. The panel has `data`, `transforms` and `fields`, so no other error comes from the shared members. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of `options` are necessary in the new types only. No older object gets a necessary member.
- The import of Grafana makes these panels for the cases of 15.6. A document that the import makes with no such panel has the same bytes as before.
- The server of this version reads and writes the types. It runs the ordinary panel query for them (6.20).

Two values of the member `kind` of a variable join format 3 with no new version. They are `interval` and `constant`. No object gets a new necessary member, and no document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no such variable keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know a kind refuses a document that has it. It gives `bad_format` at `/variables/<i>/kind`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of the new kinds are necessary in the new objects only. No older object gets a necessary member.
- The import of Grafana makes these kinds for the cases of 15.6. A document that the import makes with no such variable has the same bytes as before.
- The server reads and writes the members. It computes the text of "Auto" at query time (7.4a).

Eleven values of the member `kind` of a transform join format 3 with no new version. They are `organize`, `calculate_field`, `sort_by`, `limit`, `merge`, `reduce`, `partition_by_values`, `join_by_field`, `filter_fields_by_name`, `rename_by_regex` and `convert_field_type`. No object gets a new necessary member, and no document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no new kind keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know a kind refuses a document that has it. It gives `bad_format` at `/sections/<i>/panels/<j>/transforms/<k>/kind`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The members of the new kinds are necessary in the new objects only. No older object gets a necessary member.
- The import of Grafana makes these kinds for the cases of 15.6. A document that the import makes with no new kind has the same bytes as before.
- The server reads and writes the members. It runs no code for them at query time (9.21a).

Two values `logs` join format 3 with no new version. They are a value of the member `type` (4.13, 4.33f) and a value of the member `format` of a query (4.15). A new value of an enum is not a change of the format. Each member of the options of the new type is optional. No document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no logs panel keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the values refuses a document that has them. It gives `bad_format` at `/sections/<i>/panels/<j>/type` or at the pointer of `format`. The panel has `data`, `transforms` and `fields`, so no other error comes from the shared members. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The import of Grafana makes these values for the cases of 15.6. A document that the import makes with no logs panel has the same bytes as before.
- The server of this version reads and writes the values. It runs a query of the format `logs` as a query of the format `table` (7.16e).

The value `geomap` of the member `type` (4.13, 4.33g) joins format 3 with no new version. A new value of an enum is not a change of the format. No document that was valid becomes invalid. A document with no map keeps its bytes, and `migrate-documents` does nothing for it. The import of Grafana makes the value for a `geomap` of 15.6. Rule 3 of this section does not ask for a new version.

The versions of the history that are in format 2 keep their bytes. After the migration, they follow item 10: a read gives `422 stored_document_invalid`, `download=true` gives the bytes, and a restore gives `NotRestorable`.

The data source variable and the built-in Grafana annotations are not in format 3. The ad hoc variable is the new value `adhoc` of `kind` (4.10c). Three parts join format 3 with no new version. The first is the repeat, with optional members (4.13a). The second is the panel type `text`, a new value of `type` (4.33a). The third is the variable kinds `interval` and `constant`, new values of `kind` (4.10a, 4.10b).

A necessary member of a later feature needs format 4. An optional member joins format 3. The annotations are an optional member of format 3 (4.5b).

The several queries of a panel are optional members of format 3 (4.15a).

The optional member `allow_custom_value` of a `query` and of a `custom` variable (4.8, 4.9) joins format 3 with no new version. This follows rule 3 of 4.42. No object gets a necessary member. No document that was valid becomes invalid. Rule 3 of this section does not ask for a new version.

- A document with no such member keeps its bytes. No migration is necessary, and `migrate-documents` does nothing for it.
- A binary that does not know the member refuses a document that has it. It gives `unknown_member` at `/variables/<i>/allow_custom_value`. The refusal is safe: that binary lists the document as not valid and never serves it (5.17).
- The import of Grafana makes the member for the cases of 15.6. A document that the import makes for a closed variable has the same bytes as before.
- The server of this version reads and writes the member. For a `custom` variable it changes the check of 7.7. For a `query` variable it changes nothing on the server.

## 5. Storage

### 5.1 Two backends, one contract

Hune has two storage backends: the volume backend (use case LOCAL) and the PostgreSQL backend (use case DISTRIBUTED). A file names one store or more with the blocks `storage.volume "<id>"` and `storage.postgres` (5.42, section 13).

1. Both backends implement the one `Store` trait of 5.3, with the semantics of 5.4 and 5.5.
2. Both backends store the canonical bytes of 4.38 and give the same etags (4.39).
3. One conformance suite (5.33 to 5.36) runs on both backends.
4. The catalog (5.8) sits above the trait. The server reads dashboards from the catalog.
5. The enum `AnyStore { Volume(VolumeStore), Postgres(PgStore) }` dispatches the calls (3.4, R8).
6. `hune copy-store`, `hune export` and `hune import` move a store between the two kinds (5.37 to 5.39).
7. The type `Stores` holds the stores of one process behind the same trait: one primary store and the read-only stores (5.42 to 5.44).

### 5.2 Types

| Type | Definition |
|---|---|
| `Uid`, `FolderId` | A checked id (4.3) |
| `FolderTitle` | A text of 1 to 256 characters |
| `Etag` | 32 bytes. Text form: 64 lower-case hex characters. |
| `SaveId` | A UUID (16 bytes). Text form: `8-4-4-4-12` lower-case hex. |
| `CanonicalDocument` | The canonical bytes, the etag, the uid, the title, the format version and the typed document. Only the validation of 4.36 makes it. |
| `Subject` | A text of 1 to 256 characters: the ProcurAuth subject, or one of the reserved texts `-` (mode `open` and the command line), `external` (5.5, rule 7) and `import-grafana` (section 15). Section 11 refuses a token whose subject is one of the three reserved texts. |
| `Message` | A text of 0 to 500 characters |
| `UtcMillis` | A UTC time with millisecond precision. Text form: `YYYY-MM-DDTHH:MM:SS.mmmZ`. |
| `WriteMeta` | `saved_by: Subject`, `saved_at: UtcMillis`, `message: Message`, `save_id: Option<SaveId>` |
| `VersionKind` | One value of the table below |
| `VersionMeta` | uid, `version` (integer from 1), kind, folder, etag (absent for `delete`), `saved_at`, `saved_by`, message, `save_id`, size in bytes (absent for `delete` and for an `external` version without bytes) |
| `Version` | A `VersionMeta` and the bytes of its document (absent for `delete` and for an `external` version without bytes) |
| `Current` | uid, folder, version, etag, canonical bytes, `saved_at` and `saved_by` of the current version. When the current etag differs from the etag of the head version (an outside change), `version` is the head number, `saved_by` is `external` and `saved_at` is the modification time of the file (volume) or `updated_at` of the row (PostgreSQL). Without a head version (a file from git, or a read-only volume), `version` is 0 with the same `saved_by` and `saved_at`. |
| `Saved` | uid, folder, version, etag (absent after a delete), outcome: `written`, `unchanged` or `replayed` |
| `CatalogEntry` | Valid: a `Current` and the typed document. Invalid: uid, folder, etag of the raw bytes (4.39), list of errors (4.37). |

| Version kind | Written by |
|---|---|
| `create` | `Store::create` from the API: a new dashboard, or the import of one document in the editor |
| `save` | `Store::save` from the API |
| `restore` | `Store::restore`, from the API or from `hune restore` |
| `move` | `Store::move_to` |
| `delete` | `Store::delete`. A tombstone: it has no document and no etag. |
| `import` | `hune import-grafana --write` (section 15) |
| `external` | The store, when the current document differs from the head version (5.5, rule 7). It holds the canonical bytes of a valid current document. For a current document that is not valid, it holds no bytes, and its etag is the SHA-256 of the raw bytes. |
| `migrate` | `hune migrate-documents --write` (4.42) |

`Store::create` accepts the kinds `create` and `import`. `Store::save` accepts the kinds `save`, `import` and `migrate`. `hune import` and `hune copy-store` copy each version with its own kind.

The time `saved_at` comes from the clock of the instance that writes, cut to the millisecond. The version number, not the time, gives the order of the versions.

### 5.3 The `Store` trait

```rust
// hune-store (shape of the contract). Each method is async.
pub trait Store: Send + Sync {
    async fn load(&self) -> Result<Snapshot, StoreError>;
    async fn poll(&self, marks: &Marks) -> Result<Delta, StoreError>;
    async fn get(&self, uid: &Uid) -> Result<Current, StoreError>;
    async fn create(&self, folder: &FolderId, doc: &CanonicalDocument,
                    kind: VersionKind, meta: &WriteMeta) -> Result<Saved, StoreError>;
    async fn save(&self, doc: &CanonicalDocument, if_match: &Etag, expected_folder: &FolderId,
                  kind: VersionKind, meta: &WriteMeta) -> Result<Saved, StoreError>;
    async fn move_to(&self, uid: &Uid, folder: &FolderId, if_match: &Etag,
                     expected_folder: &FolderId, meta: &WriteMeta) -> Result<Saved, StoreError>;
    async fn delete(&self, uid: &Uid, if_match: &Etag, expected_folder: &FolderId,
                    meta: &WriteMeta) -> Result<Saved, StoreError>;
    async fn restore(&self, uid: &Uid, version: u64, if_match: Option<&Etag>,
                     expected_folder: Option<&FolderId>, meta: &WriteMeta) -> Result<Saved, StoreError>;
    async fn history(&self, uid: &Uid, before: Option<u64>, limit: u32)
        -> Result<Vec<VersionMeta>, StoreError>;
    async fn version(&self, uid: &Uid, version: u64) -> Result<Version, StoreError>;
    async fn create_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError>;
    async fn rename_folder(&self, id: &FolderId, title: &FolderTitle) -> Result<(), StoreError>;
    async fn delete_folder(&self, id: &FolderId) -> Result<(), StoreError>;
    async fn export_begin(&self) -> Result<Exporter, StoreError>;
    async fn import_begin(&self) -> Result<Importer, StoreError>;
}

// Exporter: folders() -> Vec<FolderRecord>; uids() -> Vec<Uid>;
//           dashboard(&Uid) -> DashboardRecord; end().
// Importer: folder(FolderRecord); dashboard(DashboardRecord); commit() -> ImportReport.
// FolderRecord: id, title. DashboardRecord: uid, all versions in ascending order
// (VersionMeta and bytes), and the current document with its folder, or none after a delete.
```

`Snapshot` holds all folders and all catalog entries. `Marks` holds one item for each folder and each uid of the catalog. An item has the folder, the etag and the file signature of 5.17 (volume only). `Delta` holds the changed and the removed folders and entries.

### 5.4 Operations

| Operation | Input | Output | Errors |
|---|---|---|---|
| `load` | none | All folders and all catalog entries, valid and invalid | `Unavailable` |
| `poll` | The marks of the catalog | The changes since the marks | `Unavailable` |
| `get` | uid | The current document, read from the backend | `NotFound`, `Invalid`, `Unavailable` |
| `create` | folder, document, kind, meta | `Saved` with version 1 | `FolderNotFound`, `Exists`, `SaveIdReused`, `Full`, `ReadOnly`, `Unavailable` |
| `save` | document (its uid names the dashboard), If-Match, expected folder, kind, meta | `Saved` | `NotFound`, `Conflict`, `SaveIdReused`, `Full`, `ReadOnly`, `Unavailable` |
| `move_to` | uid, target folder, If-Match, expected folder, meta | `Saved` with the same etag and a version of kind `move` | `NotFound`, `FolderNotFound`, `Exists`, `Invalid`, `Conflict`, `SaveIdReused`, `Full`, `ReadOnly`, `Unavailable` |
| `delete` | uid, If-Match, expected folder, meta | `Saved` without etag, after a tombstone version | `NotFound`, `Conflict`, `SaveIdReused`, `Full`, `ReadOnly`, `Unavailable` |
| `restore` | uid, version number, If-Match, expected folder, meta | `Saved` with the etag of the restored version | `NotFound`, `VersionNotFound`, `NotRestorable`, `FolderNotFound`, `Conflict`, `SaveIdReused`, `Full`, `ReadOnly`, `Unavailable` |
| `history` | uid, `before` (optional, exclusive), `limit` (1 to 100) | Version metadata, newest first | `NotFound`, `Unavailable` |
| `version` | uid, version number | The version with its bytes. A tombstone and an `external` version without bytes have no bytes. | `VersionNotFound`, `Unavailable` |
| `create_folder` | folder id, title | none | `Exists`, `Full`, `ReadOnly`, `Unavailable` |
| `rename_folder` | folder id, title | none | `FolderNotFound`, `ReadOnly`, `Unavailable` |
| `delete_folder` | folder id | none | `FolderNotFound`, `FolderNotEmpty`, `ReadOnly`, `Unavailable` |
| `export_begin` and the methods of `Exporter` | none | A consistent copy of all folders, uids and versions | `Invalid`, `Unavailable` |
| `import_begin` and the methods of `Importer` | Records | A report with the counts | `NotEmpty`, `Invalid`, `Full`, `ReadOnly`, `Unavailable` |

1. `create` gives `Exists` when the uid has a current dashboard or a version. Thus only `restore` (5.40) restores a deleted uid.
2. `history` gives `NotFound` when the uid has no current dashboard and no version. A uid with a current dashboard and no version gives an empty list. `history` lists the versions of a deleted dashboard.
3. `restore` of a dashboard that exists needs `if_match` equal to its etag, else `Conflict`. `restore` of a deleted dashboard needs `if_match` absent, else `NotFound`. A deleted dashboard returns in the folder of the restored version.
4. `restore` gives `VersionNotFound` when the version does not exist or was removed by the retention. It gives `NotRestorable` when the version is a tombstone or an `external` version without bytes. It also gives `NotRestorable` when the bytes are not valid in the current format (4.42).
5. `rename_folder` with the same title succeeds and changes nothing.
6. `Exporter::uids` lists each uid that has a current document or a version, in byte order.
7. `move_to` gives `Invalid` when the current document is not valid, because a version of kind `move` holds the document. It gives `Exists` when the target folder holds a file with the same uid (5.15).
8. `restore` reads the version and validates it before it takes a lock of the store. The extension method `restore_valid` takes a version that its caller read with `version` and validated (`ValidVersion`). Under its lock, it reads the metadata of the version again and gives the errors of item 4. It never parses or validates the document. The server uses it, so a restore of the API validates its version one time (6.18).

### 5.5 Concurrency semantics

These rules apply to both backends. The conformance suite checks each rule.

1. The writes on one uid are serialised. Each write sees the result of the write before it.
2. A write with `if_match` equal to the current etag applies. A write with another `if_match` returns `Conflict` with the current etag and changes nothing. A write on a dashboard that exists also carries `expected_folder`: the folder of the chain that the PDP saw (11.17). When the current folder of the uid is another folder, the write returns `Conflict` and changes nothing. The etag cannot show a move, because the document does not hold its folder. A `restore` of a deleted dashboard has no `expected_folder`.
3. Before the check of rule 2, a write with a `save_id` looks for this `save_id` in the versions of the uid. When it finds a version of the same request that is applied (rule 14), it returns `Saved` for that version with the outcome `replayed`. When it finds a version of another request, it returns `SaveIdReused` and changes nothing. A version of the same request that is not applied follows rule 14.
4. Two requests are the same request when the version has the kind of the operation and the same effect (section 6). For `create`, `save` and `restore`, the effect is the new etag. For `move_to`, it is the target folder. A `delete` has no other effect.
5. A `save` or a `restore` whose new etag equals the current etag returns the outcome `unchanged` and writes no version. A `move_to` to the current folder also returns `unchanged`.
6. When N writes with the same `if_match` run at the same time, exactly one returns `written`. The others return `Conflict`, or `replayed` when they carry the `save_id` of the first.
7. Before a write, the store compares the etag of the head version with the current etag. When they differ, it first writes a version of kind `external`. This version holds the canonical bytes of the current document, or no bytes when the current document is not valid (5.2). A change outside Hune (`git pull`) or an earlier crash causes this case.
8. The history version is durable before the effect is visible. A crash between the two leaves at most one version that was never current.
9. The version numbers of one uid start at 1 and increase by 1 for each version. A number is never used two times. This is also true after the retention, a delete and a conflict at step 12 of 5.14.
10. `get`, `history` and `version` read the backend, not the catalog.
11. A store never sends a write again by itself after an error that hides the outcome. It returns `Unavailable`. The caller sends the write again with the same `save_id` (rule 3).
12. A folder operation is atomic. `delete_folder` fails when a current dashboard is in the folder. The history does not count.
13. `create` and `create_folder` check the limits of 5.7 under the same exclusion as the write.
14. A version v is applied when its effect is visible:
    - For `create`, `save`, `restore`, `import` and `migrate`, the current document has the etag of v. For `move`, the current document is in the folder of v. For `delete`, the uid has no current document.
    - In PostgreSQL, one transaction holds the version and its effect, so each version is applied.
    - On a volume, a failure or a crash after steps 9 and 10 of 5.14 can leave a head version v that is not applied. Then a request with the `save_id` of v and the same request (rule 4) rolls forward under the lock.
    - To roll forward, the backend applies the effect of v and returns `written`. It does this only when the current state is still the state before v. For a `create`, no current document exists. For the other kinds, the current etag equals the etag of version v - 1 or the `if_match` of the request.
    - In the other case, the backend returns `Conflict` with the current etag.
    - A version that the volume backend keeps but never makes current has `save_id` null in its metadata file (5.14, step 5 and step 12; 5.15). Thus a retry never finds it, and it takes the normal path.
15. A retry finds its `save_id` while the retention keeps its version (5.18). The retention keeps the newest `history_keep` versions of the uid. Thus a retry is safe until `history_keep` later versions of the same uid exist. After that, the retry gets `Conflict` (rule 2).

### 5.6 Errors

`StoreError` has the variants of the table. Section 6 owns the table of API codes. The store errors map to these statuses and codes:

| Variant | Meaning | HTTP status and code | Retryable |
|---|---|---|---|
| `NotFound` | No current dashboard with this uid | 404 `not_found` | no |
| `FolderNotFound` | No valid folder with this id | 404 `not_found` | no |
| `VersionNotFound` | No version at this number | 404 `not_found` | no |
| `Exists` | The uid (current or with versions) or the folder id exists. For a uid, it carries the folder of the current dashboard, or of the head version of a deleted dashboard. | 409 `already_exists` | no |
| `Conflict` | `if_match` differs from the current etag, or the folder differs from `expected_folder`. It carries the `Current` of the dashboard (etag, version, folder, `saved_at`, `saved_by`). | 409 `conflict` | no |
| `FolderNotEmpty` | The folder holds a dashboard or another entry | 409 `folder_not_empty` | no |
| `ReadOnly` | The store is read-only: it is not the primary store (5.42), or a command opened a volume read-only | 403 `read_only_store` | no |
| `Invalid` | A stored document is not valid in the current format, for a `get` or a `move_to`. It carries the errors of 4.37. | 422 `stored_document_invalid` | no |
| `NotRestorable` | The version is a tombstone or an `external` version without bytes, or its bytes are not valid in the current format | 422 `not_restorable` | no |
| `SaveIdReused` | The `save_id` belongs to a version of another request (5.5, rule 4) | 422 `idempotency_key_reused` | no |
| `Full` | No space (`ENOSPC`, `EDQUOT`), or a limit of 5.7 | 507 `storage_full` | no |
| `Unavailable` | The backend does not answer, or an I/O error | 503 `storage_unavailable` | yes |
| `NotEmpty` | The target of an import is not empty | command line only (exit code 1) | no |

### 5.7 Limits of a store

| Constant | Value |
|---|---|
| `MAX_DASHBOARDS` | 10,000 current dashboards in one store |
| `MAX_FOLDERS` | 1,000 folders in one store |
| `MAX_MESSAGE_CHARS` | 500 characters in one history message |
| `MAX_SUBJECT_CHARS` | 256 characters in `saved_by` |
| `MAX_HISTORY_PAGE` | 100 versions in one answer of `history` |
| Retention | `history_keep` versions for each uid (5.18, 5.27) |

A `create` above `MAX_DASHBOARDS` or a `create_folder` above `MAX_FOLDERS` returns `Full`. These limits bound the memory of the catalog.

### 5.8 The catalog

The catalog is the copy of the store in the memory of each instance. It is in `hune-store`, above the trait. The server reads the list, the documents and the SQL of the panels from it.

1. At start, the catalog calls `Store::load` one time. When PostgreSQL does not answer, it tries again after each `storage.postgres.connect_timeout` (5.29). `/readyz` answers 200 after the first load (section 14).
2. Then the catalog calls `Store::poll` with its marks and applies the changes. Each store has its own period: `rescan_interval` of its volume block, or `storage.postgres.poll_interval` (5.42, rule 9).
3. After a write of this instance, the catalog applies the result at once. A save is visible to the same instance before the answer goes to the client.
4. A valid entry holds the canonical bytes, the typed document and the lexed SQL of each panel and variable (section 7).
5. An invalid entry holds its errors. The list of dashboards shows it with its errors, and the server does not serve it. Section 6 gives the answers of the API for an invalid entry. Only the PostgreSQL store gives an invalid entry: a volume stops the start (5.43) or keeps the last valid state (5.44).
6. When a poll fails, the catalog keeps its content and serves it. The metric `hune_store_up` goes to 0, and the log line `store.sync_failed` occurs one time. The next poll tries again. The first poll that succeeds writes `store.sync_recovered` and sets `hune_store_up` to 1.
7. A change between two polls is visible after one poll period, plus the time of the poll.

### 5.9 Volume: layout

```text
<root>/                                path of the volume block: absolute and canonical
  .hune/                              reserved; made by a writer; mode 0750
    lock                               flock target; holds "<host name> <pid> <UtcMillis>" of the holder
  .history/                            reserved; mode 0750
    <uid>/                             the versions of one uid; mode 0750
      <n>.json                         the document bytes of version n (absent for a delete)
      <n>.meta.json                    the metadata of version n (5.11); written last
  <folder id>/                         one folder; mode 0750
    _folder.json                       the folder file (4.41)
    <uid>.json                         one dashboard: its document; mode 0640
    .<name>.tmp-<16 hex>               a temporary file during a write
```

1. A dashboard file is `<root>/<folder id>/<uid>.json`. The file stem is the uid of the document.
2. The number `<n>` is a decimal integer from 1, without a `0` before the first other digit.
3. A temporary file has the name `.<final name>.tmp-<16 hex>`, with 64 random bits in lower-case hex. It is in the directory of its final file.
4. The underscore of `_folder.json` cannot start a uid, so no name collides.

### 5.10 Volume: entries of the scan

| Place | Entry | Action |
|---|---|---|
| root | A directory whose name fits the folder id rule | A folder |
| root | `.hune` | Reserved. It MUST be a directory. The scan does not read it. |
| root | `.history` | Reserved. It MUST be a directory. The scan does not read it. |
| root | Another name that starts with `.` (for example `.git`) | Ignored, without a log line |
| root | Another entry: a file, a symbolic link, a directory with a wrong name | Ignored, with the log line `store.entry_ignored` |
| folder | `_folder.json`, a regular file | The folder file (4.41) |
| folder | `<uid>.json`, a regular file whose stem fits the uid rule | A dashboard |
| folder | A name that starts with `.` | Ignored, without a log line (temporary files, files of an editor) |
| folder | Another entry | Ignored, with the log line `store.entry_ignored` |
| `.history` | A directory whose name fits the uid rule | The versions of that uid |
| `.history/<uid>` | `<n>.json` and `<n>.meta.json` | The files of version n |
| `.history/<uid>` | A name that starts with `.` | Ignored, without a log line |
| `.history/<uid>` | Another entry | Ignored, with the log line `store.entry_ignored` |

1. The log line `store.entry_ignored` (section 14) gives the path relative to the root and the reason. It occurs one time for each entry, and again after the entry changes.
2. A folder directory without a valid `_folder.json` is an invalid folder. The list shows it with the error of its folder file. Its dashboards are invalid entries with one error `not_permitted` that names the folder. The server does not serve them.
3. When two folders hold a file with the same uid, both entries are invalid with the error `not_unique`. The server serves neither of them.
4. A file of more than `MAX_DOCUMENT_BYTES` is an invalid entry with the error `too_large`. The scan reads at most `MAX_DOCUMENT_BYTES` plus 1 bytes of it.

### 5.11 Volume: history metadata file

| Member | Type | Rule |
|---|---|---|
| `format_version` | integer | `1`. It does not change with the format of the document (4.42, item 11). |
| `uid` | text | The uid. It MUST equal the name of the directory. |
| `version` | integer | 1 to 9223372036854775807. It MUST equal `<n>` of the file name. |
| `kind` | enum | A version kind (5.2) |
| `folder` | text | The folder id of the dashboard at this version |
| `etag` | text or null | The etag of `<n>.json`. For an `external` version without bytes: the SHA-256 of the raw bytes of the invalid file. `null` exactly when `kind` is `delete`. |
| `saved_at` | text | A `UtcMillis` text |
| `saved_by` | text | A subject of 1 to 256 characters, or a reserved text `-`, `external` or `import-grafana` (5.2) |
| `message` | text | 0 to 500 characters |
| `save_id` | text or null | A `SaveId` text, or `null` |

1. The file uses the canonical form of 4.38.
2. The file `<n>.json` exists, except for a `delete` version and for an `external` version of an invalid current file. When it exists, it holds canonical bytes, and its SHA-256 equals `etag`.
3. A version without its metadata file does not exist. The writer removes such a `<n>.json` at start (5.21).
4. A metadata file that breaks a rule is skipped by `history` and `version`, with the log line `store.entry_ignored`.
5. A version of kind `external` has `saved_by` equal to `external`.

Example: `<root>/.history/nodes/7.meta.json`

```json
{
  "format_version": 1,
  "uid": "nodes",
  "version": 7,
  "kind": "save",
  "folder": "transit",
  "etag": "50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501",
  "saved_at": "2026-11-09T10:15:42.123Z",
  "saved_by": "-",
  "message": "Add the ratio column",
  "save_id": "0f8e6b2c-3c1d-4f7a-9b55-2d4c8e1a7f30"
}
```

### 5.12 Volume: path safety

1. `path` of the volume block MUST be absolute. No component is empty, `.` or `..`. It has no `/` at the end. The canonical path (`realpath`) MUST equal the text, so no component is a symbolic link.
2. At start, the backend opens the root one time with `O_DIRECTORY | O_NOFOLLOW` and keeps the descriptor.
3. Each later access is relative to a directory descriptor: `openat`, `mkdirat`, `renameat`, `unlinkat` and `fstatat`, through `rustix`. Each call uses `O_NOFOLLOW` or `AT_SYMLINK_NOFOLLOW`.
4. Each name comes from a checked id or a fixed name of 5.9. A name never holds `/`, and it is never `.` or `..`. No name comes from the raw text of a client.
5. The backend never follows a symbolic link and never creates one. The scan skips a symbolic link as a foreign entry. An open that meets a link fails with `ELOOP`, and the operation returns `NotFound`.

A folder is a ProcurAuth node (section 11). A link would let a grant on one folder read another folder, in both access modes.

### 5.13 Volume: atomic write of one file

The write of one file has two phases, so that 5.14 can check the etag between them.

Prepare:

1. Open the temporary file with `openat(dir, tmp, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0640)`.
2. Write all bytes. Write again after a short write.
3. Call `fsync` on the file, then close it.

Commit:

1. Call `renameat(dir, tmp, dir, final)`.
2. Call `fsync` on the directory.

When a step fails, the backend removes the temporary file and returns the error. It never calls `fsync` again after an `fsync` error, because the state of the page cache is then unknown. The errors map as follows: `ENOSPC` and `EDQUOT` give `Full`. `EROFS`, `EACCES`, `EPERM`, `EIO` and the other errors give `Unavailable`.

### 5.14 Volume: save, step by step

Input: the document D with uid U, canonical bytes B and etag E_new; the value `if_match` E_if; the kind; the meta.

1. When the volume is open read-only, return `ReadOnly`. A volume is open read-only when it is not the primary store (5.42).
2. Take the store lock in shared mode, then the lock of U (5.16).
3. Find the folder F of U. Look first in the folder that the catalog gives, then in each folder directory. When no folder holds `<U>.json`, return `NotFound`.
4. Read `<F>/<U>.json`. Compute its etag E_cur: the canonical etag when the file is valid, else the SHA-256 of the raw bytes.
5. When the meta has a `save_id`, read the metadata files of `.history/<U>`, newest first. Let v be the version with this `save_id`.
   - When v belongs to another request (5.5, rule 4), return `SaveIdReused`.
   - When v is not the head, return `Saved` for v with the outcome `replayed`. Such a version was current one time, because a version that was never current has no `save_id` (5.5, rule 14).
   - When v is the head and is applied, return `Saved` for v with the outcome `replayed`. The test depends on the kind of v. For `save`, `restore`, `import` and `migrate`, v is applied when E_cur equals the etag of v. For `move`, v is applied when F equals the folder of v.
   - When v is the head, is not applied, and the state is still the state before v, roll forward (5.5, rule 14). Then return `Saved` for v with the outcome `written`. The state before v is: E_cur equals the etag of version v - 1 or E_if, and, for a `move`, F equals the folder of version v - 1.
   - Else write the metadata file of v again with `save_id` null (5.13), and return `Conflict` with E_cur.
   - To roll forward is to apply the effect of v. For a save, do steps 11 to 14 with the bytes of v. For a move, do steps 6 to 8 of `move_to` (5.15). For a delete, do steps 4 and 5 of `delete` (5.15).
6. When E_if differs from E_cur, or when F differs from `expected_folder`, return `Conflict` with E_cur.
7. When E_new equals E_cur, return `Saved` for the current version with the outcome `unchanged`.
8. Read the head version (the highest `<n>` with a metadata file). The head can be absent, or its etag can differ from E_cur. In both cases, first do steps 9 and 10 with the kind `external`. When the current file is valid, the `external` version holds its canonical bytes, and its etag is E_cur. When the current file is not valid, the version has the metadata file only, with the etag E_cur (5.11).
9. Let n be the head number plus 1. Write `.history/<U>/<n>.json` with B: prepare, then commit (5.13). Make the directory `.history/<U>` first when it does not exist, and call `fsync` on `.history`.
10. Write `.history/<U>/<n>.meta.json`: prepare, then commit.
11. Prepare the temporary file of `<F>/<U>.json` with B.
12. Read `<F>/<U>.json` again and compute its etag. When it differs from E_cur, remove the temporary file. Version n stays as a version that was never current (5.5, rule 8), so its number is not used again. Write the metadata file of version n again with `save_id` null (5.13), so that a retry takes the normal path (5.5, rule 14). The `external` version of step 8, when step 8 wrote one, also stays: it records the state before this attempt. Return `Conflict` with the new etag.
13. Commit the temporary file: rename it to `<F>/<U>.json`, then call `fsync` on `<F>`.
14. Apply the retention (5.18).
15. Release the locks. Return `Saved` with n, E_new and the outcome `written`.

A writer outside Hune that changes the file between steps 12 and 13 can lose its change (5.22, item 5).

### 5.15 Volume: the other writes

Each write below starts with the check of the read-only mode and returns `ReadOnly` when the volume is open read-only (5.19).

`create(F, D, kind, meta)`:

1. Take the store lock in exclusive mode, then the lock of U.
2. When F is not a valid folder, return `FolderNotFound`.
3. Do the `save_id` check of 5.14 step 5.
4. When a folder holds `<U>.json`, or when `.history/<U>` holds a version, return `Exists`.
5. When the store holds `MAX_DASHBOARDS` dashboards, return `Full`.
6. Write version 1 with the kind and B (5.14 steps 9 and 10).
7. Prepare the temporary file of `<F>/<U>.json`. When `<F>/<U>.json` exists now, remove the temporary file. Version 1 stays as a version that was never current: write its metadata file again with `save_id` null, then return `Exists`.
8. Commit the file. Return `Saved` with the outcome `written`.

`move_to(U, F2, E_if, meta)`:

1. Take the store lock in shared mode, then the lock of U.
2. When F2 is not a valid folder, return `FolderNotFound`.
3. Do 5.14 steps 3 to 6 (folder F1, etag, `save_id`, `if_match`).
4. When the current file is not valid, return `Invalid`. When F2 equals F1, return `unchanged`. When `<F2>/<U>.json` exists, return `Exists`.
5. Do 5.14 step 8 (version `external`). Write version n of kind `move` with folder F2 and the canonical bytes of the current document.
6. Rename `<F1>/<U>.json` to `<F2>/<U>.json` with `renameat2` and the flag `RENAME_NOREPLACE`. The target can exist now (`EEXIST`). Then version n stays as a version that was never current: write its metadata file again with `save_id` null, then return `Exists`. When the file system refuses the flag (`EINVAL`), check with `fstatat` that the target is absent, then call `renameat`.
7. Call `fsync` on `<F2>`, then on `<F1>`.
8. Apply the retention. Return `Saved` with the same etag.

`delete(U, E_if, meta)`:

1. Take the store lock in shared mode, then the lock of U.
2. Do 5.14 steps 3 to 6.
3. Do 5.14 step 8. Write version n of kind `delete`: the metadata file only.
4. Remove `<F>/<U>.json` with `unlinkat`. Call `fsync` on `<F>`.
5. Apply the retention. Return `Saved` without etag.

`restore(U, n, E_if, meta)`:

1. Read version n. When it does not exist, return `VersionNotFound`. When it is a tombstone or an `external` version without bytes, return `NotRestorable`.
2. Validate its bytes with the expected uid U (4.36). When they are not valid, return `NotRestorable`. Steps 1 and 2 take no lock of the store. Under the lock of step 3 or 4, the store first reads the metadata file of version n again. No file gives `VersionNotFound`, and an etag that is not the etag of the document of step 2 gives `NotRestorable`.
3. When U exists, do the save of 5.14 with these bytes, `if_match` and the kind `restore`.
4. When U does not exist, `if_match` MUST be absent, else return `NotFound`. Let F be the folder of version n. Do the steps of `create` above in F, with the kind `restore` and two changes.
5. Change 1: step 4 checks only that no folder holds `<U>.json`. The versions of U stay.
6. Change 2: step 6 writes version n' = the head number plus 1. When F does not exist, step 2 returns `FolderNotFound`.

`create_folder(id, title)`:

1. Take the store lock in exclusive mode.
2. When the store holds `MAX_FOLDERS` folders, return `Full`.
3. Make the directory `<id>` with `mkdirat` and the mode 0750. When it exists, return `Exists`.
4. Write `<id>/_folder.json` (5.13). When this fails, remove the directory and return the error.
5. Call `fsync` on the root.

`rename_folder(id, title)`: take the store lock in exclusive mode. When the folder is not valid, return `FolderNotFound`. Write `<id>/_folder.json` again (5.13).

`delete_folder(id)`:

1. Take the store lock in exclusive mode.
2. When the folder does not exist, return `FolderNotFound`.
3. List the directory. When it holds an entry other than `_folder.json`, return `FolderNotEmpty`. The message names the first such entry.
4. Remove `_folder.json`.
5. Remove the directory with `unlinkat` and `AT_REMOVEDIR`.
6. Call `fsync` on the root.

### 5.16 Volume: locks and the single writer

The volume has one writer process. A writer takes the lock of the volume at start and keeps it for the life of the process.

1. At start, a writer makes the directory `.hune` with the mode 0750 when it does not exist.
2. It opens `.hune/lock` with `O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC` and the mode 0640.
3. It calls `flock(LOCK_EX | LOCK_NB)` on it.
4. When the lock is held (`EWOULDBLOCK`), the process stops with exit code 1. The message names the path and gives the content of the lock file.
5. When the lock succeeds, the writer writes `<host name> <pid> <UtcMillis>` and a line end into the file, then calls `fsync`.
6. When the file system has no `flock` (`ENOLCK`, `EOPNOTSUPP` or `EINVAL`), the writer writes the log line `store.lock_unsupported` and continues. One writer is then the duty of the operator.
7. A volume that is open read-only takes no lock. A volume that is not the primary store is open read-only (5.19).

`flock` is best effort on NFS and FUSE. Several writers on one shared volume are not supported: use PostgreSQL. These commands are writers and take the same lock: `import-grafana --write`, `migrate-documents --write`, `import`, `copy-store` with a volume target, and `restore`. Each one stops when `serve` holds the lock.

Inside the process, the backend has two kinds of locks. The store lock is one async read-write lock: `create`, `create_folder`, `rename_folder` and `delete_folder` take it in exclusive mode, and the other writes take it in shared mode. The lock of a uid is one async mutex for each uid. A write takes the store lock first, then the lock of the uid, so two writes cannot block each other.

### 5.17 Volume: polling

The catalog calls `Store::poll` every `rescan_interval` of the volume block. The volume backend then does these steps:

1. List the root. For each folder directory, read the signature of `_folder.json` and list the directory.
2. For each dashboard file, read its signature with `fstatat`. The signature is `(dev, ino, size, mtime_ns, ctime_ns)`.
3. When the signature equals the mark of the catalog, skip the file.
4. Else read the file, validate it with the uid of its stem (4.36), and compute its etag (4.39). Then read the head metadata file of `.history/<uid>` (the highest `<n>`). The entry takes its `version`, `saved_at` and `saved_by` from it, with the rules of `Current` in 5.2 (an outside change, or no head version).
5. A file that is absent now is a removed entry. A folder directory that is absent now is a removed folder.
6. Apply the rules of 5.10 for foreign entries, invalid folders and duplicate uids.
7. Return the changes with the new signatures.

These rules apply to the poll:

1. The signature is a hint only. A write always compares the etag of the bytes (5.14). Thus a cache of file attributes on NFS cannot cause a lost update.
2. A changed file can have an etag that differs from the etag of the catalog. Then the backend writes the log line `dashboard.changed` with the action `external`, the uid and the two etags (section 14). It writes no version: the next write records the version `external` (5.5, rule 7).
3. An error on one file makes that entry invalid. An error on the root fails the poll (5.8, item 6).
4. In `.history`, the poll reads only the head metadata file of each changed uid.
5. After its own write, the backend gives the catalog the new signature. Thus the next poll does not read the file again.

### 5.18 Volume: history and retention

1. The versions of a uid are the files `<n>.meta.json` in `.history/<uid>`. The head version is the one with the highest `<n>`.
2. `history` reads the metadata files only. `version` reads the metadata file and `<n>.json`, when the version has this file (5.11, rule 2).
3. After each written version, the backend applies the retention. While the uid has more than `history_keep` versions (the primary volume only), it removes the oldest version: first `<n>.meta.json`, then `<n>.json`. Then it calls `fsync` on `.history/<uid>`.
4. A tombstone counts as a version.
5. The versions of a deleted dashboard stay until a new write on the same uid applies the retention.
6. Section 13 gives the range of `history_keep`.
7. The retention can remove the version of a `save_id` before its retry. 5.5 rule 15 gives the effect.

### 5.19 Volume: read-only mode

A volume is open read-only when its block has no `primary = true`, and in a dry run of a command:

1. The backend takes no lock, makes no directory, writes no temporary file and removes nothing.
2. Each write returns `ReadOnly`.
3. Reads and polling work as in a writer. A read-only volume has no `.hune` and no `.history`.
4. Several read-only processes can share a volume. One writer can write it at the same time, for example `import-grafana --write` or a job that updates a git checkout. Each write is a rename, so a reader sees each file complete, and it sees each change after one rescan.
5. Section 11 gives the effect on the published actions.

### 5.20 Volume: permissions and file systems

1. The process runs as the user 65532 in the image (section 17).
2. The backend makes files with the mode 0640 and directories with the mode 0750. It never changes the mode of a file that exists.
3. A root that others can write (`mode & 0o002`) stops the start. A folder directory that others can write is ignored as a foreign entry.
4. Supported: a local file system (ext4, xfs, btrfs), a Docker volume, a bind mount, NFSv4 and FUSE with one writer.
5. Not supported: several writers on one volume. `flock` cannot exclude them on all network file systems.

### 5.21 Volume: checks at start

The start stops with exit code 1 and a message that names the path in each of these cases:

1. `path` of the volume block breaks a rule of 5.12, item 1.
2. The root does not exist, is not a directory, or the process cannot read it.
3. Others can write the root.
4. `.hune` or `.history` exists and is not a directory.
5. A writer cannot write the root. The writer checks it with a probe file `.hune/probe-<16 hex>`: create with `O_EXCL`, write, `fsync`, remove.
6. The lock is held by another process (5.16).

The start stops for a file of the volume that cannot be read or that is not valid (5.43). The start does not stop for a foreign entry: Hune ignores it and writes `store.entry_ignored` (5.10).

After it takes the lock, a writer removes the temporary files of 5.9. It looks in the root, in each folder and in each directory of `.history`. It also removes each `<n>.json` without its metadata file. The lock makes sure that no other writer owns these files. A version with its metadata file stays, also when a crash came before its effect. A retry with its `save_id` applies it (5.5, rule 14).

### 5.22 Volume: use with git

1. Add `.hune/` to `.gitignore`.
2. `.history/` can be in git or in `.gitignore`. The command `git clean -fdx` removes an ignored history.
3. A `git pull` is visible after one rescan. The next write records the version `external` before its own version.
4. A file with git conflict markers is not valid JSON. It is an invalid entry until a person corrects it.
5. A change of a file in use by an editor of Hune can conflict with a save (5.14). The save then answers `Conflict`, or, in a small window, the outside change is lost.

### 5.23 PostgreSQL: connection, TLS and credentials

The backend uses `tokio-postgres` with the pool `deadpool-postgres`. Section 13 gives the keys of the block `storage.postgres`.

| Connection setting | Value |
|---|---|
| Host and port | `storage.postgres.host`, `storage.postgres.port` |
| Database and user | `storage.postgres.database`, `storage.postgres.user` |
| Password | From `password_env` or `password_file` only (a secret) |
| `application_name` | `hune` |
| `target_session_attrs` | `read-write`: a node in recovery is refused at connect |
| Connect timeout | `storage.postgres.connect_timeout` |
| TCP keepalive | On; idle 10 s, interval 5 s, 3 probes |
| `tcp_user_timeout` | 10 s |
| Pool size | `storage.postgres.max_connections` |
| Pool wait | At most `storage.postgres.statement_timeout`, then `Unavailable` |
| Pool recycle | The `Fast` method of `deadpool-postgres` |

1. After each new connection, the backend sends one batch: `SET statement_timeout`, `SET idle_in_transaction_session_timeout` (both to `storage.postgres.statement_timeout`), `SET search_path = public` and `SET timezone = 'UTC'`.
2. A cut node never resets a connection. The keepalive and `tcp_user_timeout` close such a connection in about 25 s.
3. With `storage.postgres.sslmode = "verify-full"`, the backend uses TLS through `tokio-postgres-rustls`. The root store holds only the certificates of `storage.postgres.tls.ca_file`. The backend checks the server name `storage.postgres.tls.server_name`. It accepts TLS 1.2 and TLS 1.3.
4. With `verify-full`, the backend never uses plain TCP. With `sslmode = "disable"`, it uses plain TCP. Section 13 gives the keys, the contradictions and the warning for a host that is not a loopback address.
5. The backend connects to the primary service directly (for example the CloudNativePG service `-rw`), or through PgBouncer in session mode. Transaction pooling is not supported: `migrate-store` needs a session advisory lock.

These errors make the backend return `Unavailable`:

| Error | Effect on the connection |
|---|---|
| An I/O error, a timeout, a TLS error, SQLSTATE class `08` | The connection leaves the pool |
| SQLSTATE `57P01`, `57P02`, `57P03` (server stop), `25006` (read-only transaction) | The connection leaves the pool |
| SQLSTATE `57014` (statement timeout), `40001`, `40P01`, class `53` | The connection stays in the pool |
| Another SQLSTATE that 5.27 does not map | The connection stays in the pool. The log line `store.query_failed` gives the SQLSTATE. |

A read (`load`, `poll`, `get`, `history`, `version`) that meets a connection error is sent again one time on a new connection. A write is never sent again (5.5, rule 11).

### 5.24 PostgreSQL: DDL

Migration 1 creates the schema of v1. `hune migrate-store` applies it in one transaction (5.28). The SQL is in the binary as `hune-store/migrations/0001.sql`.

```sql
-- Migration 1 of the Hune schema.
CREATE TABLE hune_schema (
    id         boolean     PRIMARY KEY DEFAULT true CHECK (id),
    version    integer     NOT NULL CHECK (version >= 1),
    updated_at timestamptz NOT NULL
);

CREATE TABLE hune_folders (
    id         text        PRIMARY KEY CHECK (id ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    title      text        NOT NULL CHECK (char_length(title) BETWEEN 1 AND 256),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE hune_dashboards (
    uid            text        PRIMARY KEY CHECK (uid ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    folder_id      text        NOT NULL REFERENCES hune_folders (id) ON DELETE RESTRICT,
    version        bigint      NOT NULL CHECK (version >= 1),
    etag           text        NOT NULL CHECK (etag ~ '^[0-9a-f]{64}$'),
    format_version integer     NOT NULL,
    title          text        NOT NULL,
    document       text        NOT NULL CHECK (octet_length(document) <= 4194304),
    updated_at     timestamptz NOT NULL,
    updated_by     text        NOT NULL CHECK (char_length(updated_by) BETWEEN 1 AND 256)
);

CREATE TABLE hune_versions (
    uid        text        NOT NULL CHECK (uid ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    version    bigint      NOT NULL CHECK (version >= 1),
    kind       text        NOT NULL CHECK (kind IN ('create', 'save', 'restore', 'move',
                                                    'delete', 'import', 'external', 'migrate')),
    folder_id  text        NOT NULL CHECK (folder_id ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    document   text        CHECK (octet_length(document) <= 4194304),
    etag       text        CHECK (etag ~ '^[0-9a-f]{64}$'),
    saved_at   timestamptz NOT NULL,
    saved_by   text        NOT NULL CHECK (char_length(saved_by) BETWEEN 1 AND 256),
    message    text        NOT NULL CHECK (char_length(message) <= 500),
    save_id    uuid,
    PRIMARY KEY (uid, version),
    UNIQUE (uid, save_id),
    CHECK ((kind IN ('delete', 'external')) OR document IS NOT NULL),
    CHECK (kind <> 'delete' OR document IS NULL),
    CHECK ((kind = 'delete') = (etag IS NULL))
);

INSERT INTO hune_schema (id, version, updated_at) VALUES (true, 1, now());

-- The default privileges of 5.25 give the application role INSERT, UPDATE and DELETE on
-- each new table. Only migrate-store writes hune_schema, so remove these rights from each
-- role other than the owner. With the role names of 5.25, this is:
-- REVOKE INSERT, UPDATE, DELETE ON hune_schema FROM hune_app;
DO $$
DECLARE r record;
BEGIN
    FOR r IN SELECT DISTINCT grantee FROM information_schema.role_table_grants
              WHERE table_schema = 'public' AND table_name = 'hune_schema'
                AND privilege_type IN ('INSERT', 'UPDATE', 'DELETE')
                AND grantee <> current_user AND grantee <> 'PUBLIC'
    LOOP
        EXECUTE format('REVOKE INSERT, UPDATE, DELETE ON hune_schema FROM %I', r.grantee);
    END LOOP;
END $$;
```

1. The column `document` holds the canonical bytes as `text`, not `jsonb`. `jsonb` keeps the last of two duplicate members, changes the order of members and changes the bytes. The etag would then differ from the volume.
2. `hune_versions` has no foreign key. The history of a deleted dashboard stays, also after its folder is deleted.
3. `hune_dashboards.version` is the number of the current version. `updated_at` and `updated_by` equal `saved_at` and `saved_by` of that version.
4. `UNIQUE (uid, save_id)` accepts many rows with `save_id` null.
5. The database encoding MUST be `UTF8` (5.29).
6. A later migration k ends with `UPDATE hune_schema SET version = k, updated_at = now() WHERE id`.
7. A `delete` version has no document and no etag. An `external` version of a current document that is not valid has no document, and its etag is the SHA-256 of the stored text (5.2). Each other version has a document and an etag.
8. Hune writes `etag` and `document` of a row together, and `etag` is the SHA-256 of the canonical form of `document`. `load`, `poll` (P7) and `get` compute the etag of `document` again. The entry is invalid when this etag differs from the column `etag`, or when `document` is not canonical. Its one error has the code `not_permitted` and names the column (4.37). A tool that changes a row outside Hune MUST write both columns.

### 5.25 PostgreSQL: roles and rights

Hune uses two PostgreSQL roles (12.13). Each role has its own configuration file (14.17):

| Role | Configuration file | Commands | Rights |
|---|---|---|---|
| Owner role (example `hune_owner`) | `migrate.hcl`: its `storage.postgres.user` names the owner role | `migrate-store` only | Owns the four tables; `CREATE` and `USAGE` on the schema `public` |
| Application role (example `hune_app`) | `config.hcl`: its `storage.postgres.user` names the application role | `serve`, `export`, `import`, `copy-store`, `restore`, `import-grafana --write`, `migrate-documents` | `CONNECT` on the database, `USAGE` on the schema, and only `SELECT`, `INSERT`, `UPDATE` and `DELETE` on the tables of Hune. It cannot change the schema. |

```sql
CREATE ROLE hune_owner LOGIN;
GRANT CONNECT ON DATABASE hune TO hune_owner;
GRANT CREATE, USAGE ON SCHEMA public TO hune_owner;

CREATE ROLE hune_app LOGIN;
GRANT CONNECT ON DATABASE hune TO hune_app;
GRANT USAGE ON SCHEMA public TO hune_app;

-- Before the first "hune migrate-store": each table that the owner role makes later
-- (migration 1 and each later migration) gives these rights to the application role.
ALTER DEFAULT PRIVILEGES FOR ROLE hune_owner IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO hune_app;

-- Only for tables that exist before the statement above (a database made without it):
GRANT SELECT ON hune_schema TO hune_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON hune_folders, hune_dashboards, hune_versions TO hune_app;

-- In every case: the application role only reads hune_schema.
-- Migration 1 of 5.24 does the same for each role other than the owner.
REVOKE INSERT, UPDATE, DELETE ON hune_schema FROM hune_app;
```

1. Neither role is a superuser.
2. `serve` MUST stop at start when its role is a superuser. It MUST also stop when its role owns a table of Hune, directly or as a member of the role that owns it. The check is step 6 of 5.29, and the message names this rule.
3. `migrate-store` uses the owner role, through `migrate.hcl` (5.28).
4. The operator sets the passwords outside this SQL. Each file gives the password with `storage.postgres.password_env` or `storage.postgres.password_file` (section 13).
5. `pg_hba.conf` authenticates both roles with `scram-sha-256` (12.13).
6. Both files are complete configuration files, and `hune check-config` checks each one (13.18).

### 5.26 PostgreSQL: queries

The backend sends only these statements. Each one is a prepared statement with typed parameters. `COLLATE "C"` gives the byte order of the ids.

```sql
-- P1  Encoding of the database (start, migrate-store)
SHOW server_encoding;
-- P2  The schema table exists (start, migrate-store)
SELECT to_regclass('public.hune_schema') IS NOT NULL;
-- P3  Schema version (start, migrate-store)
SELECT version FROM hune_schema WHERE id;
-- P4  All folders (load, poll, export)
SELECT id, title FROM hune_folders ORDER BY id COLLATE "C";
-- P5  All current dashboards (load)
SELECT uid, folder_id, version, etag, document, updated_at, updated_by
  FROM hune_dashboards ORDER BY uid COLLATE "C";
-- P6  Marks of the current dashboards (poll)
SELECT uid, folder_id, etag FROM hune_dashboards ORDER BY uid COLLATE "C";
-- P7  Changed dashboards (poll), at most 1000 uids in $1
SELECT uid, folder_id, version, etag, document, updated_at, updated_by
  FROM hune_dashboards WHERE uid = ANY($1::text[]) ORDER BY uid COLLATE "C";
-- P8  One current dashboard (get, export)
SELECT folder_id, version, etag, document, updated_at, updated_by
  FROM hune_dashboards WHERE uid = $1;
-- P9  Lock of one current dashboard (writes)
SELECT folder_id, version, etag, document FROM hune_dashboards WHERE uid = $1 FOR UPDATE;
-- P10 Version with this save id (writes): the kind decides "the same request" (5.5, rule 4)
SELECT version, kind, folder_id, etag FROM hune_versions WHERE uid = $1 AND save_id = $2;
-- P11 Head version (writes)
SELECT version, etag FROM hune_versions WHERE uid = $1 ORDER BY version DESC LIMIT 1;
-- P12 Lock of the creates (create, create_folder, restore of a deleted dashboard, import)
SELECT pg_advisory_xact_lock(8532464671393841154);
-- P13 Counts (create, create_folder)
SELECT (SELECT count(*) FROM hune_dashboards), (SELECT count(*) FROM hune_folders);
-- P14 The folder exists (create, move_to, restore of a deleted dashboard)
SELECT 1 FROM hune_folders WHERE id = $1 FOR SHARE;
-- P15 Insert a version
INSERT INTO hune_versions (uid, version, kind, folder_id, document, etag,
                            saved_at, saved_by, message, save_id)
  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10);
-- P16 Insert a current dashboard (create, restore of a deleted dashboard, import)
INSERT INTO hune_dashboards (uid, folder_id, version, etag, format_version, title,
                              document, updated_at, updated_by)
  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (uid) DO NOTHING;
-- P17 Update a current dashboard (save, move_to, restore)
UPDATE hune_dashboards SET folder_id = $2, version = $3, etag = $4, format_version = $5,
       title = $6, document = $7, updated_at = $8, updated_by = $9 WHERE uid = $1;
-- P18 Remove a current dashboard (delete)
DELETE FROM hune_dashboards WHERE uid = $1;
-- P19 Retention: $2 = the new version number minus history_keep
DELETE FROM hune_versions WHERE uid = $1 AND version <= $2;
-- P20 One page of history: $2 = before, or 9223372036854775807; $3 = limit
SELECT version, kind, folder_id, etag, saved_at, saved_by, message, save_id, octet_length(document)
  FROM hune_versions WHERE uid = $1 AND version < $2 ORDER BY version DESC LIMIT $3;
-- P21 One version (version, restore)
SELECT kind, folder_id, document, etag, saved_at, saved_by, message, save_id
  FROM hune_versions WHERE uid = $1 AND version = $2;
-- P22 Create a folder (create_folder, import)
INSERT INTO hune_folders (id, title, created_at, updated_at) VALUES ($1, $2, $3, $3)
  ON CONFLICT (id) DO NOTHING;
-- P23 Rename a folder
UPDATE hune_folders SET title = $2, updated_at = $3 WHERE id = $1;
-- P24 Delete a folder
DELETE FROM hune_folders WHERE id = $1;
-- P25 All uids with a current dashboard or a version (export)
SELECT uid FROM hune_versions UNION SELECT uid FROM hune_dashboards ORDER BY 1 COLLATE "C";
-- P26 All versions of one uid (export)
SELECT version, kind, folder_id, document, etag, saved_at, saved_by, message, save_id
  FROM hune_versions WHERE uid = $1 ORDER BY version;
-- P27 The store is empty (import)
SELECT (SELECT count(*) FROM hune_folders) + (SELECT count(*) FROM hune_dashboards)
     + (SELECT count(*) FROM hune_versions);
-- P28 Lock of the migrations (migrate-store)
SELECT pg_try_advisory_lock(8532464671393841153);
-- P29 Release of the lock of the migrations (migrate-store)
SELECT pg_advisory_unlock(8532464671393841153);
-- P30 Rights of the role (start of serve): superuser, and owner of a table of Hune
SELECT r.rolsuper,
       EXISTS (SELECT 1 FROM pg_tables t
                WHERE t.schemaname = 'public'
                  AND t.tablename IN ('hune_schema', 'hune_folders', 'hune_dashboards', 'hune_versions')
                  AND pg_has_role(current_user, t.tableowner, 'MEMBER'))
  FROM pg_roles r WHERE r.rolname = current_user;
```

The key 8532464671393841153 is `0x7669676965000001` (the ASCII text `hune` and the number 1). The key of the creates is this key plus 1.

### 5.27 PostgreSQL: transactions of each operation

Each write is one transaction at the level `READ COMMITTED`. A step that returns an error first sends `ROLLBACK`. An error at `COMMIT` returns `Unavailable`: the outcome is then unknown (5.5, rule 11).

| Operation | Steps |
|---|---|
| `save` | `BEGIN`; P9 (no row: `NotFound`); P10 when `save_id` is set (a row: `replayed` or `SaveIdReused`, 5.5 rule 4); compare the etag (`Conflict`); same etag as the new document: `unchanged`; P11 (head etag differs from the current etag: P15 with kind `external` and the current document, or with `document` null when the current document is not valid); P15 with the kind of the call; P17; P19; `COMMIT` |
| `create` | `BEGIN`; P12; P10 when `save_id` is set (`replayed` or `SaveIdReused`); P14 (no row: `FolderNotFound`); P13 (dashboards at `MAX_DASHBOARDS`: `Full`); P11 (a row: `Exists`); P16 with version 1 (0 rows: `Exists`); P15 with version 1; `COMMIT` |
| `move_to` | `BEGIN`; P9; P10; compare the etag; current document not valid: `Invalid`; same folder: `unchanged`; P14 for the target (`FolderNotFound`); P11 and the version `external` when needed; P15 with kind `move`, the target folder and the current document; P17; P19; `COMMIT` |
| `delete` | `BEGIN`; P9; P10; compare the etag; P11 and the version `external` when needed; P15 with kind `delete`, `document` and `etag` null; P18; P19; `COMMIT` |
| `restore`, dashboard exists | Before `BEGIN`: the read of `version` and the validation of the bytes (`NotRestorable`), out of the transaction. Then `BEGIN`; P21 again (no row: `VersionNotFound`; kind `delete`, `document` null, or an etag that is not the etag of the valid document: `NotRestorable`); then the steps of `save` with the valid document and the kind `restore`. The transaction never validates the document. |
| `restore`, dashboard deleted | Before `BEGIN`: the read and the validation, as above. Then `BEGIN`; P21; P12; P9 (a row: then the case above); P10; P14 for the folder of the version (`FolderNotFound`); P13 (`Full`); P11 (n = head plus 1); P16; P15 with kind `restore`; `COMMIT` |
| `create_folder` | `BEGIN`; P12; P13 (folders at `MAX_FOLDERS`: `Full`); P22 (0 rows: `Exists`); `COMMIT` |
| `rename_folder` | P23 alone (0 rows: `FolderNotFound`) |
| `delete_folder` | P24 alone (SQLSTATE `23503`: `FolderNotEmpty`; 0 rows: `FolderNotFound`) |
| `get` | P8 (no row: `NotFound`); validate the bytes and compare the etag (5.24, item 8; `Invalid`) |
| `history` | P20; with no row and no `before`, P8 (no row: `NotFound`; a row: an empty list) |
| `version` | P21 (no row: `VersionNotFound`; `document` null: the metadata without bytes) |
| `load` | `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY`; P4; P5; `COMMIT` |
| `poll` | `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY`; P4; P6; `COMMIT`; then P7 for the changed uids |
| export | `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY`; P4; P25; for each uid P26 and P8; `COMMIT` |
| import | `BEGIN`; P12; P27 (not 0: `NotEmpty`); P22 for each folder; for each uid, P15 for each version in ascending order, then P16 for the current document; `COMMIT` |

1. The version number of a new version is the head number plus 1. After a version `external`, the next version takes the number after it.
2. P9 serialises the writes on one uid. P12 serialises the creates, so P13 counts exactly.
3. The parameters `saved_at` and `updated_at` are `UtcMillis` values from the instance. The backend does not use `now()` for them.
4. SQLSTATE `23503` in `create` or `move_to` (a folder deleted at the same time) returns `FolderNotFound`.
5. In each step "compare the etag", the backend also compares `folder_id` of P9 with `expected_folder` (5.5, rule 2). A difference returns `Conflict`.
6. In each step P10, a row of the same request (5.5, rule 4) returns `replayed`, and a row of another request returns `SaveIdReused`. The comparison uses `kind`, `etag` and `folder_id` of the row. PostgreSQL never needs the roll-forward of 5.5 rule 14.
7. The current etag of a write is the column `etag` of P9 (5.24, item 8).

### 5.28 PostgreSQL: `hune migrate-store`

`hune migrate-store --config <migrate.hcl>` applies the schema migrations. It is the only code that changes the schema. `serve` never migrates (rule: no migration in the start code). It uses the owner role of 5.25 through `migrate.hcl`: the key `storage.postgres.user` of this file names the owner role.

1. Read and check the configuration file. When the primary store is not `storage.postgres`, stop with exit code 1.
2. Open one direct session, outside the pool.
3. Send P1. When the encoding is not `UTF8`, stop with exit code 1.
4. Send P28. When it gives `false`, stop with exit code 1 and the message `another hune migrate-store holds the lock`.
5. Send P2 and P3. The database version V is 0 when the table `hune_schema` does not exist.
6. Let M be the schema version of the binary (1 in v1). When V is greater than M, release the lock (P29) and stop with exit code 1: `database schema V is newer than this binary (M)`.
7. For each step k from V + 1 to M: `BEGIN`, the SQL of step k, `COMMIT`. Then write the log line `store.migrated` with k.
8. When a step fails, send `ROLLBACK` and P29, and stop with exit code 1. The message gives k and the PostgreSQL error. The schema stays at version k - 1.
9. Send P29. Print `schema version M` and exit with code 0. When V equals M, print `schema version M, nothing to do` and exit with code 0.

In compose, `migrate-store` is a one-shot service with `migrate.hcl`, and `serve` depends on it with `service_completed_successfully`. In K3s, the operator scales the Deployment to 0, runs `migrate-store` as a Job, and then scales the Deployment back (14.19).

### 5.29 PostgreSQL: checks at start

`serve`, `export`, `import`, `copy-store`, `restore`, `import-grafana --write` and `migrate-documents` check the database before their first other statement:

1. Connect. A failure is temporary or permanent:
   - Temporary: a network failure, a timeout, SQLSTATE `57P03` (the server starts), or a node in recovery that `target_session_attrs` refuses. `serve` writes the log line `store.sync_failed` one time and tries again after each `storage.postgres.connect_timeout`. `/readyz` stays at 503 (section 14).
   - Permanent: SQLSTATE class `28` (a wrong role or password), `3D000` (no such database), or a failure of the certificate check. `serve` writes `server.start_failed` with the error and stops with exit code 1.
   - The other commands stop with exit code 1 after any failure.
2. Send P1. When the encoding is not `UTF8`, stop with exit code 1: `storage.postgres: database encoding is <X>; Hune needs UTF8`.
3. Send P2. When the table does not exist, stop with exit code 1: `storage.postgres: the database has no Hune schema; run: hune migrate-store -c <file>`.
4. Send P3. When V is less than M, stop with exit code 1: `storage.postgres: database schema version V is older than this binary (M); run: hune migrate-store -c <file>`.
5. When V is greater than M, stop with exit code 1: `storage.postgres: database schema version V is newer than this binary (M); use a binary of schema version V`.
6. `serve` only: send P30. When `rolsuper` is true, or when the role owns a table of Hune, stop with exit code 1. The message is `storage.postgres: the role <user> is a superuser or owns the Hune tables; serve needs a role with only SELECT, INSERT, UPDATE and DELETE (5.25, 12.13)`.

A process that waits for PostgreSQL does not stop. Thus a PostgreSQL outage at start causes no restart loop of the pods. A configuration error stops the start at once (section 13).

### 5.30 PostgreSQL: polling of the catalog

Hune does not use `LISTEN` and `NOTIFY`. Each instance polls the catalog every `storage.postgres.poll_interval`:

1. In one read-only transaction, send P4 and P6.
2. Compare the result with the marks of the catalog. A new folder title is a change. A new uid, an absent uid, and a uid with another etag or folder are changes.
3. Send P7 for the changed uids, in groups of at most 1000.
4. Return the changes.

A change on one instance is visible on the other instances after at most `poll_interval` plus the time of the poll. Every panel query carries the etag of its dashboard. Thus a page that shows an old version gets 409 and loads the new version (3.13).

The poll of PostgreSQL writes no `dashboard.changed`: the instance that wrote the change wrote that line. The poll checks each fetched row with 5.24, item 8.

### 5.31 PostgreSQL: failover, step by step

The reference is CloudNativePG with three nodes and synchronous replication `ANY 1`. A measure on such a cluster gave 1 to 2 minutes without a primary after the loss of a node.

1. The primary node is lost. Its connections receive no reset. The keepalive and `tcp_user_timeout` close them in about 25 s. `statement_timeout` bounds a statement that waits.
2. A save in flight fails. The store returns `Unavailable`, and the server answers 503 `storage_unavailable`. The commit can have occurred or not.
3. The browser keeps the draft and tells the author that saves are not possible now (section 10).
4. The poll fails. The catalog keeps its content. The list, the pages and the panel queries of all dashboards continue. `hune_store_up` is 0.
5. Each new write opens a new connection to `host:port`. While no primary exists, the connection fails after `storage.postgres.connect_timeout`, or `target_session_attrs` refuses a standby. The write answers 503.
6. `get`, `history` and `version` read PostgreSQL. Each one is sent again one time on a new connection, then answers 503.
7. CloudNativePG promotes a standby and moves the service `-rw` to it. New connections reach the new primary.
8. The next poll succeeds. The catalog compares all rows and applies the changes. `hune_store_up` is 1.
9. The browser sends the save again with the same `save_id`. When the first commit occurred, the store returns `replayed`. The synchronous replication kept that commit on the new primary. Else the store writes the save now.
10. The old primary returns as a standby. `target_session_attrs` refuses it at connect. A pooled connection that reaches it gets `25006` on a write and leaves the pool.

No instance restarts. `/readyz` stays at 200 (section 14).

### 5.32 PostgreSQL: versions and HA reference

1. Supported: PostgreSQL 15, 16, 17 and 18. CI runs the conformance suite on 15 and on 18 (section 16).
2. Reference HA deployment: CloudNativePG. The reference setup is CloudNativePG 1.30 with PostgreSQL 18.4, three instances, synchronous quorum and `dataDurability: required`. With this setting, an acknowledged save is not lost at failover.
3. With asynchronous replication, a failover can lose the last acknowledged saves. The documentation states it.
4. The automatic failover test runs in Docker with a three-node Patroni cluster. A TCP proxy gives one `host:port` that points to the leader. Section 16 gives the steps and the pass criteria.
5. The manual drill on CloudNativePG runs before release 1.0.0 (section 16).

### 5.33 Conformance suite: rules

1. The suite is the module `hune_store::conformance`, behind the Cargo feature `conformance`.
2. Each test is generic over the `Store` trait. It gets a new empty store from a factory of the backend.
3. A factory also gives test hooks. A hook changes the backend outside the trait. For example, it writes a file, changes a row, breaks the connection or injects a failure.
4. CI runs the common tests (5.34) on a volume in a temporary directory and on PostgreSQL 15 and 18 in containers. It runs the tests of 5.35 on the volume and the tests of 5.36 on PostgreSQL.
5. A common test MUST pass on both backends. A failure on one backend fails the job.
6. The tests use small fixtures. The fixture of 4.40 is one of them.

### 5.34 Conformance suite: common tests

| Id | Test | Pass criterion |
|---|---|---|
| SC-01 | Empty store | `load` gives no folder and no entry. Export gives nothing. |
| SC-02 | Create a folder | `load` gives the folder with its title |
| SC-03 | Create a folder that exists | `Exists` |
| SC-04 | Rename a folder | The title changes and the id stays. The same title succeeds without a change. |
| SC-05 | Rename or delete an absent folder | `FolderNotFound` |
| SC-06 | Delete an empty folder | Success; `load` does not give it |
| SC-07 | Delete a folder with a dashboard | `FolderNotEmpty` |
| SC-08 | Create a dashboard | Version 1, kind `create`, outcome `written`. `get` gives the exact bytes. The etag is the SHA-256 of the bytes. |
| SC-09 | Create in an absent folder | `FolderNotFound`; the uid has no version |
| SC-10 | Create a uid that exists, in the same folder and in another folder | `Exists` both times |
| SC-11 | Create a deleted uid | `Exists`; the versions stay |
| SC-12 | Save with the current etag | `written`; version plus 1; message, `saved_by`, `saved_at` and `save_id` kept |
| SC-13 | Save with an old etag | `Conflict` with the current etag; no change in `get` and `history` |
| SC-14 | Save the same content | `unchanged`; no version |
| SC-15 | Save an absent uid | `NotFound` |
| SC-16 | Save two times with one `save_id`, the second with the old etag | First `written`, second `replayed` with the same version and etag; one version |
| SC-17 | Create two times with one `save_id` | One dashboard, one version; second `replayed` |
| SC-18 | Second request with the same `save_id` and another content; a move with the `save_id` of a save; a delete with the `save_id` of a save; a restore with the `save_id` of a save of the same bytes | `SaveIdReused` each time; nothing written |
| SC-19 | 16 saves at the same time with one `if_match` and 16 contents | Exactly one `written`, 15 `Conflict`; one new version |
| SC-20 | 16 creates of one uid at the same time | Exactly one `written`, 15 `Exists` |
| SC-21 | 16 saves at the same time on 16 uids | 16 `written` |
| SC-22 | Move | New folder in `get`; same etag; version of kind `move` with the target folder |
| SC-23 | Move to an absent folder; with an old etag; to the same folder | `FolderNotFound`; `Conflict`; `unchanged` |
| SC-24 | Delete | Tombstone without document and etag; `get` gives `NotFound`; `history` lists all versions; `version` of the tombstone gives its metadata without bytes |
| SC-25 | Delete with an old etag | `Conflict` |
| SC-26 | Restore version n of a dashboard that exists | New version of kind `restore` with the bytes and the etag of version n |
| SC-27 | Restore with an old or an absent `if_match` | `Conflict` |
| SC-28 | Restore a tombstone; an absent version; a version removed by the retention | `NotRestorable`; `VersionNotFound`; `VersionNotFound` |
| SC-29 | Restore a deleted dashboard without and with `if_match` | Back in the folder of version n, with the version number head plus 1; with `if_match`: `NotFound` |
| SC-30 | Restore a deleted dashboard whose folder is deleted | `FolderNotFound` |
| SC-31 | Restore a version of another format version (hook) | `NotRestorable` |
| SC-32 | History pages | Newest first; `before` is exclusive; `limit` applies; at most 100 |
| SC-33 | History of a uid with no current dashboard and no version | `NotFound` |
| SC-34 | Retention with `history_keep` = 3 | After a create and 4 saves, versions 3 to 5 stay. The next save is version 6. |
| SC-35 | Retention after a delete | The tombstone counts as a version |
| SC-36 | Read each kept version | Exact bytes and metadata |
| SC-37 | Outside change (hook), then a save with the new etag | A version `external` with the outside bytes, then the version of the save |
| SC-38 | Outside change (hook), then a save with the old etag | `Conflict` with the etag of the outside bytes |
| SC-39 | A second store on the same backend polls | It sees a create, a save, a move, a delete, a folder create, rename and delete after one poll |
| SC-40 | Invalid stored document (hook) | `load` and `poll` give an invalid entry with its errors; `get` gives `Invalid` |
| SC-41 | Limits | Folder number `MAX_FOLDERS` plus 1 and dashboard number `MAX_DASHBOARDS` plus 1 give `Full` |
| SC-42 | Time precision | `saved_at` keeps the milliseconds and loses nothing in a round trip |
| SC-43 | Fixture of 4.40 | `get` gives 8,397 bytes and the etag `50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501` |
| SC-44 | Export, import into an empty store of the other kind, export again | Byte-equal directories, with folders, history, tombstones and an outside change |
| SC-45 | Import into a store that is not empty | `NotEmpty`; nothing written |
| SC-46 | Import of a layout with an invalid current document | `Invalid`; nothing written |
| SC-47 | `copy_store` volume to PostgreSQL to volume; PostgreSQL to volume to PostgreSQL | Byte-equal files; equal rows of dashboards and versions; equal ids and titles of folders |
| SC-48 | The backend breaks during a save (hook), then works again | The save gives `Unavailable`. The same save with the same `save_id` gives `written` or `replayed`. Then `get` gives the new bytes. The uid never gets two versions for it. |
| SC-49 | A save, a move, a delete and a restore with the current etag and an `expected_folder` that is not the current folder | `Conflict` each time; nothing written |
| SC-50 | The current document is not valid (hook); a save with `if_match` equal to the SHA-256 of its raw bytes | A version `external` without bytes, with that etag, then the version of the save. `version` of the `external` version gives no bytes; `restore` of it gives `NotRestorable`; a `move_to` before the save gives `Invalid`. |
| SC-51 | A uid with a current document and no version (hook) | `get` gives version 0 and `saved_by` `external`; `history` gives an empty list |
| SC-52 | Retention with `history_keep` = 2; a save S with a `save_id`, then 2 other saves, then S again | `Conflict` (5.5, rule 15) |

### 5.35 Conformance suite: volume tests

| Id | Test | Pass criterion |
|---|---|---|
| SV-01 | Root path relative, with `..`, with a final `/`, with a symbolic link | The start stops |
| SV-02 | Root absent, a file, not readable | The start stops |
| SV-03 | Root that others can write | The start stops |
| SV-04 | `.hune` or `.history` is a symbolic link or a file | The start stops |
| SV-05 | Second writer on a held lock | It stops; the message gives the content of the lock file |
| SV-06 | Read-only process beside a writer | No lock; each write gives `ReadOnly`; reads work |
| SV-07 | Symbolic link as a folder, a dashboard file, `_folder.json`, a history directory | Not followed; foreign entry; no write through it |
| SV-08 | Foreign entries; `.git` | Ignored; one log line for each foreign entry; no line for `.git` |
| SV-09 | Folder without `_folder.json` | Invalid folder; its dashboards are invalid and not served |
| SV-10 | One uid in two folders | Both entries invalid with `not_unique` |
| SV-11 | Dashboard file of 4 MiB plus 1 byte | Invalid with `too_large`; the scan reads at most `MAX_DOCUMENT_BYTES` plus 1 bytes |
| SV-12 | Valid file that is not canonical | Served in canonical form with the canonical etag; the file stays; after a save, the file is canonical; the `external` version of that save holds the canonical bytes, not the raw bytes |
| SV-13 | Truncated JSON written outside, then the full file | Invalid, then valid after the next poll |
| SV-14 | A new file renamed over the old one (as `git pull`) | Visible after one rescan; log line `dashboard.changed` with the action `external`; the next save writes `external` |
| SV-15 | No space on a small tmpfs | `Full`; no temporary file stays; the current file does not change |
| SV-16 | Failure injected after the history files of version n, before the rename; then the same save with the same `save_id` | The failure gives `Unavailable`, and the current file does not change. The retry rolls forward: `written` for version n, `get` gives the new bytes, and the uid has one version for this save. |
| SV-16a | Failure as SV-16, then a save with another `save_id` and the old etag | A version `external` with the current bytes, then the version of the new save |
| SV-17 | Failure injected after `<n>.json`, before `<n>.meta.json` | At the next start, `<n>.json` is removed; version n does not exist |
| SV-18 | Temporary files from a crash | Removed at the next start |
| SV-19 | The file changes between the prepare and the check of 5.14 step 12 (hook) | `Conflict`; version n stays as a version that was never current, with `save_id` null; the `external` version of step 8 stays; the next version takes n + 1; a retry with the same `save_id` gives `Conflict` |
| SV-20 | Modes | Files 0640, directories 0750; the mode of a file that exists does not change |
| SV-21 | File system without `flock` (hook returns `ENOLCK`) | Log line `store.lock_unsupported`; the start continues |
| SV-22 | `import`, `restore`, `migrate-documents --write` beside `serve` | Each one stops on the lock |
| SV-23 | Delete a folder with a foreign entry | `FolderNotEmpty`; the message names the entry |
| SV-24 | History metadata file with a wrong uid or version | Skipped; log line `store.entry_ignored` |
| SV-25 | Crash after the history files of a `create` (hook), then the same create with the same `save_id` | Rolls forward: `written` for version 1; `get` gives the document |
| SV-26 | A read-only process polls while a writer saves | After one rescan, the entry has the version and `saved_by` of the head metadata file |
| SV-27 | A file appears in the target folder between steps 4 and 6 of `move_to` (hook) | `Exists`; the file of the target folder does not change; version n stays as a version that was never current, with `save_id` null |
| SV-28 | Crash after the history files of a `move_to`, before the rename (hook); then the same move with the same `save_id` | Rolls forward: `written`; the file is in the target folder; not `replayed` |
| SV-29 | A save S gets `Conflict` at step 12 (hook); then another save is written; then S again with its `save_id` and its old `if_match` | `Conflict` from the normal path; never `replayed`; the content of S never becomes current |

### 5.36 Conformance suite: PostgreSQL tests

| Id | Test | Pass criterion |
|---|---|---|
| SP-01 | `serve` on an empty database | Stops with the message that names `hune migrate-store` |
| SP-02 | Schema version above the binary (row changed by the test); test binary with M = 2 on a database at 1 | Stops with the message "newer"; stops with the message "older" |
| SP-03 | Database encoding `SQL_ASCII` | Stops with the encoding message |
| SP-04 | `migrate-store` on an empty database, two times | Version 1; second run: "nothing to do" |
| SP-05 | Two `migrate-store` at the same time | One applies; the other exits with code 1 on the lock |
| SP-06 | A step that fails (test step) | `ROLLBACK`; the version does not change; exit code 1 |
| SP-07 | Direct inserts that break a `CHECK` (bad uid, bad etag, document of 4 MiB plus 1 byte, `delete` with a document) | PostgreSQL refuses each one |
| SP-08 | Delete a folder with a dashboard | The foreign key gives `FolderNotEmpty` |
| SP-09 | PostgreSQL down at start, then available | `serve` waits, not ready; then checks the schema and loads the catalog |
| SP-10 | The TCP proxy closes the connection during a save | `Unavailable`; the connection leaves the pool; the save with the same `save_id` gives one version |
| SP-11 | `get` on a pooled connection that is broken | Sent again on a new connection; it succeeds |
| SP-12 | A write on a node in recovery (`25006`) | `Unavailable`; the connection leaves the pool |
| SP-13 | A save while another session holds the row with `FOR UPDATE` | `Unavailable` after `statement_timeout` (`57014`) |
| SP-14 | Two instances; a save on A | Visible on B after at most `poll_interval` plus 1 s |
| SP-15 | TLS with the right CA; with a wrong CA; with a wrong `server_name` | Connects; connection refused; connection refused |
| SP-16 | Order of the uids `a-b`, `a0`, `ab` | `load` gives the byte order: `a-b`, `a0`, `ab` |
| SP-17 | `serve` with the owner role; with a member of the owner role; with a `NOINHERIT` member of the owner role; with a superuser; with the application role | Stops with the message of 5.29 step 6 four times; starts with the application role |
| SP-18 | `serve` with a wrong password; with an absent database; with a wrong CA; with PostgreSQL in recovery, then primary | `server.start_failed` and exit code 1 three times; for the last case, `serve` waits and then starts |
| SP-19 | A row whose `etag` differs from the SHA-256 of `document` (hook) | `load` and `get` give an invalid entry with one error `not_permitted` |

### 5.37 `hune copy-store`

`hune copy-store --from <a.hcl> --to <b.hcl>` copies a store into an empty store, of the same kind or of the other kind.

1. Read and check both configuration files (section 13).
2. Open the source. A volume source is read without the lock, with the rule of 5.38, item 3.
3. Open the target. A volume target takes the lock (5.16). A PostgreSQL target does the checks of 5.29.
4. Read the folders and the current entries of the source. When one is invalid, print the list and stop with exit code 1. Nothing is written.
5. Call `import_begin` on the target. When the target is not empty, stop with exit code 1.
6. Copy the folders. Then copy the record of each uid: its versions from the lowest number to the highest, and its current document.
7. Commit. A PostgreSQL target commits one transaction. A volume target has its files in place.
8. Verify: read both stores again and compare each folder, each version (metadata and bytes) and each current document. A difference stops the command with exit code 1.
9. Print the counts of folders, dashboards and versions. Exit with code 0.

After a failure, a volume target can hold a partial copy. The message says so, and the operator removes the target directory before the next attempt.

### 5.38 `hune export`

`hune export -c <file> --to <dir>` writes the whole store as a volume layout.

1. `<dir>` MUST be an absolute path. It MUST be absent or an empty directory, and its parent MUST exist.
2. A PostgreSQL source reads in one transaction at `REPEATABLE READ` (5.27). The export is a consistent copy at one instant.
3. A volume source is read without the lock, so `serve` can run. For each uid, the export lists the versions, reads them, reads the current file, then lists the versions again. When the two lists differ, it reads the uid again. After 3 attempts, it stops with exit code 1.
4. The source can hold an invalid folder or an invalid current document. Then the export prints the list and stops with exit code 1 before it writes.
5. The export writes the files through the volume backend (3.4, R4). It writes each folder with its canonical `_folder.json`, and each current document. It writes each version as `.history/<uid>/<n>.json` and `<n>.meta.json`, or as `<n>.meta.json` only for a version without bytes (5.11, rule 2). It writes no `.hune/`.
6. It prints the counts and exits with code 0.

The export format is the volume layout of 5.9 to 5.11. The directory is a valid volume root: `serve` can use it, and `hune import` reads it. The bytes of each file equal the stored bytes. The export of one document in the editor gives the canonical bytes of that document (section 10).

### 5.39 `hune import`

`hune import -c <file> --from <dir>` reads a volume layout into the empty store of the configuration.

1. Read `<dir>` with the rules of a read-only volume (5.10 to 5.12). The source takes no lock and follows no symbolic link.
2. Check everything before any write. Each folder is valid. Each current document is valid with the uid of its file. Each metadata file is valid (5.11). The SHA-256 of each `<n>.json` equals its `etag`.
3. When a check fails, print all errors and stop with exit code 1. Nothing is written.
4. Open the target. A PostgreSQL target does the checks of 5.29. A volume target takes the lock.
5. Call `import_begin`. When the target is not empty, stop with exit code 1.
6. Write the folders, then each uid: its versions from the lowest number to the highest, then its current document. A PostgreSQL target writes one transaction.
7. Verify as 5.37, item 8. Print the counts and exit with code 0.

The import keeps all versions, also when a uid has more than `history_keep` versions. The retention applies at the next write on that uid. A version number gap from an earlier retention stays as it is.

### 5.40 `hune restore`

`hune restore -c <file> --uid <uid> [--version <n>]` restores a deleted dashboard.

1. Read and check the file. Open the store. On a volume, the command takes the lock, so `serve` MUST be stopped.
2. When the uid has a current dashboard, stop with exit code 1: `the dashboard exists; restore a version in the editor`.
3. Without `--version`, n is the highest version of the uid that has a document.
4. Call `Store::restore(uid, n, None, meta)` with `saved_by` equal to `-` and the message `restore of version n by hune restore`.
5. Print the new version number and the etag. Exit with code 0.

An error of 5.4 stops the command with exit code 1 and the error text. A dashboard that exists gets a restore of a version from the editor (section 10).

### 5.41 Backup and restore

| Backend | Backup | Restore |
|---|---|---|
| Both | `hune export -c <file> --to <dir>`, for example each day from a job. Keep `<dir>` with the other backups. | Stop the writers. Run `hune import -c <file> --from <dir>` into an empty store. |
| Volume | A file copy of the root (for example `rsync -a`, a file system snapshot, or a git commit). Each write is a rename, so each file of a live copy is complete. Stop `serve` to get a copy of the whole root at one instant. | Stop `serve`. Copy the files back with the owner 65532 and the modes of 5.20. Start `serve`. |
| PostgreSQL | The backups of CloudNativePG: base backup and WAL archive | The recovery of CloudNativePG into a new cluster. Then start `serve`, which checks the schema (5.29). |

1. The operator schedules `hune export` each day, for example with a Kubernetes CronJob. Section 17 gives no CronJob manifest.
2. CI runs a restore drill: export, import into the other backend, export again, byte equality (SC-44 and section 16).
3. Each restore procedure is a manual step of the operator.

### 5.42 The stores of the process

A file names one store or more. Each block `storage.volume "<id>"` is one volume store. The block `storage.postgres` is the PostgreSQL store. The type `Stores` of `hune-store` holds them behind the `Store` trait (5.3).

1. The primary store is the store with `primary = true`. At most one store has it. It is the only store that accepts a write.
2. A file with no primary store has no edit. The nine write routes answer `403 editing_disabled`, and the page shows no edit control. Hune publishes no `dashboard.write` and no `source.query`, and `GET /api/sources` is empty (section 11, section 13).
3. The id of a volume store is its label. The id of the PostgreSQL store is `postgres`. A volume cannot have the label `postgres`.
4. Each other store is read-only. A read-only volume takes no lock and writes no file (5.19). A read-only PostgreSQL store sends no write to the database.
5. A write whose target is in a read-only store returns `ReadOnly`. The target is the store of the folder or of the uid. A move needs the primary store for the dashboard and for the target folder.
6. The content of the primary store shows at the root of the page. Each read-only store shows as one root folder. The `title` of the store is the name of this folder, and the folders of the store are below it.
7. A folder id is unique across all stores. A dashboard uid is unique across all stores. The title of a read-only store is not the title of a folder of the primary store. A `create_folder` or a `rename_folder` with such a title returns `Exists`.
8. Only the primary store has history. `history` of a dashboard of a read-only store gives an empty list. `version` gives `VersionNotFound`.
9. Each store has its own period of poll. The poller wakes at the shortest period, and `Stores` polls a store when its period has passed.
10. A command of the command line works on the primary store only. A file with no primary store gives an error. `migrate-store` needs `storage.postgres` as the primary store.
11. A primary volume that more than one instance writes needs a volume that the instances share, with the lock of 5.16. A read-only volume can be a copy for each instance. The PostgreSQL store is shared by all instances.

### 5.43 Stores: checks at start

The start stops with exit code 1 in each of these cases. The message names the store, and the file or the ids, and the reason.

1. A volume store breaks a rule of 5.21.
2. A file of a volume cannot be read, or it breaks the format. This holds for a dashboard file, a folder file and a uid that is twice in the volume. This rule applies to the primary volume too.
3. A read-only store has no dashboard. The primary store can be empty, so a person can create the first dashboard.
4. A folder id exists in two stores.
5. A dashboard uid exists in two stores.
6. A folder of the primary store has the title of a read-only store.
7. A label, a path or a title that two stores share, and two paths where one holds the other (section 13).

A foreign entry in a volume stays a data error: Hune ignores it and writes `store.entry_ignored` (5.10). The PostgreSQL store keeps a row that is not valid as an invalid entry (5.8, rule 5). Hune writes one line `store.loaded` for each store, with its label, its folders, its dashboards and its invalid entries.

After the start, a folder id or a uid that a second store adds is not taken. The catalog keeps the entry of the store that holds the id, and Hune writes `store.rescan_rejected`.

### 5.44 Stores: a file that is not valid after the start

At start, each file of a volume is valid (5.43). After the start, a person or a tool can change a file, for example with `git pull`.

1. When a file becomes not valid, the catalog keeps the last valid state of the dashboard. A folder file follows the same rule.
2. Hune writes the log line `store.rescan_rejected` one time for each change of the file. The line holds the store, the path and the reason. It stays in the log: the API never gives it.
3. The API gives the member `stale` with the code `file_invalid` for this dashboard (section 6). The page shows a warning. Each subject that can read the dashboard sees the member.
4. A file that is new and not valid has no last valid state. The catalog ignores it, and Hune writes the log line one time.
5. The gauge `hune_store_invalid_files` with the label `store` counts the files of a volume that are not valid now. It counts the stale entries, the new files and the folders.
6. A file that becomes valid again clears the member `stale` and the count at the next poll.
7. A file that is deleted leaves the catalog. The rule keeps no state for it.
8. A dashboard of the PostgreSQL store stays an invalid entry (5.8, rule 5). The rule of this section is for volumes.
