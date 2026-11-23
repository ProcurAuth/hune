//! The limits of format 3 (4.35). They are constants, not configuration keys.
//!
//! A document that is valid on one instance is valid on all instances and in both backends.

/// The largest input and canonical document, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 4_194_304;
/// The largest depth of objects and arrays. The top object has depth 1.
pub const MAX_JSON_DEPTH: usize = 32;
/// The largest joined SQL text, in bytes (4.16).
pub const MAX_SQL_BYTES: usize = 262_144;
/// The largest number of lines in the `options.content` of a text panel (4.33a, 4.35).
pub const MAX_TEXT_LINES: usize = 2_000;
/// The largest length of one line of the `options.content` of a text panel, in bytes (4.33a, 4.35).
pub const MAX_TEXT_LINE_BYTES: usize = 8_192;
/// The largest joined text of the `options.content` of a text panel, in bytes (4.33a, 4.35).
pub const MAX_TEXT_BYTES: usize = 65_536;
/// The largest number of panels in one document.
pub const MAX_PANELS: usize = 200;
/// The largest number of sections in one document.
pub const MAX_SECTIONS: usize = 50;
/// The default of `server.repeat_max_per_panel`: the largest number of copies of one panel that repeats (4.13a).
/// This is a run time limit: the validation of 4.36 does not check it. The page checks it (9.8c).
pub const MAX_REPEAT_PER_PANEL: usize = 24;
/// The default of `server.repeat_max_per_section`: the largest number of copies of one section that repeats.
pub const MAX_REPEAT_PER_SECTION: usize = 12;
/// The default of `server.repeat_max_per_dashboard`: the largest number of copies of panels that repeat in
/// one dashboard.
pub const MAX_REPEAT_PER_DASHBOARD: usize = 120;
/// The hard upper bound of `server.repeat_max_per_panel`. It is `MAX_PANELS`: no document needs more.
pub const REPEAT_BOUND_PER_PANEL: usize = 200;
/// The hard upper bound of `server.repeat_max_per_section`. It is `MAX_SECTIONS`.
pub const REPEAT_BOUND_PER_SECTION: usize = 50;
/// The hard upper bound of `server.repeat_max_per_dashboard`. It is `MAX_PANELS`.
pub const REPEAT_BOUND_PER_DASHBOARD: usize = 200;
/// The largest number of variables in one document.
pub const MAX_VARIABLES: usize = 64;
/// The largest number of items in the member `intervals` of one `interval` variable (4.10a, 4.35).
pub const MAX_INTERVALS: usize = 50;
/// The largest number of items in the member `keys` of one `adhoc` variable (4.10c, 4.35).
pub const MAX_ADHOC_KEYS: usize = 64;
/// The largest number of filters of one `adhoc` variable in one query request (7.7a, 4.35).
pub const MAX_ADHOC_FILTERS: usize = 16;
/// The largest number of overrides in one panel.
pub const MAX_OVERRIDES: usize = 512;
/// The largest number of transforms in one panel.
pub const MAX_TRANSFORMS: usize = 16;
/// The largest number of filters in one `filter_by_value` transform.
pub const MAX_FILTERS: usize = 32;
/// The largest number of items in each array of the transforms of 4.25a, except the arrays that have their
/// own bound (`sort`, `reducers`, `fields` of `partition_by_values` and `conversions`) (4.35).
pub const MAX_TRANSFORM_ITEMS: usize = 64;
/// The largest number of reducers in one `reduce` transform: the number of calculations of 4.26 (4.25a).
pub const MAX_TRANSFORM_REDUCERS: usize = 18;
/// The largest number of key fields in one `partition_by_values` transform (4.25a).
pub const MAX_PARTITION_FIELDS: usize = 4;
/// The largest number of conversions in one `convert_field_type` transform (4.25a).
pub const MAX_CONVERSIONS: usize = 16;
/// The largest value of `count` in a `limit` transform (4.25a).
pub const MAX_LIMIT_COUNT: i64 = 1_000_000;
/// The largest length of a new name or of a rename text of 4.25a, in characters.
pub const MAX_TRANSFORM_NAME_CHARS: usize = 256;
/// The largest number of rows of one frame after a transform of 4.25a (4.35). It equals `MAX_JOIN_ROWS`.
/// This is a run time limit: the validation of 4.36 does not check it. The page checks it (9.21a, rule 13).
pub const MAX_TRANSFORM_ROWS: usize = 200_000;
/// The largest number of fields of one frame after a transform of 4.25a (4.35). The page checks it.
pub const MAX_TRANSFORM_FIELDS: usize = 2_000;
/// The largest number of frames after a transform of 4.25a (4.35). The page checks it.
pub const MAX_TRANSFORM_FRAMES: usize = 100;
/// The largest number of comparisons of rows in one `merge` transform (4.35). The page checks it.
pub const MAX_MERGE_STEPS: usize = 4_000_000;
/// The largest length of a title, a label or an option text, in characters.
pub const MAX_TITLE_CHARS: usize = 256;
/// The largest length of a description, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 4096;
/// The largest number of tags in one document.
pub const MAX_TAGS: usize = 32;
/// The largest length of one tag, in characters.
pub const MAX_TAG_CHARS: usize = 64;
/// The largest number of items in one `default`, and of options in one `custom` variable.
pub const MAX_VARIABLE_VALUES: usize = 10_000;
/// The largest length of a variable value, an option value, an `all_value`, a field name or a
/// matcher value, in bytes.
pub const MAX_VALUE_BYTES: usize = 1024;
/// The largest number of links in one panel, and in one property set.
pub const MAX_LINKS: usize = 16;
/// The largest number of dashboard links in one document (4.5a).
pub const MAX_DASHBOARD_LINKS: usize = 16;
/// The largest number of tags in one dashboard link of the kind `dashboards` (4.5a).
pub const MAX_LINK_TAGS: usize = 8;
/// The largest length of one zone name of `time.timezone`, in bytes (4.6).
pub const MAX_TIMEZONE_BYTES: usize = 64;
/// The largest length of one link URL template, in bytes.
pub const MAX_URL_BYTES: usize = 2048;
/// The largest number of mappings in one property set.
pub const MAX_MAPPINGS: usize = 256;
/// The largest number of steps in one thresholds object.
pub const MAX_THRESHOLD_STEPS: usize = 32;
/// The largest length of one regular expression, in bytes.
pub const MAX_PATTERN_BYTES: usize = 1024;
/// The largest number of sort keys in one table.
pub const MAX_SORT_KEYS: usize = 8;
/// The largest number of names in one matcher `by` = `names` (4.17).
pub const MAX_MATCHER_NAMES: usize = 64;
/// The largest number of calculations in one table footer (4.18).
pub const MAX_FOOTER_REDUCERS: usize = 8;
/// The values of `time.refresh` (4.6) and `refresh_choices` of section 6.
pub const REFRESH_CHOICES: [&str; 9] = ["10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"];
/// The largest number of items in `y_fields`.
pub const MAX_Y_FIELDS: usize = 32;
/// The largest number of errors in one validation result.
pub const MAX_ERRORS: usize = 200;
/// The largest number of warnings in one answer of a write or of `validate_only` (6.13, 7.18, 4.35).
pub const MAX_WARNINGS: usize = 200;
/// The largest number of annotations in one document (4.5b, 4.35).
pub const MAX_ANNOTATIONS: usize = 8;
/// The largest number of items in the member `panels` of one annotation (4.5b, 4.35). It equals `MAX_PANELS`.
pub const MAX_ANNOTATION_PANELS: usize = 200;
/// The largest number of rows in the result of one annotation query (7.37, 4.35).
pub const MAX_ANNOTATION_ROWS: usize = 1_000;
/// The largest number of marks that the annotations of one panel give together. The page enforces it (9.19d).
pub const MAX_ANNOTATIONS_PER_PANEL: usize = 2_000;
/// The largest length of the title of one mark, in characters. It equals `MAX_TITLE_CHARS` (4.35).
pub const MAX_ANNOTATION_TITLE_CHARS: usize = 256;
/// The largest length of the text of one mark, in bytes (4.35).
pub const MAX_ANNOTATION_TEXT_BYTES: usize = 2_048;
/// The largest number of tags of one mark (4.35).
pub const MAX_ANNOTATION_TAGS: usize = 8;
/// The largest length of one tag of a mark, in characters (4.35).
pub const MAX_ANNOTATION_TAG_CHARS: usize = 64;
/// The largest number of queries in one panel: `data.query` and at most 7 items of `extra_queries`
/// (4.15a, 4.35). The limit is fail closed: it is not a key.
pub const MAX_QUERIES_PER_PANEL: usize = 8;
/// The largest number of queries in one document, hidden queries included (4.15a, 4.35). It is twice `MAX_PANELS`.
pub const MAX_QUERIES: usize = 400;
