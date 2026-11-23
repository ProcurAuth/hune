//! The canonical form (4.38), the etag (4.39), the worked example (4.40), the folder file
//! (4.41), the grammars of 4.4 and the typed accessors of the model.

use hune_core::document::{
    FolderFile, MAX_ABSOLUTE_MS, Offset, PanelData, PanelOptions, SqlOwner, TextEnum, TimeText, TimeUnit, VariableKind,
    folder_file_bytes, parse_duration_ms, parse_folder_file, parse_time_text, validate,
};
use hune_core::error::ErrorCode;
use hune_core::etag::Etag;
use hune_core::ids::Uid;
use hune_core::limits::*;
use serde_json::Value;

const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");
const ALL_TYPES_COMPACT: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.compact.json");
const MINIMAL: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/minimal.json");
const ETAG: &str = "50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501";

fn uid(s: &str) -> Uid {
    Uid::parse(s).unwrap()
}

// -------------------------------------------------------- worked example

#[test]
fn worked_example_passes() {
    let doc = validate(NODES, Some(&uid("nodes"))).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(NODES.len(), 8397);
    assert_eq!(doc.bytes(), NODES);
    assert_eq!(doc.etag().to_hex(), ETAG);
    assert_eq!(doc.etag(), &Etag::of(NODES));
    assert_eq!(doc.uid().as_str(), "nodes");
    assert_eq!(doc.title(), "Nodes");
    assert_eq!(doc.format_version(), 3);
    assert_eq!(doc.document().title(), "Nodes");
}

#[test]
fn worked_example_ends_with_one_line_feed() {
    assert!(NODES.ends_with(b"}\n"));
    assert!(!NODES.ends_with(b"\n\n"));
    assert!(!NODES.starts_with(&[0xEF, 0xBB, 0xBF]));
}

fn same_as_worked_example(bytes: &[u8]) {
    let doc = validate(bytes, Some(&uid("nodes"))).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(doc.bytes(), NODES);
    assert_eq!(doc.etag().to_hex(), ETAG);
}

#[test]
fn minified_layout_gives_the_same_bytes() {
    let v: Value = serde_json::from_slice(NODES).unwrap();
    // serde_json writes the members of an object in the order of their names: not table order.
    let minified = serde_json::to_vec(&v).unwrap();
    assert!(minified.len() < 6000);
    same_as_worked_example(&minified);
}

#[test]
fn other_indents_and_line_ends_give_the_same_bytes() {
    let v: Value = serde_json::from_slice(NODES).unwrap();
    for indent in [&b"\t"[..], b"    ", b" "] {
        let mut out = Vec::new();
        let fmt = serde_json::ser::PrettyFormatter::with_indent(indent);
        let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
        serde::Serialize::serialize(&v, &mut ser).unwrap();
        same_as_worked_example(&out);
    }
    let text = String::from_utf8(NODES.to_vec()).unwrap();
    same_as_worked_example(text.replace('\n', "\r\n").as_bytes());
    same_as_worked_example(format!("  \n\t{text}  \n\n").as_bytes());
    same_as_worked_example(text.replace("\": ", "\" :   ").replace(",\n", " ,\n\n").as_bytes());
}

#[test]
fn members_in_another_order_give_the_same_bytes() {
    // Reverse the order of the top-level members and of the members of the first panel by text.
    let v: Value = serde_json::from_slice(NODES).unwrap();
    let Value::Object(top) = &v else { panic!() };
    let mut names: Vec<&String> = top.keys().collect();
    names.reverse();
    let parts: Vec<String> = names
        .iter()
        .map(|n| format!("{}:{}", serde_json::to_string(n).unwrap(), serde_json::to_string(&top[*n]).unwrap()))
        .collect();
    let reversed = format!("{{{}}}", parts.join(","));
    same_as_worked_example(reversed.as_bytes());
}

#[test]
fn escapes_in_the_input_give_the_same_bytes() {
    let text = String::from_utf8(NODES.to_vec()).unwrap();
    // The letter W of "Weight" as W, the slash as \/, and a raw non-ASCII letter kept.
    let changed = text.replacen("\"Weight over time\"", "\"\\u0057eight over time\"", 1).replacen(
        "https://nodes",
        "https:\\/\\/nodes",
        1,
    );
    assert_ne!(changed, text);
    same_as_worked_example(changed.as_bytes());
}

#[test]
fn canonical_bytes_are_a_fixed_point() {
    for (bytes, uid_text) in [(NODES, "nodes"), (ALL_TYPES, "all-types"), (MINIMAL, "minimal")] {
        let first = validate(bytes, Some(&uid(uid_text))).unwrap();
        assert_eq!(first.bytes(), bytes, "{uid_text}");
        let second = validate(first.bytes(), None).unwrap();
        assert_eq!(first.bytes(), second.bytes());
        assert_eq!(first.etag(), second.etag());
        assert_eq!(first.document(), second.document());
    }
}

#[test]
fn compact_input_gives_the_golden_file() {
    let doc = validate(ALL_TYPES_COMPACT, Some(&uid("all-types"))).unwrap();
    assert_eq!(doc.bytes(), ALL_TYPES);
    assert!(ALL_TYPES_COMPACT.len() < ALL_TYPES.len() / 2);
}

#[test]
fn numbers_in_the_golden_file_have_the_canonical_form() {
    let compact = String::from_utf8(ALL_TYPES_COMPACT.to_vec()).unwrap();
    // The input spells the numbers of the table of 4.38. The golden file has the canonical text.
    for spelling in ["1.50", "1e3", "-0", "0.000001", "1e-7", "1e21", "123456789012345678901234567890"] {
        assert!(compact.contains(spelling), "{spelling}");
    }
    let golden = String::from_utf8(ALL_TYPES.to_vec()).unwrap();
    for text in [": 1.5,", ": 1000,", ": 0,", ": 0.000001,", ": 1e-7,", ": 1e+21,", ": 1.2345678901234568e+29,"] {
        assert!(golden.contains(text), "{text}");
    }
    for text in ["1.50", "1e3", "-0", "1e21", "123456789012345678901234567890"] {
        assert!(!golden.contains(text), "{text}");
    }
}

#[test]
fn integers_have_no_plus_and_no_leading_zero_in_the_output() {
    let golden = String::from_utf8(ALL_TYPES.to_vec()).unwrap();
    assert!(!golden.contains(": +"));
    assert!(!golden.contains(": 00"));
    assert!(!golden.contains(": -0,"));
}

#[test]
fn strings_are_escaped_by_rule_7() {
    let text = String::from_utf8(MINIMAL.to_vec()).unwrap();
    let title = "q\"b\\s/é\u{1F600}";
    let changed = text.replacen("\"Minimal\"", &serde_json::to_string(title).unwrap(), 1);
    let doc = validate(changed.as_bytes(), None).unwrap();
    let out = String::from_utf8(doc.bytes().to_vec()).unwrap();
    assert!(out.contains("\"title\": \"q\\\"b\\\\s/é\u{1F600}\""), "{out}");
    assert_eq!(doc.title(), title);
}

#[test]
fn a_tab_in_sql_is_written_as_backslash_t() {
    let doc = validate(ALL_TYPES, None).unwrap();
    let out = String::from_utf8(doc.bytes().to_vec()).unwrap();
    assert!(out.contains("\"\\tFROM t -- tab above\""));
    assert!(!out.contains('\t'));
}

#[test]
fn property_set_writes_only_its_members_in_table_order() {
    let text = String::from_utf8(NODES.to_vec()).unwrap();
    // Order in the input: width, decimals, unit (reverse of the table).
    let changed = text.replacen(
        "\"unit\": \"suffix: pts\",\n                  \"decimals\": 2,\n                  \"width\": 140",
        "\"width\": 140, \"decimals\": 2, \"unit\": \"suffix: pts\"",
        1,
    );
    assert_ne!(changed, text);
    same_as_worked_example(changed.as_bytes());
}

// ----------------------------------------------------------- size limit

#[test]
fn canonical_form_above_the_limit_is_too_large() {
    let text = String::from_utf8(MINIMAL.to_vec()).unwrap();
    let panel = format!(
        "{{\"id\":1,\"type\":\"stat\",\"title\":\"\",\"description\":\"\",\"transparent\":false,\"grid\":{{\"x\":0,\"y\":0,\"w\":1,\"h\":1}},\
         \"data\":{{\"query\":{{\"source\":\"a\",\"format\":\"table\",\"sql\":[{}],\"min_interval\":null,\"max_data_points\":null}}}},\
         \"transforms\":[],\"fields\":{{\"defaults\":{{}},\"overrides\":[]}},\
         \"options\":{{\"reduce\":{{\"calc\":\"last\",\"fields\":null,\"values\":false}},\"color_mode\":\"none\",\"graph_mode\":\"none\",\"text_mode\":\"auto\",\"orientation\":\"auto\"}},\
         \"links\":[]}}",
        vec!["\"\""; 262_145].join(",")
    );
    let big = text.replacen("\"panels\": []", &format!("\"panels\": [{panel}]"), 1);
    assert!(big.len() < MAX_DOCUMENT_BYTES);
    let e = validate(big.as_bytes(), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::TooLarge, ""));
    // With 150,000 lines the canonical form fits.
    let ok = big.replacen(&vec!["\"\""; 262_145].join(","), &vec!["\"\""; 150_000].join(","), 1);
    assert!(validate(ok.as_bytes(), None).is_ok());
}

// -------------------------------------------------------- never panics

#[test]
fn every_prefix_of_the_worked_example_is_refused_without_a_panic() {
    // The prefix without the final line feed is a valid document: stop before it.
    for len in 0..NODES.len() - 1 {
        let result = validate(&NODES[..len], None);
        assert!(result.is_err(), "prefix {len}");
    }
}

#[test]
fn byte_changes_never_panic() {
    for i in (0..NODES.len()).step_by(7) {
        for replacement in [b'"', b'{', b'}', b'[', b']', b',', b':', b'\\', 0x00, 0xFF, b'9', b'-'] {
            let mut bytes = NODES.to_vec();
            bytes[i] = replacement;
            let _ = validate(&bytes, None);
        }
    }
}

// ----------------------------------------------------- typed accessors

#[test]
fn typed_accessors() {
    let doc = validate(NODES, None).unwrap();
    let d = doc.document();
    assert_eq!(d.uid().as_str(), "nodes");
    assert_eq!(d.description, "Weight and status of the nodes.");
    assert_eq!(d.tags, ["chain"]);
    assert_eq!(d.time.from, "now-24h");
    assert_eq!(d.time.refresh.as_deref(), Some("1m"));
    assert_eq!(d.sections().len(), 2);
    assert_eq!(d.sections()[0].title, None);
    assert_eq!(d.sections()[1].title.as_deref(), Some("Details"));
    assert_eq!(d.variables().len(), 3);
    let ids: Vec<(usize, u32)> = d.panels().map(|(s, p)| (s, p.id)).collect();
    assert_eq!(ids, [(0, 1), (0, 3), (1, 2)]);
    assert!(d.panel(99).is_none());
    let table = d.panel(2).unwrap();
    assert_eq!(table.panel_type.as_str(), "table");
    assert_eq!(table.fields.overrides.len(), 4);
    assert!(matches!(table.options, PanelOptions::Table(_)));
    assert_eq!(table.options.panel_type(), table.panel_type);
    assert_eq!(table.transforms.len(), 1);
    let stat = d.panel(3).unwrap();
    assert!(matches!(stat.data, PanelData::Reuse { panel: 2, with_transforms: true }));
    let ts = d.panel(1).unwrap();
    let PanelData::Query(q) = &ts.data else { panic!() };
    assert_eq!(q.source.as_str(), "prod");
    assert_eq!(q.sql.len(), 9);
    assert_eq!(q.sql_text().lines().count(), 9);
    assert!(q.sql_text().starts_with("SELECT\n  $__timeInterval(ts) AS t,\n"));
    assert!(!q.sql_text().ends_with('\n'));
    assert_eq!(q.min_interval_ms(), Some(60_000));
    assert_eq!(q.max_data_points, None);
    let v = d.variable("node").unwrap();
    let VariableKind::Query { source, multi, include_all, all_value, default, .. } = &v.kind else { panic!() };
    assert_eq!(source.as_str(), "prod");
    assert!(*multi && *include_all);
    assert_eq!(*all_value, None);
    assert_eq!(default, &["$__all"]);
    assert!(matches!(d.variable("account").unwrap().kind, VariableKind::Text { .. }));
    assert!(d.variable("nope").is_none());
}

#[test]
fn sql_texts_with_their_pointers() {
    let doc = validate(NODES, None).unwrap();
    let texts = doc.document().sql_texts();
    let pointers: Vec<&str> = texts.iter().map(|t| t.pointer.as_str()).collect();
    assert_eq!(
        pointers,
        ["/variables/0/sql", "/sections/0/panels/0/data/query/sql", "/sections/1/panels/0/data/query/sql"]
    );
    assert_eq!(texts[0].owner, SqlOwner::Variable { name: "node" });
    assert_eq!(texts[1].owner, SqlOwner::Panel { id: 1 });
    assert_eq!(texts[2].owner, SqlOwner::Panel { id: 2 });
    let text0 = texts[0].text();
    assert!(text0.starts_with("SELECT DISTINCT node\nFROM "));
    assert!(text0.ends_with(".nodes\nORDER BY node"));
    assert_eq!(texts[0].line_pointer(2), "/variables/0/sql/2");
    assert_eq!(texts[1].line_pointer(5), "/sections/0/panels/0/data/query/sql/5");
    assert_eq!(texts[2].source.as_str(), "prod");
    assert_eq!(texts[2].lines.len(), 5);
    // Each pointer leads to the array of lines in the document.
    let tree: Value = serde_json::from_slice(doc.bytes()).unwrap();
    for t in &texts {
        let at = tree.pointer(&t.pointer).unwrap().as_array().unwrap();
        assert_eq!(at.len(), t.lines.len());
        assert_eq!(at[0].as_str().unwrap(), t.lines[0]);
    }
}

#[test]
fn sql_pointers_in_the_golden_file() {
    let doc = validate(ALL_TYPES, None).unwrap();
    let texts = doc.document().sql_texts();
    // One variable, and six panels with their own query (the stat panel reuses).
    assert_eq!(texts.len(), 1 + 6);
    assert_eq!(texts[0].pointer, "/variables/0/sql");
    assert_eq!(texts[1].pointer, "/sections/0/panels/0/data/query/sql");
    assert_eq!(texts[2].pointer, "/sections/0/panels/1/data/query/sql");
    assert_eq!(texts[3].pointer, "/sections/1/panels/0/data/query/sql");
}

#[test]
fn every_text_enum_round_trips() {
    use hune_core::document::{Hide, PanelType, ReduceCalc, VariableSort};
    fn check<E: TextEnum + PartialEq + std::fmt::Debug>() {
        for (i, name) in E::NAMES.iter().enumerate() {
            let v = E::parse(name).unwrap();
            assert_eq!(v.as_str(), *name);
            assert_eq!(E::parse(&format!("{name}x")), None);
            assert!(i < E::NAMES.len());
        }
    }
    check::<Hide>();
    check::<PanelType>();
    check::<ReduceCalc>();
    check::<VariableSort>();
    assert_eq!(PanelType::ALL.len(), 15);
}

// --------------------------------------------------- the grammar of 4.4

#[test]
fn table_of_4_4_time_texts() {
    let table: [(&str, bool); 10] = [
        ("now", true),
        ("now-6h", true),
        ("now/d", true),
        ("now-1d/d", true),
        ("1731100000000", true),
        ("now-0h", false),
        ("now+1h", false),
        ("now-6H", false),
        ("01731100000000", false),
        ("4294967296000", false),
    ];
    for (text, valid) in table {
        assert_eq!(parse_time_text(text).is_some(), valid, "{text}");
    }
    assert_eq!(parse_time_text("0"), Some(TimeText::Absolute(0)));
    assert_eq!(parse_time_text("4294967295999"), Some(TimeText::Absolute(MAX_ABSOLUTE_MS)));
    assert_eq!(
        parse_time_text("now-6h"),
        Some(TimeText::Relative { offset: Some(Offset { count: 6, unit: TimeUnit::Hour }), round: None })
    );
    assert_eq!(parse_time_text("now/d"), Some(TimeText::Relative { offset: None, round: Some(TimeUnit::Day) }));
    assert_eq!(parse_time_text("now"), Some(TimeText::Relative { offset: None, round: None }));
    for unit in ["s", "m", "h", "d", "w", "M", "y"] {
        assert!(parse_time_text(&format!("now-5{unit}/{unit}")).is_some());
    }
    assert!(parse_time_text("now-5ms").is_none());
}

#[test]
fn table_of_4_4_durations() {
    assert_eq!(parse_duration_ms("90s"), Some(90_000));
    assert_eq!(parse_duration_ms("1.5m"), None);
    assert_eq!(parse_duration_ms("1m30s"), None);
    assert_eq!(parse_duration_ms("1ms"), Some(1));
    assert_eq!(parse_duration_ms("1m"), Some(60_000));
    assert_eq!(parse_duration_ms("1h"), Some(3_600_000));
    assert_eq!(parse_duration_ms("1d"), Some(86_400_000));
    assert_eq!(parse_duration_ms("999999999s"), Some(999_999_999_000));
    assert_eq!(parse_duration_ms("1000000000s"), None);
    assert_eq!(parse_duration_ms("0s"), None);
    assert_eq!(parse_duration_ms("01s"), None);
    assert_eq!(parse_duration_ms("1w"), None);
}

// ------------------------------------------------------------ folder file

#[test]
fn folder_file_example_of_4_41() {
    let bytes = folder_file_bytes("Transit desk");
    assert_eq!(bytes.len(), 53);
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        "{\n  \"format_version\": 1,\n  \"title\": \"Transit desk\"\n}\n"
    );
    assert_eq!(parse_folder_file(&bytes).unwrap(), FolderFile { title: "Transit desk".to_owned() });
}

#[test]
fn folder_file_accepts_any_layout_and_order() {
    for text in ["{\"title\":\"A\",\"format_version\":1}", "\n {  \"format_version\" : 1 ,\r\n \"title\" : \"A\" } \n"]
    {
        assert_eq!(parse_folder_file(text.as_bytes()).unwrap().title, "A");
    }
}

#[test]
fn folder_file_errors() {
    let cases: [(&str, &str, ErrorCode); 12] = [
        ("{\"format_version\": 1}", "", ErrorCode::MissingMember),
        ("{\"format_version\": 1, \"title\": \"\"}", "/title", ErrorCode::OutOfRange),
        ("{\"format_version\": 1, \"title\": 5}", "/title", ErrorCode::WrongType),
        ("{\"format_version\": 1, \"title\": \"a\", \"extra\": 1}", "/extra", ErrorCode::UnknownMember),
        ("{\"format_version\": 1, \"title\": \"a\\u0001\"}", "/title", ErrorCode::BadFormat),
        ("{\"title\": \"a\"}", "/format_version", ErrorCode::FormatVersion),
        ("{\"format_version\": 2, \"title\": \"a\"}", "/format_version", ErrorCode::FormatVersion),
        ("{\"format_version\": \"1\", \"title\": \"a\"}", "/format_version", ErrorCode::FormatVersion),
        ("{\"format_version\": 1, \"title\": \"a\", \"title\": \"b\"}", "/title", ErrorCode::DuplicateMember),
        ("{\"format_version\": 1, \"title\": ", "", ErrorCode::Syntax),
        ("[]", "", ErrorCode::WrongType),
        ("{\"format_version\": 1, \"title\": \"a\"} x", "", ErrorCode::Syntax),
    ];
    for (text, pointer, code) in cases {
        let e = parse_folder_file(text.as_bytes()).unwrap_err();
        assert_eq!(e.len(), 1, "{text}: {e:?}");
        assert_eq!((e[0].pointer.as_str(), e[0].code), (pointer, code), "{text}");
    }
    let e = parse_folder_file(&[0xEF, 0xBB, 0xBF, b'{', b'}']).unwrap_err();
    assert_eq!(e[0].code, ErrorCode::Encoding);
    let e = parse_folder_file(b"{\"format_version\": 2, \"title\": \"a\"}").unwrap_err();
    assert_eq!(e[0].message, "not loadable: format 2");
}

#[test]
fn folder_title_length_in_characters() {
    let ok = folder_file_bytes(&"é".repeat(256));
    assert_eq!(parse_folder_file(&ok).unwrap().title.chars().count(), 256);
    let long = folder_file_bytes(&"é".repeat(257));
    assert_eq!(parse_folder_file(&long).unwrap_err()[0].code, ErrorCode::OutOfRange);
}

#[test]
fn folder_file_bytes_escape_control_characters_in_lower_case() {
    let bytes = folder_file_bytes("a\u{0}\u{1f}\u{8}\u{c}\n\r\t\"\\/\u{7f}b");
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"a\\u0000\\u001f\\b\\f\\n\\r\\t\\\"\\\\/\u{7f}b\""), "{text}");
    assert!(text.ends_with("\"\n}\n"));
}

#[test]
fn folder_file_is_closed_and_canonical() {
    let a = folder_file_bytes("x");
    let b = folder_file_bytes("x");
    assert_eq!(a, b);
    assert_eq!(Etag::of(&a), Etag::of(&b));
    assert_ne!(Etag::of(&a), Etag::of(&folder_file_bytes("y")));
}

// ------------------------------------------------------- member `transparent`

#[test]
fn transparent_is_written_after_description_in_every_panel() {
    let text = String::from_utf8(ALL_TYPES.to_vec()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let mut panels = 0;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("          \"id\": ") {
            panels += 1;
            // The member order of 4.13: id, type, title, description, transparent, grid.
            assert!(lines[i + 1].starts_with("          \"type\": "), "{line}");
            assert!(lines[i + 2].starts_with("          \"title\": "), "{line}");
            assert!(lines[i + 3].starts_with("          \"description\": "), "{line}");
            assert_eq!(lines[i + 4], "          \"transparent\": false,", "{line}");
            assert_eq!(lines[i + 5], "          \"grid\": {", "{line}");
        }
    }
    assert_eq!(panels, 7);
}

#[test]
fn transparent_true_round_trips() {
    let text = String::from_utf8(MINIMAL.to_vec()).unwrap();
    let panel = "{\"id\":1,\"type\":\"stat\",\"title\":\"\",\"description\":\"\",\"transparent\":true,\"grid\":{\"x\":0,\"y\":0,\"w\":1,\"h\":1},\
        \"data\":{\"query\":{\"source\":\"a\",\"format\":\"table\",\"sql\":[\"SELECT 1\"],\"min_interval\":null,\"max_data_points\":null}},\
        \"transforms\":[],\"fields\":{\"defaults\":{},\"overrides\":[]},\
        \"options\":{\"reduce\":{\"calc\":\"last\",\"fields\":null,\"values\":false},\"color_mode\":\"none\",\"graph_mode\":\"none\",\"text_mode\":\"auto\",\"orientation\":\"auto\"},\
        \"links\":[]}";
    let input = text.replacen("\"panels\": []", &format!("\"panels\": [{panel}]"), 1);
    let doc = validate(input.as_bytes(), None).unwrap_or_else(|e| panic!("{e:#?}"));
    let out = String::from_utf8(doc.bytes().to_vec()).unwrap();
    assert!(out.contains("          \"transparent\": true,\n          \"grid\": {"), "{out}");
    assert!(doc.document().sections[0].panels[0].transparent);
    assert_eq!(validate(doc.bytes(), None).unwrap().bytes(), doc.bytes());
}
