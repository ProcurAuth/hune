// SPDX-License-Identifier: MIT OR Apache-2.0
//! The rows of the text panel in 15.6 (4.33a): one test for each new row in the v1 form and in the v2
//! forms, and separate tests for the fence of a code block, the line ends, the control characters, the
//! constructs of the list under the table, the limits and the hostile text.

mod common;

use common::*;
use hune_core::limits::{MAX_TEXT_BYTES, MAX_TEXT_LINE_BYTES, MAX_TEXT_LINES};
use hune_grafana::{Class, DashboardImport, Fallback, ReportItem};
use serde_json::{Value, json};

// ------------------------------------------------------------------ helpers

fn text_v1(id: i64, y: i64, options: Value) -> Value {
    json!({
        "id": id, "type": "text", "title": format!("Text {id}"), "description": "",
        "gridPos": {"h": 4, "w": 12, "x": 0, "y": y},
        "options": options, "fieldConfig": {"defaults": {}, "overrides": []}, "pluginVersion": "12.4.2"
    })
}

fn code_default() -> Value {
    json!({"language": "plaintext", "showLineNumbers": false, "showMiniMap": false})
}

fn md(content: &str) -> Value {
    json!({"mode": "markdown", "content": content, "code": code_default()})
}

fn v2_element(form: &str, id: i64, options: Value) -> Value {
    let viz = if form == "alpha1" {
        json!({"kind": "text", "spec": {"pluginVersion": "12.4.2", "options": options, "fieldConfig": {"defaults": {}, "overrides": []}}})
    } else {
        json!({"kind": "VizConfig", "group": "text", "version": "12.4.2", "spec": {"options": options, "fieldConfig": {"defaults": {}, "overrides": []}}})
    };
    json!({"kind": "Panel", "spec": {"id": id, "title": format!("Text {id}"), "description": "", "links": [],
        "data": {"kind": "QueryGroup", "spec": {"queries": [], "transformations": [], "queryOptions": {}}}, "vizConfig": viz}})
}

fn v2_dash(form: &str, options: Value) -> Value {
    json!({
        "apiVersion": format!("dashboard.grafana.app/v2{form}"), "kind": "Dashboard",
        "metadata": {"name": "t2", "namespace": "default", "resourceVersion": "1", "annotations": {"grafana.app/folder": "fe1"}},
        "spec": {"title": "V2 test", "description": "", "tags": [], "cursorSync": "Off",
            "timeSettings": {"from": "now-6h", "to": "now", "timezone": "utc", "autoRefresh": ""},
            "variables": [], "elements": {"panel-1": v2_element(form, 1, options)},
            "layout": {"kind": "RowsLayout", "spec": {"rows": [{"kind": "RowsLayoutRow", "spec": {
                "title": "", "collapse": false, "hideHeader": true,
                "layout": {"kind": "GridLayout", "spec": {"items": [{"kind": "GridLayoutItem", "spec": {
                    "x": 0, "y": 0, "width": 12, "height": 4, "element": {"kind": "ElementReference", "name": "panel-1"}}}]}}}}]}}}
    })
}

/// Imports a v1 dashboard that has one text panel. The result holds the first run (with its items) and
/// the document of the run that accepts each item.
fn import(options: Value) -> (DashboardImport, Value) {
    let r = run(&dash(vec![text_v1(1, 0, options)], vec![]));
    let d = doc(&r);
    (r, d)
}

fn content_of(d: &Value) -> Vec<String> {
    d["sections"][0]["panels"][0]["options"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect()
}

fn content_items(r: &DashboardImport) -> Vec<&ReportItem> {
    r.items.iter().filter(|i| i.member == "options.content").collect()
}

fn no_panel(d: &Value) -> bool {
    d["sections"][0]["panels"].as_array().unwrap().is_empty()
}

const V2_FORMS: [&str; 2] = ["alpha1", "beta1"];

// --------------------------------------------------------------- the row `type` and `options.mode`

#[test]
fn a_text_panel_maps_with_the_members_of_4_33a_and_no_item() {
    let (r, d) = import(md("# Title\n\nSome *text*."));
    no_items(&r);
    let p = first_panel(&d);
    assert_eq!(p["type"], "text");
    assert_eq!(p["options"], json!({"mode": "markdown", "content": ["# Title", "", "Some *text*."]}));
    for member in ["data", "transforms", "fields", "repeat"] {
        assert!(p.get(member).is_none(), "{member}");
    }
    assert_eq!(p["title"], "Text 1");
    assert_eq!(p["links"], json!([]));
    assert_eq!(p["transparent"], false);
}

#[test]
fn the_v2_forms_give_the_same_document_as_the_v1_form() {
    let (r1, d1) = import(md("# Title\n\nSome *text*."));
    let p1 = first_panel(&d1).clone();
    for f in V2_FORMS {
        let r = run(&v2_dash(f, md("# Title\n\nSome *text*.")));
        let d = doc(&r);
        assert_eq!(first_panel(&d)["options"], p1["options"], "{f}");
        assert_eq!(first_panel(&d)["type"], "text", "{f}");
        assert!(first_panel(&d).get("data").is_none(), "{f}");
        assert_eq!(r.items.len(), r1.items.len(), "{f}: {:#?}", r.items);
    }
}

#[test]
fn a_mode_that_is_absent_or_unknown_is_changed_to_markdown() {
    // Absent: one `changed` item at the mode.
    let (r, d) = import(json!({"content": "x"}));
    assert_eq!(first_panel(&d)["options"]["mode"], "markdown");
    let item = one_item(&r, "options.mode");
    assert_eq!(item.class, Class::Changed);
    assert!(item.pointer.ends_with("/options/mode"), "{}", item.pointer);
    // The options are absent too.
    let r = run(&dash(
        vec![{
            let mut p = text_v1(1, 0, json!({}));
            p.as_object_mut().unwrap().remove("options");
            p
        }],
        vec![],
    ));
    let d = doc(&r);
    assert_eq!(first_panel(&d)["options"], json!({"mode": "markdown", "content": []}));
    assert_eq!(items_with(&r, "options.mode").len() + items_with(&r, "options.content").len(), 2, "{:#?}", r.items);
    // A text that Grafana reads as markdown.
    for mode in ["Markdown", "text", "markdown2", ""] {
        let (r, d) = import(json!({"mode": mode, "content": "x"}));
        assert_eq!(first_panel(&d)["options"]["mode"], "markdown", "{mode:?}");
        if mode.is_empty() {
            continue; // An empty text is a trivial value.
        }
        assert_eq!(one_item(&r, "options.mode").class, Class::Changed, "{mode:?}");
    }
    // `null` counts as absent.
    let (r, _) = import(json!({"mode": null, "content": "x"}));
    assert_eq!(one_item(&r, "options.mode").class, Class::Changed);
}

#[test]
fn a_mode_that_is_not_text_is_unsupported() {
    let (r, d) = import(json!({"mode": 5, "content": "x"}));
    assert_eq!(first_panel(&d)["options"]["mode"], "markdown");
    let item = one_item(&r, "options.mode");
    assert_eq!(item.class, Class::Unsupported);
    assert_eq!(item.fallback, Some(Fallback::other("the mode markdown")));
}

#[test]
fn the_mode_html_is_unsupported_and_the_content_becomes_a_code_block() {
    let (r, d) =
        import(json!({"mode": "html", "content": "<b>x</b>\n<script>alert(1)</script>", "code": code_default()}));
    assert_eq!(
        first_panel(&d)["options"],
        json!({"mode": "markdown", "content": ["```", "<b>x</b>", "<script>alert(1)</script>", "```"]})
    );
    let item = one_item(&r, "options.mode");
    assert_eq!(item.class, Class::Unsupported);
    assert_eq!(item.fallback, Some(Fallback::other("the content becomes a code block")));
    // The default `code` is metadata, and the HTML content has no construct item.
    assert!(content_items(&r).is_empty(), "{:#?}", r.items);
    assert!(items_with(&r, "options.code").is_empty());
}

#[test]
fn the_mode_code_is_changed_and_the_content_becomes_a_code_block() {
    let (r, d) = import(json!({"mode": "code", "content": "fn main() {}", "code": code_default()}));
    assert_eq!(first_panel(&d)["options"]["content"], json!(["```", "fn main() {}", "```"]));
    let item = one_item(&r, "options.mode");
    assert_eq!(item.class, Class::Changed);
    assert!(item.fallback.is_none());
}

#[test]
fn the_fence_of_a_code_block_follows_the_longest_run_of_backticks() {
    for (run_len, fence_len) in [(0, 3), (1, 3), (2, 3), (3, 4), (4, 5), (5, 6)] {
        let line = format!("a{}b", "`".repeat(run_len));
        let (_, d) = import(json!({"mode": "code", "content": line.clone()}));
        let fence = "`".repeat(fence_len);
        assert_eq!(content_of(&d), [fence.clone(), line, fence], "run of {run_len}");
    }
}

#[test]
fn an_empty_text_in_the_html_mode_stays_empty() {
    let (_, d) = import(json!({"mode": "html", "content": ""}));
    assert_eq!(first_panel(&d)["options"]["content"], json!([]));
}

// ------------------------------------------------------------------------------ `options.content`

#[test]
fn the_content_is_cut_at_the_line_end() {
    for (text, want) in [
        ("", vec![]),
        ("one", vec!["one"]),
        ("a\nb", vec!["a", "b"]),
        ("a\n\nb\n", vec!["a", "", "b", ""]),
        ("tab\there", vec!["tab\there"]),
    ] {
        let (r, d) = import(md(text));
        assert_eq!(content_of(&d), want, "{text:?}");
        assert!(content_items(&r).is_empty(), "{text:?}: {:#?}", r.items);
    }
}

#[test]
fn a_cr_lf_and_a_lone_cr_are_changed_to_line_ends() {
    let (r, d) = import(md("a\r\nb\rc\nd"));
    assert_eq!(content_of(&d), ["a", "b", "c", "d"]);
    let items = content_items(&r);
    assert_eq!(items.len(), 1, "{:#?}", r.items);
    assert_eq!(items[0].class, Class::Changed);
}

#[test]
fn a_control_character_is_unsupported_and_removed() {
    let (r, d) = import(md("a\u{0}b\u{1}c\u{7f}d\te"));
    assert_eq!(content_of(&d), ["abcd\te"]);
    let items = content_items(&r);
    assert_eq!(items.len(), 1, "{:#?}", r.items);
    assert_eq!(items[0].class, Class::Unsupported);
    assert_eq!(items[0].fallback, Some(Fallback::other("the character is removed")));
}

#[test]
fn a_content_that_is_absent_is_changed_to_an_empty_content() {
    let (r, d) = import(json!({"mode": "markdown"}));
    assert_eq!(first_panel(&d)["options"]["content"], json!([]));
    let item = one_item(&r, "options.content");
    assert_eq!(item.class, Class::Changed);
}

#[test]
fn a_content_that_is_not_text_is_unsupported() {
    let (r, d) = import(json!({"mode": "markdown", "content": 12}));
    assert_eq!(first_panel(&d)["options"]["content"], json!([]));
    assert_eq!(one_item(&r, "options.content").class, Class::Unsupported);
}

#[test]
fn a_text_of_spaces_is_kept() {
    let (r, d) = import(md("   \n  "));
    assert_eq!(content_of(&d), ["   ", "  "]);
    assert!(content_items(&r).is_empty());
}

#[test]
fn a_text_above_a_limit_of_4_35_removes_the_panel_and_a_text_at_the_limit_stays() {
    let at_lines = vec!["x"; MAX_TEXT_LINES].join("\n");
    let (r, d) = import(md(&at_lines));
    assert_eq!(content_of(&d).len(), MAX_TEXT_LINES);
    no_items(&r);
    let at_line = "é".repeat(MAX_TEXT_LINE_BYTES / 2);
    let (r, d) = import(md(&at_line));
    assert_eq!(content_of(&d)[0].len(), MAX_TEXT_LINE_BYTES);
    no_items(&r);
    let mut lines = vec!["b".repeat(8191); 7];
    lines.push("c".repeat(8192));
    let at_bytes = lines.join("\n");
    assert_eq!(at_bytes.len(), MAX_TEXT_BYTES);
    let (r, d) = import(md(&at_bytes));
    assert_eq!(content_of(&d).len(), 8);
    no_items(&r);
    // One above.
    let above = [
        vec!["x"; MAX_TEXT_LINES + 1].join("\n"),
        "a".repeat(MAX_TEXT_LINE_BYTES + 1),
        format!("{}\n{}", "b".repeat(8192), vec!["b".repeat(8192); 7].join("\n")),
    ];
    for (i, text) in above.iter().enumerate() {
        let (r, d) = import(md(text));
        assert!(no_panel(&d), "case {i}");
        let item = one_item(&r, "options.content");
        assert_eq!(item.class, Class::Unsupported, "case {i}");
        assert_eq!(item.fallback, Some(Fallback::PanelRemoved), "case {i}");
    }
}

#[test]
fn a_fence_that_pushes_the_text_above_a_limit_removes_the_panel() {
    // The text has 2,000 lines: the two fence lines make 2,002.
    let (r, d) = import(json!({"mode": "code", "content": vec!["x"; MAX_TEXT_LINES].join("\n")}));
    assert!(no_panel(&d));
    assert!(r.items.iter().any(|i| i.fallback == Some(Fallback::PanelRemoved)));
}

// ----------------------------------------------------------- the constructs of the list under the table

/// Each construct of the list gives one `changed` item with the construct and its line. The panel keeps its text.
#[test]
fn each_construct_of_the_list_gives_one_changed_item_and_the_panel_keeps_its_text() {
    let cases: [(&str, &str); 12] = [
        ("<b>x</b>", "HTML"),
        ("![alt](http://a/i.png)", "image"),
        ("fish &amp; chips", "entity"),
        ("~~old~~", "strike"),
        ("- [ ] todo", "task"),
        ("Title\n=====", "= or -"),
        ("para\n\n    code", "indented code"),
        ("[name]: http://a", "link reference"),
        ("[x](javascript:alert(1))", "link destination"),
        ("[x](//evil.example)", "link destination"),
        ("${v:json} and [[v]]", "placeholder"),
        ("see [x](https://a/$v)", "placeholder"),
    ];
    for (text, what) in cases {
        let (r, d) = import(md(text));
        let items = content_items(&r);
        assert_eq!(items.len(), 1, "{text:?}: {:#?}", r.items);
        assert_eq!(items[0].class, Class::Changed, "{text:?}");
        assert!(items[0].reason.contains(what), "{text:?}: {}", items[0].reason);
        assert!(items[0].reason.starts_with("line "), "{}", items[0].reason);
        assert_eq!(content_of(&d).join("\n"), text, "the panel keeps its text");
    }
}

#[test]
fn the_report_lists_the_first_construct_and_its_line_only() {
    let (r, _) = import(md("plain\nplain\n~~strike~~\n<b>html</b>\n![i](http://a)"));
    let items = content_items(&r);
    assert_eq!(items.len(), 1, "{:#?}", r.items);
    assert!(items[0].reason.starts_with("line 3: "), "{}", items[0].reason);
}

#[test]
fn a_text_that_hune_shows_as_grafana_does_gives_no_item() {
    let text = "# Title\n\n- one\n- two\n\n1. a\n2. b\n\n> quote\n\n```\ncode\n```\n\n| a | b |\n|---|:-:|\n| 1 | 2 |\n\n**bold** *em* `code` [docs](https://wiki.example/a?b=1&c=2) see https://x.example\n\nRegion $region and ${region:csv} and ${region:text}.";
    let (r, d) = import(md(text));
    no_items(&r);
    assert_eq!(content_of(&d).join("\n"), text);
}

#[test]
fn a_nesting_above_6_levels_and_a_table_of_21_columns_remove_the_panel() {
    let quotes = format!("{}x", "> ".repeat(7));
    let list = (0..7).map(|i| format!("{}- x", "  ".repeat(i))).collect::<Vec<_>>().join("\n");
    let row = |c: &str| format!("|{}", format!("{c}|").repeat(21));
    let table = [row(" a "), row("---"), row(" 1 ")].join("\n");
    for (name, text) in [("quotes", quotes), ("list", list), ("table", table)] {
        let (r, d) = import(md(&text));
        assert!(no_panel(&d), "{name}");
        let items = content_items(&r);
        assert!(
            items.iter().any(|i| i.class == Class::Unsupported && i.fallback == Some(Fallback::PanelRemoved)),
            "{name}: {:#?}",
            r.items
        );
    }
    // At the limit the panel stays.
    let quotes = format!("{}x", "> ".repeat(6));
    let (r, d) = import(md(&quotes));
    assert_eq!(content_of(&d), [quotes]);
    no_items(&r);
}

// ----------------------------------------------------------------------------------- `options.code`

#[test]
fn the_options_code_is_metadata_in_markdown_and_listed_in_the_other_modes_when_it_is_not_the_default() {
    let (r, _) = import(
        json!({"mode": "markdown", "content": "x", "code": {"language": "go", "showLineNumbers": true, "showMiniMap": true}}),
    );
    no_items(&r);
    let (r, _) = import(
        json!({"mode": "code", "content": "x", "code": {"language": "go", "showLineNumbers": true, "showMiniMap": false}}),
    );
    let item = one_item(&r, "options.code");
    assert_eq!(item.class, Class::Dropped);
    let (r, _) = import(json!({"mode": "code", "content": "x", "code": code_default()}));
    assert!(items_with(&r, "options.code").is_empty());
}

#[test]
fn an_unknown_member_of_the_options_is_listed_as_in_the_other_types() {
    let (r, _) = import(json!({"mode": "markdown", "content": "x", "wrap": true}));
    let item = one_item(&r, "options.wrap");
    assert_eq!(item.class, Class::Unsupported);
}

// ---------------------------------------------------------------------- the members that it does not use

#[test]
fn the_repeat_of_a_text_panel_is_unsupported_and_the_panel_stays_without_it() {
    let mut p = text_v1(1, 0, md("x"));
    p["repeat"] = json!("host");
    p["repeatDirection"] = json!("h");
    p["maxPerRow"] = json!(3);
    let host = json!({"type": "custom", "allowCustomValue": false, "name": "host", "query": "a,b,c", "multi": true, "includeAll": true, "current": {"value": ["a"]}, "options": []});
    let r = run(&dash(vec![p], vec![host]));
    let d = doc(&r);
    assert!(first_panel(&d).get("repeat").is_none());
    let item = one_item(&r, "repeat");
    assert_eq!(item.class, Class::Unsupported);
    assert_eq!(item.fallback, Some(Fallback::other("no repeat")));
    assert_eq!(r.items.len(), 1, "{:#?}", r.items);
}

#[test]
fn the_v2_repeat_of_a_text_panel_is_unsupported_too() {
    for f in V2_FORMS {
        let mut d = v2_dash(f, md("x"));
        d["spec"]["layout"]["spec"]["rows"][0]["spec"]["layout"]["spec"]["items"][0]["spec"]["repeat"] =
            json!({"mode": "variable", "value": "host", "direction": "v"});
        d["spec"]["variables"] = json!([{"kind": "CustomVariable", "spec": {"allowCustomValue": false, "name": "host", "multi": true, "includeAll": true,
            "query": "a,b", "current": {"text": "a", "value": "a"}, "options": [{"text": "a", "value": "a"}]}}]);
        let r = run(&d);
        let out = doc(&r);
        assert!(first_panel(&out).get("repeat").is_none(), "{f}");
        assert!(
            r.items.iter().any(|i| i.class == Class::Unsupported && i.fallback == Some(Fallback::other("no repeat"))),
            "{f}: {:#?}",
            r.items
        );
    }
}

#[test]
fn the_query_and_field_members_of_a_text_panel_are_metadata_when_empty_and_dropped_when_not() {
    // Empty and default values: metadata, no item.
    let mut p = text_v1(1, 0, md("x"));
    p["targets"] = json!([{"refId": "A"}]);
    p["transformations"] = json!([]);
    p["interval"] = json!("");
    p["maxDataPoints"] = json!(0);
    p["timeFrom"] = json!(null);
    p["timeShift"] = json!("");
    p["hideTimeOverride"] = json!(false);
    p["cacheTimeout"] = json!("");
    let r = run(&dash(vec![p], vec![]));
    doc(&r);
    no_items(&r);
    // A value that is not empty is listed.
    let mut p = text_v1(1, 0, md("x"));
    p["datasource"] = ch();
    p["targets"] = json!([target("SELECT 1")]);
    p["transformations"] = json!([{"id": "organize", "options": {}}]);
    p["interval"] = json!("1m");
    p["maxDataPoints"] = json!(100);
    p["timeFrom"] = json!("6h");
    p["timeShift"] = json!("1h");
    p["hideTimeOverride"] = json!(true);
    p["fieldConfig"] = json!({"defaults": {"unit": "bytes"}, "overrides": []});
    let r = run(&dash(vec![p], vec![]));
    let d = doc(&r);
    for member in [
        "datasource",
        "targets",
        "transformations",
        "interval",
        "maxDataPoints",
        "timeFrom",
        "timeShift",
        "hideTimeOverride",
        "fieldConfig",
    ] {
        let item = one_item(&r, member);
        assert_eq!(item.class, Class::Dropped, "{member}");
    }
    let panel = first_panel(&d);
    for member in ["data", "transforms", "fields"] {
        assert!(panel.get(member).is_none(), "{member}");
    }
}

#[test]
fn a_text_panel_keeps_its_links_and_its_other_common_members() {
    let mut p = text_v1(1, 0, md("x"));
    p["transparent"] = json!(true);
    p["description"] = json!("About");
    p["links"] = json!([{"title": "Wiki", "url": "https://wiki.example", "targetBlank": true}]);
    let r = run(&dash(vec![p], vec![]));
    let d = doc(&r);
    let panel = first_panel(&d);
    assert_eq!(panel["transparent"], true);
    assert_eq!(panel["description"], "About");
    assert_eq!(panel["links"], json!([{"title": "Wiki", "url": "https://wiki.example", "target_blank": true}]));
}

// -------------------------------------------------------------------------------------- hostile text

#[test]
fn a_hostile_content_goes_to_the_document_unchanged_and_the_document_is_valid() {
    let hostile = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "[x](javascript:alert(1))",
        "![a](http://evil.example/i.png)",
        "<iframe src=//evil.example></iframe>",
        "${__from} [[x]] $__to",
        "&lt;b&gt; &#60;",
        "\u{202e}rtl \u{200b}zero width",
    ]
    .join("\n");
    let (r, d) = import(md(&hostile));
    assert_eq!(content_of(&d).join("\n"), hostile, "the text is data: no sanitiser, no change");
    // The report lists the first construct (the HTML at line 1).
    let items = content_items(&r);
    assert_eq!(items.len(), 1, "{:#?}", r.items);
    assert!(items[0].reason.starts_with("line 1: HTML"), "{}", items[0].reason);
}

#[test]
fn the_import_of_a_text_panel_is_stable() {
    let first = run(&dash(vec![text_v1(1, 0, md("# a\r\nb"))], vec![]));
    let second = run(&dash(vec![text_v1(1, 0, md("# a\r\nb"))], vec![]));
    assert_eq!(first.document_bytes, second.document_bytes);
}

// ---------------------------------------------------------------------- the file of the browser case B109

#[test]
fn the_file_text_panels_gives_the_rows_of_15_6() {
    let file = repo_root().join("tests/fixtures/grafana/text_panels.json");
    let input = read_json(&file);
    let first = run_plain(&input);
    // The first run needs an acceptance: the html mode and the repeat are `unsupported`.
    assert!(first.items.iter().any(|i| i.class == Class::Unsupported));
    let r = run(&input);
    let d = doc(&r);
    let panels = d["sections"][0]["panels"].as_array().unwrap();
    assert_eq!(panels.len(), 9);
    assert!(panels.iter().all(|p| p["type"] == "text" && p.get("data").is_none() && p.get("repeat").is_none()));
    let text = |i: usize| {
        panels[i]["options"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    };
    // The html mode: the content in a code block of 3 backticks.
    assert_eq!(text(1), "```\n<p style=\"color:red\">Hello <b>HTML</b></p>\n<img src=x onerror=alert(1)>\n```");
    // The code mode with a fence of 4 backticks: the content has a run of 3.
    assert_eq!(text(2), "````\nfn main() {\n    println!(\"hi\");\n}\n```\n````");
    // The HTML in a markdown content stays as it is.
    assert_eq!(text(3), "Text <b>bold html</b> and <script>alert(1)</script>");
    assert_eq!(text(4), "| a | b |\n|---|---|\n| 1 | 2 |\n\n![alt](http://127.0.0.1:1/i.png)");
    assert_eq!(text(5), "$region ${region:csv} ${region:json} [[region]] $__from");
    assert_eq!(panels[7]["options"]["content"], json!([]));
    assert_eq!(text(8), "   \n  ");
    // The items of the rows.
    let by = |panel: i64, member: &str| -> Vec<&ReportItem> {
        r.items.iter().filter(|i| i.panel_id == Some(panel) && i.member == member).collect()
    };
    assert_eq!(by(2, "options.mode")[0].class, Class::Unsupported);
    assert_eq!(by(3, "options.mode")[0].class, Class::Changed);
    assert_eq!(by(3, "options.code")[0].class, Class::Dropped);
    assert!(by(4, "options.content")[0].reason.starts_with("line 1: HTML"));
    assert!(by(5, "options.content")[0].reason.starts_with("line 5: an image"));
    assert!(by(6, "options.content")[0].reason.starts_with("line 1: a placeholder"));
    assert_eq!(by(7, "repeat")[0].class, Class::Unsupported);
    assert!(
        by(1, "options.content").is_empty()
            && by(8, "options.content").is_empty()
            && by(9, "options.content").is_empty()
    );
}
