// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the whole run on the synthetic export. They check the golden documents, the report,
//! the nginx map, the acceptance, the exit codes, the folders, the uids and the links. They also
//! check that the four input forms give the same document (I01, I03, I04, I08, I09, I11, I12).

mod common;

use std::collections::BTreeSet;

use common::*;
use hune_core::document::validate;
use hune_core::ids::Uid;
use hune_grafana::{
    Class, DashboardResult, Fallback, ImportResult, ReportMeta, import_all, parse_acceptance_file, render_nginx_map,
    render_report,
};
use serde_json::{Value, json};

/// The text of the acceptance file for the items of a run without acceptance: one line for each
/// item id, with the reason as comment.
fn acceptance_text_for(result: &ImportResult) -> String {
    let mut lines: Vec<(String, String)> = Vec::new();
    for i in result.all_items() {
        if !lines.iter().any(|(id, _)| *id == i.id) {
            lines.push((i.id.clone(), format!("{}: {}", i.class.as_str(), i.reason)));
        }
    }
    lines.sort();
    let mut text = String::from("# Synthetic acceptance file of the importer tests. Each line accepts one loss.\n");
    for (id, why) in lines {
        text.push_str(&format!("{id}   # {why}\n"));
    }
    text
}

fn accept_file() -> String {
    std::fs::read_to_string(repo_root().join("tests/fixtures/grafana/accept.txt")).expect("accept.txt")
}

fn dry_run() -> ImportResult {
    import_all(&load_input(), &options("")).expect("import")
}

fn accepted_run() -> ImportResult {
    import_all(&load_input(), &options(&accept_file())).expect("import")
}

#[test]
fn the_acceptance_file_lists_the_current_items() {
    let text = acceptance_text_for(&dry_run());
    if std::env::var_os("HUNE_BLESS").is_some() {
        std::fs::write(repo_root().join("tests/fixtures/grafana/accept.txt"), &text).expect("write");
    }
    assert_eq!(accept_file(), text, "run with HUNE_BLESS=1 to write tests/fixtures/grafana/accept.txt");
}

#[test]
fn a_run_without_acceptance_exits_with_1() {
    let r = dry_run();
    assert_eq!(r.exit_code(), 1);
    let t = r.totals();
    assert!(t.unaccepted > 0);
    assert_eq!(t.accepted, 0);
    for d in &r.dashboards {
        assert!(d.document_bytes.is_none() || d.items.is_empty(), "{}", d.grafana_uid);
        match d.grafana_uid.as_str() {
            "plain-raw" | "prod-main" => assert_eq!(d.result, DashboardResult::WouldWrite),
            _ => assert_eq!(d.result, DashboardResult::Blocked, "{}", d.grafana_uid),
        }
    }
}

#[test]
fn a_run_with_the_acceptance_file_exits_with_0() {
    let r = accepted_run();
    assert_eq!(r.exit_code(), 0, "{}", r.exit_reason());
    assert!(r.stale_accept_lines.is_empty());
    let t = r.totals();
    assert_eq!(t.unaccepted, 0);
    assert!(t.accepted > 0);
    let by = |uid: &str| r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
    for uid in [
        "monitor",
        "monitor-wrapped",
        "monitor-v2a",
        "monitor-v2b",
        "Mixed_Case_Uid",
        "Prod_Main",
        "plain-raw",
        "prod-main",
    ] {
        assert_eq!(by(uid).result, DashboardResult::WouldWrite, "{uid}");
        assert!(by(uid).etag.is_some());
    }
    for uid in ["nested-case", "bad-folder"] {
        assert_eq!(by(uid).result, DashboardResult::NotImported, "{uid}");
        assert!(by(uid).document_bytes.is_none());
    }
}

#[test]
fn a_stale_line_gives_exit_code_1() {
    let text = format!("{}monitor:/no/such/member\n", accept_file());
    let r = import_all(&load_input(), &options(&text)).expect("import");
    assert_eq!(r.stale_accept_lines, vec!["monitor:/no/such/member".to_owned()]);
    assert_eq!(r.exit_code(), 1);
    assert_eq!(r.totals().unaccepted, 0);
}

#[test]
fn the_documents_equal_the_golden_files_and_validate() {
    let r = accepted_run();
    let mut written = BTreeSet::new();
    for d in &r.dashboards {
        let Some(bytes) = &d.document_bytes else { continue };
        let text = String::from_utf8(bytes.clone()).expect("utf-8");
        check_golden(&format!("documents/{}.json", d.uid), &text);
        // I12: the document validates and is canonical.
        let uid = Uid::parse(&d.uid).expect("uid");
        let valid = validate(bytes, Some(&uid)).expect("valid document");
        assert_eq!(valid.bytes(), bytes.as_slice());
        assert_eq!(valid.etag().to_hex(), d.etag.clone().unwrap());
        written.insert(d.uid.clone());
    }
    assert_eq!(written.len(), 8);
}

#[test]
fn the_run_is_deterministic() {
    let a = accepted_run();
    let b = accepted_run();
    for (x, y) in a.dashboards.iter().zip(&b.dashboards) {
        assert_eq!(x.document_bytes, y.document_bytes);
        assert_eq!(x.items, y.items);
    }
    let meta = report_meta();
    assert_eq!(render_report(&a, &meta), render_report(&b, &meta));
}

fn strip(doc: &str, uid: &str) -> Value {
    let mut v: Value = serde_json::from_str(doc).expect("json");
    assert_eq!(v["uid"], uid);
    v["uid"] = json!("x");
    // The member `allow_custom_value` comes from `allowCustomValue`, which only the form v2beta1 of the fixture has
    // on a `custom` variable. The test `open_custom_variable_of_the_v2beta1_export` checks it.
    for var in v["variables"].as_array_mut().expect("variables") {
        var.as_object_mut().expect("variable").remove("allow_custom_value");
    }
    v
}

#[test]
fn open_custom_variable_of_the_v2beta1_export() {
    // 15.6: Grafana reads an absent member as `true`. A `query` variable imports as open. A `custom` variable
    // imports as closed unless the export says `true`. The v2beta1 export says `true` on its variables.
    let r = accepted_run();
    let doc = |uid: &str| -> Value {
        let d = r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
        serde_json::from_slice(d.document_bytes.as_ref().unwrap()).unwrap()
    };
    let open = |v: &Value| v["allow_custom_value"] == json!(true);
    for uid in ["monitor", "monitor-wrapped", "monitor-v2a", "monitor-v2b"] {
        let d = doc(uid);
        for var in d["variables"].as_array().unwrap() {
            match var["kind"].as_str().unwrap() {
                "query" => assert!(open(var), "{uid}: a query variable is open"),
                "text" => assert!(var.get("allow_custom_value").is_none(), "{uid}"),
                _ => assert_eq!(open(var), uid == "monitor-v2b", "{uid}: {}", var["name"]),
            }
        }
    }
    let by = |uid: &str| r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
    // The absent member of a `custom` variable has one `changed` item in each form that lacks the member.
    for (uid, pointer) in [
        ("monitor", "/templating/list/1/allowCustomValue"),
        ("monitor-wrapped", "/dashboard/templating/list/1/allowCustomValue"),
        ("monitor-v2a", "/spec/variables/1/spec/allowCustomValue"),
    ] {
        let item = by(uid).items.iter().find(|i| i.pointer == pointer).expect(pointer);
        assert_eq!(item.class, Class::Changed);
        assert!(item.reason.contains("keeps the variable closed"), "{}", item.reason);
    }
    // The member of a `text` variable is metadata: no item names it.
    assert!(!by("monitor-v2b").items.iter().any(|i| i.member.contains("allowCustomValue")));
}

#[test]
fn the_four_forms_give_the_same_document() {
    // I01: one dashboard in the forms v1 raw, v1 wrapped, v2alpha1 and v2beta1.
    let r = accepted_run();
    let text = |uid: &str| {
        String::from_utf8(r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap().document_bytes.clone().unwrap())
            .unwrap()
    };
    let raw = strip(&text("monitor"), "monitor");
    for uid in ["monitor-wrapped", "monitor-v2a", "monitor-v2b"] {
        assert_eq!(strip(&text(uid), uid), raw, "{uid}");
    }
    let by = |uid: &str| r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
    assert_eq!(by("monitor").form, Some(hune_grafana::Form::V1Raw));
    assert_eq!(by("monitor-wrapped").form, Some(hune_grafana::Form::V1Wrapped));
    assert_eq!(by("monitor-v2a").form, Some(hune_grafana::Form::V2Alpha1));
    assert_eq!(by("monitor-v2b").form, Some(hune_grafana::Form::V2Beta1));
    // The folder of each form: the search answer, the meta, the annotation, none (the root).
    assert_eq!(by("monitor").folder, "transit");
    assert_eq!(by("monitor-wrapped").folder, "transit");
    assert_eq!(by("monitor-v2a").folder, "transit");
    assert_eq!(by("monitor-v2b").folder, "general");
}

#[test]
fn the_items_of_the_four_forms_agree() {
    // The items of a loss have the same class and reason in each form. The exception is a member
    // that only one form has, for example the v2 member `allowCustomValue`.
    let r = accepted_run();
    let key = |d: &hune_grafana::DashboardImport| {
        let mut v: Vec<(String, String, Option<i64>)> = d
            .items
            .iter()
            .filter(|i| !(i.member.contains("allowCustomValue") || i.member.starts_with("zzz") || i.member == "style"))
            .filter(|i| {
                !i.id.contains("annotations")
                    && !i.member.starts_with("links.")
                    && !i.member.starts_with("annotations")
                    && i.member != "tags.2"
            })
            .map(|i| (i.class.as_str().to_owned(), i.reason.clone(), i.panel_id))
            .collect();
        v.sort();
        v
    };
    let by = |uid: &str| r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
    let base = key(by("monitor"));
    // 40 items since P03: the orientation `auto` of a bar chart maps (4.30).
    // 39 since P14: `spanNulls` as a number maps (4.28).
    assert!(base.len() >= 39, "{}", base.len());
    for uid in ["monitor-wrapped", "monitor-v2a", "monitor-v2b"] {
        let got = key(by(uid));
        let missing: Vec<_> = base.iter().filter(|x| !got.contains(x)).collect();
        assert!(missing.is_empty(), "{uid} lacks {missing:#?}");
    }
}

#[test]
fn uids_and_folders() {
    let r = accepted_run();
    let by = |uid: &str| r.dashboards.iter().find(|d| d.grafana_uid == uid).unwrap();
    // I08: the rule of 15.8 and the collision.
    assert_eq!(by("Mixed_Case_Uid").uid, "mixed-case-uid");
    assert_eq!(by("prod-main").uid, "prod-main");
    assert!(by("Prod_Main").uid.starts_with("g-"));
    assert_eq!(by("Prod_Main").uid.len(), 18);
    assert!(by("Mixed_Case_Uid").items.iter().any(|i| i.class == Class::Changed && i.pointer == "/dashboard/uid"));
    // I09: root, folder map, nested folder, folder without id.
    assert_eq!(by("plain-raw").folder, "general");
    assert_eq!(by("Mixed_Case_Uid").folder, "nodes");
    assert_eq!(by("Mixed_Case_Uid").folder_title, "Nodes");
    assert_eq!(by("monitor").folder_title, "Transit desk");
    assert_eq!(r.folder_items.len(), 1);
    assert_eq!(r.folder_items[0].id, "folder:kid0nested:/2/parentUid");
    assert_eq!(r.folder_items[0].fallback, Some(Fallback::other("its dashboards are not imported")));
    let dry = dry_run();
    let nested = dry.dashboards.iter().find(|d| d.grafana_uid == "nested-case").unwrap();
    assert_eq!(nested.result, DashboardResult::Blocked);
    let bad = dry.dashboards.iter().find(|d| d.grafana_uid == "bad-folder").unwrap();
    assert_eq!(bad.result, DashboardResult::Blocked);
    assert_eq!(bad.items.len(), 1);
    assert_eq!(bad.items[0].fallback, Some(Fallback::DashboardNotImported));
    let ids: Vec<&str> = r.folders.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["general", "nodes", "transit"]);
}

#[test]
fn links_and_the_nginx_map() {
    let r = accepted_run();
    // I11: a link to a dashboard of the run is rewritten with the link prefix.
    let sw = r.dashboards.iter().find(|d| d.grafana_uid == "monitor").unwrap();
    let text = String::from_utf8(sw.document_bytes.clone().unwrap()).unwrap();
    assert!(text.contains("/dashboards/d/mixed-case-uid/other?var-x=${node}"), "{text}");
    assert!(!text.contains("/grafana/d/"));
    // Without a prefix, the link stays and gets an item.
    let mut o = options(&accept_file());
    o.link_prefix = None;
    let no_prefix = import_all(&load_input(), &o).expect("import");
    let sw2 = no_prefix.dashboards.iter().find(|d| d.grafana_uid == "monitor").unwrap();
    assert!(sw2.items.iter().any(|i| i.reason.contains("--link-prefix")));
    assert!(no_prefix.redirects.is_empty());
    // The nginx map.
    let map = render_nginx_map(&r);
    check_golden("nginx.map", &map);
    assert!(map.contains("\"Mixed_Case_Uid\" \"mixed-case-uid\";"));
    assert!(map.contains("var-node=%24__all"), "$ is encoded as %24");
    assert!(!map.contains("$__all"));
    assert!(
        map.contains(
            "\"xYz12AB\" \"/dashboards/d/monitor/node-monitor?orgId=1&from=now-7d&to=now&var-node=%24__all\";"
        )
    );
    assert!(map.contains("\"Lm0pqRs\" \"/dashboards/\";"));
    // The short uid with a space has no line and an item.
    assert!(!map.contains("bad uid"));
    assert!(r.short_link_items.iter().any(|i| i.id == "short-url:bad uid:/2/uid"));
    assert_eq!(r.not_imported.short_links, 4);
    assert_eq!(r.not_imported.public_dashboards, 1);
    assert!(r.redirects.iter().any(|x| x.from == "/grafana/goto/xYz12AB"));
    assert!(
        r.redirects.iter().any(|x| x.from == "/grafana/d/Mixed_Case_Uid" && x.to == "/dashboards/d/mixed-case-uid")
    );
}

fn report_meta() -> ReportMeta {
    let o = options("");
    ReportMeta {
        hune_version: "0.1.1".to_owned(),
        created_at: "2026-11-20T09:12:44Z".to_owned(),
        input_dir: "/private/grafana-export-20261120T090000Z".to_owned(),
        source_map: o.source_map.clone(),
        link_prefix: o.link_prefix.clone(),
        folders_file: Some("folders.map.json".to_owned()),
        accept_file: Some("accept.txt".to_owned()),
        write: false,
        replace: false,
    }
}

#[test]
fn the_reports_equal_their_golden_files_and_the_json_follows_its_schema() {
    // I03.
    let r = accepted_run();
    let rendered = render_report(&r, &report_meta());
    check_golden("report.json", &rendered.json);
    check_golden("report.md", &rendered.markdown);
    let schema = read_json(&golden_dir().join("report.schema.json"));
    let report: Value = serde_json::from_str(&rendered.json).expect("json");
    let mut errors = Vec::new();
    schema_errors(&schema, &schema, &report, "", &mut errors);
    assert!(errors.is_empty(), "{errors:#?}");
    for d in report["dashboards"].as_array().unwrap() {
        if let Some(e) = d["etag"].as_str() {
            assert_eq!(e.len(), 64);
            assert!(e.bytes().all(|b| b.is_ascii_hexdigit()));
        }
    }
    assert_eq!(report["exit_code"], 0);
    // The dashboards are in the order of their Grafana uid, and items in the order of their pointer.
    let uids: Vec<&str> =
        report["dashboards"].as_array().unwrap().iter().map(|d| d["grafana_uid"].as_str().unwrap()).collect();
    let mut sorted = uids.clone();
    sorted.sort();
    assert_eq!(uids, sorted);
    // A dry run without acceptance.
    let dry = dry_run();
    let rendered = render_report(&dry, &report_meta());
    let report: Value = serde_json::from_str(&rendered.json).expect("json");
    let mut errors = Vec::new();
    schema_errors(&schema, &schema, &report, "", &mut errors);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(report["exit_code"], 1);
    check_golden("report-unaccepted.md", &rendered.markdown);
}

#[test]
fn the_markdown_report_has_the_six_parts_in_order() {
    let md = render_report(&dry_run(), &report_meta()).markdown;
    let heads: Vec<&str> = md.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        heads,
        vec![
            "## Input",
            "## Totals",
            "## Objects that are not imported",
            "## Dashboards",
            "## Redirects",
            "## Exit code"
        ]
    );
}

#[test]
fn the_json_value_is_cut_to_200_characters() {
    let r = dry_run();
    for i in r.all_items() {
        if let Some(v) = &i.value {
            assert!(v.chars().count() <= 200);
            assert_eq!(i.truncated, v.chars().count() == 200 && i.truncated);
        }
    }
    assert!(r.all_items().any(|i| i.truncated));
}

#[test]
fn acceptance_of_a_dropped_item_and_of_an_empty_file() {
    let r = import_all(&load_input(), &options("")).expect("import");
    assert!(r.all_items().any(|i| i.class == Class::Dropped));
    assert!(parse_acceptance_file("").unwrap().ids.is_empty());
}
