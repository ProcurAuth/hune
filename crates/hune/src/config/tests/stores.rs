// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the stores of the block `storage` (13.10): `storage.volume "<id>"`, `storage.postgres`, the primary
//! store and the contradictions C2, C10 and C30 to C34.

use super::*;

const MAIN_PRIMARY: &str = "    primary         = true\n";
const MAIN_HISTORY: &str = "    history_keep    = 100\n";
const MAIN_BLOCK: &str = "  volume \"main\" {\n";
const PG_PRIMARY: &str = "    primary           = true\n";
const PG_HISTORY: &str = "    history_keep      = 100\n";

/// The local file with its volume as a read-only store: no primary store, a title, no history. The file does not
/// load: C10 needs exactly one primary store.
fn read_only_main(text: &str) -> String {
    let t = replace_once(text, MAIN_PRIMARY, "    title           = \"Main\"\n");
    replace_once(&t, MAIN_HISTORY, "")
}

/// A second volume, read-only, with the label, the title and the path that the test gives.
fn volume(label: &str, title: Option<&str>, path: &str, extra: &str) -> String {
    let title = title.map(|t| format!("    title           = \"{t}\"\n")).unwrap_or_default();
    format!(
        "  volume \"{label}\" {{\n{title}    path            = \"{path}\"\n    rescan_interval = \"5s\"\n{extra}  }}\n\n"
    )
}

fn with_volume(text: &str, block: &str) -> String {
    replace_once(text, MAIN_BLOCK, &format!("{block}{MAIN_BLOCK}"))
}

#[test]
fn the_local_file_has_one_primary_volume_and_the_distributed_file_a_primary_postgres_store() {
    let fx = Fx::new();
    let local = fx.load(&fx.local()).unwrap().config.storage;
    assert_eq!(local.primary_id(), Some("main"));
    let dist = fx.load(&fx.distributed()).unwrap().config.storage;
    assert_eq!(dist.primary_id(), Some("postgres"));
}

#[test]
fn c10_a_file_with_no_primary_store_stops_the_start_and_lists_the_store_blocks() {
    let fx = Fx::new();
    // One store: the message names it.
    let text = read_only_main(&fx.local());
    assert_eq!(
        fx.one(&text),
        "storage: the file has no primary store; give `primary = true` to `storage.volume \"main\"`"
    );
    // Two stores: the message lists both, in the order of the file.
    let lab = volume("lab", Some("Lab"), "/var/lib/hune/lab", "");
    assert_eq!(
        fx.one(&with_volume(&text, &lab)),
        "storage: the file has no primary store; give `primary = true` to exactly one of `storage.volume \"lab\"` and `storage.volume \"main\"`"
    );
    // Three stores: a comma list.
    let dist = replace_once(&fx.distributed(), PG_PRIMARY, "    title             = \"Archive\"\n");
    let dist = replace_once(&dist, PG_HISTORY, "");
    let text = replace_once(
        &dist,
        "storage {\n",
        &format!("storage {{\n{}", volume("lab", Some("Lab"), "/var/lib/hune/lab", "")),
    );
    let text = replace_once(
        &text,
        "storage {\n",
        &format!("storage {{\n{}", volume("alt", Some("Alt"), "/var/lib/hune/alt", "")),
    );
    assert_eq!(
        fx.one(&text),
        "storage: the file has no primary store; give `primary = true` to exactly one of `storage.volume \"alt\"`, `storage.volume \"lab\"` and `storage.postgres`"
    );
    // `primary = false` on the only store means the same as no key.
    let off = replace_once(&fx.local(), MAIN_PRIMARY, "    primary         = false\n    title           = \"Main\"\n");
    let off = replace_once(&off, MAIN_HISTORY, "");
    assert!(fx.one(&off).starts_with("storage: the file has no primary store"));
}

#[test]
fn a_read_only_volume_and_a_read_only_postgres_store_can_follow_a_primary_store() {
    let fx = Fx::new();
    let lab = volume("lab", Some("Lab"), "/var/lib/hune/lab", "");
    let text = with_volume(&fx.local(), &lab);
    let storage = fx.load(&text).unwrap().config.storage;
    assert_eq!(storage.volumes.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["lab", "main"]);
    assert_eq!(storage.primary_id(), Some("main"));
    // The PostgreSQL store as a read-only store, next to a primary volume.
    let dist = fx.distributed();
    let dist = replace_once(&dist, PG_PRIMARY, "    title             = \"Archive\"\n");
    let dist = replace_once(&dist, PG_HISTORY, "");
    let text = replace_once(
        &dist,
        "storage {\n",
        "storage {\n  volume \"main\" {\n    path            = \"/var/lib/hune/dashboards\"\n    primary         = true\n    rescan_interval = \"5s\"\n    history_keep    = 100\n  }\n\n",
    );
    let storage = fx.load(&text).unwrap().config.storage;
    assert_eq!(storage.primary_id(), Some("main"));
    let pg = storage.postgres.as_ref().unwrap();
    assert_eq!((pg.primary, pg.title.as_deref(), pg.history_keep), (false, Some("Archive"), None));
}

#[test]
fn the_removed_keys_stop_the_start_as_unknown_keys() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(&local, "storage {\n", "storage {\n  backend = \"volume\"\n");
    assert!(fx.one(&text).starts_with("storage.backend: unknown attribute (line "), "{}", fx.one(&text));
    let text = replace_once(&local, MAIN_PRIMARY, &format!("{MAIN_PRIMARY}    read_only       = false\n"));
    assert!(fx.one(&text).starts_with("storage.volume \"main\".read_only: unknown attribute (line "));
    let text = replace_once(
        &local,
        "  base_path               = \"/\"\n",
        "  base_path               = \"/\"\n  editing                 = true\n",
    );
    assert!(fx.one(&text).starts_with("server.editing: unknown attribute (line "));
}

#[test]
fn c2_the_file_needs_a_store() {
    let fx = Fx::new();
    let no_volume = "  volume \"main\" {\n    path            = \"/var/lib/hune/dashboards\"\n    primary         = true\n    rescan_interval = \"5s\"\n    history_keep    = 100\n  }\n";
    let text = replace_once(&fx.local(), no_volume, "");
    assert_eq!(
        fx.one(&text),
        "storage.volume, storage.postgres: the file has no store; give at least one block `storage.volume \"<id>\"` or the block `storage.postgres`"
    );
}

#[test]
fn c10_two_stores_with_primary_true_stop_the_start() {
    let fx = Fx::new();
    let second = volume("lab", None, "/var/lib/hune/lab", "    primary         = true\n    history_keep    = 100\n");
    let text = with_volume(&fx.local(), &second);
    assert_eq!(
        fx.one(&text),
        "storage.volume \"lab\".primary, storage.volume \"main\".primary: `storage.volume \"lab\".primary` (true) and `storage.volume \"main\".primary` (true): exactly one store has `primary = true`"
    );
    // A volume and the PostgreSQL store.
    let dist = fx.distributed();
    let text = replace_once(
        &dist,
        "storage {\n",
        "storage {\n  volume \"main\" {\n    path            = \"/var/lib/hune/dashboards\"\n    primary         = true\n    rescan_interval = \"5s\"\n    history_keep    = 100\n  }\n\n",
    );
    assert_eq!(
        fx.one(&text),
        "storage.volume \"main\".primary, storage.postgres.primary: `storage.volume \"main\".primary` (true) and `storage.postgres.primary` (true): exactly one store has `primary = true`"
    );
}

#[test]
fn c30_a_read_only_store_needs_a_title_and_the_primary_store_has_none() {
    let fx = Fx::new();
    let local = fx.local();
    let with_title = replace_once(&local, MAIN_PRIMARY, &format!("{MAIN_PRIMARY}    title           = \"Main\"\n"));
    assert_eq!(
        fx.one(&with_title),
        "storage.volume \"main\".title, storage.volume \"main\".primary: `storage.volume \"main\".title` (\"Main\") is not permitted on the primary store; the primary store shows at the root of the page"
    );
    let no_title = with_volume(&local, &volume("lab", None, "/var/lib/hune/lab", ""));
    assert_eq!(
        fx.one(&no_title),
        "storage.volume \"lab\".title, storage.volume \"lab\".primary: `storage.volume \"lab\".title` is necessary: the store is read-only and the title is the name of its root folder"
    );
    // The PostgreSQL store follows the same rule.
    let dist = fx.distributed();
    let text = replace_once(&dist, PG_PRIMARY, &format!("{PG_PRIMARY}    title             = \"Archive\"\n"));
    assert!(fx.one(&text).starts_with(
        "storage.postgres.title, storage.postgres.primary: `storage.postgres.title` (\"Archive\") is not permitted"
    ));
    let text = replace_once(&replace_once(&dist, PG_PRIMARY, ""), PG_HISTORY, "");
    // With no primary store, C10 comes too: the title error is in the list.
    let e = fx.errors(&text);
    assert!(e.contains(&"storage.postgres.title, storage.postgres.primary: `storage.postgres.title` is necessary: the store is read-only and the title is the name of its root folder".to_owned()), "{e:?}");
}

#[test]
fn c31_only_the_primary_store_has_history() {
    let fx = Fx::new();
    let local = fx.local();
    let no_history = replace_once(&local, MAIN_HISTORY, "");
    assert_eq!(
        fx.one(&no_history),
        "storage.volume \"main\".history_keep, storage.volume \"main\".primary: `storage.volume \"main\".history_keep` is necessary: the store is the primary store"
    );
    let lab = volume("lab", Some("Lab"), "/var/lib/hune/lab", "    history_keep    = 7\n");
    assert_eq!(
        fx.one(&with_volume(&local, &lab)),
        "storage.volume \"lab\".history_keep, storage.volume \"lab\".primary: `storage.volume \"lab\".history_keep` (7) is not permitted: only the primary store has history"
    );
    let dist = fx.distributed();
    let text = replace_once(&dist, PG_HISTORY, "");
    assert_eq!(
        fx.one(&text),
        "storage.postgres.history_keep, storage.postgres.primary: `storage.postgres.history_keep` is necessary: the store is the primary store"
    );
}

#[test]
fn c32_the_paths_of_the_volumes_are_different_and_none_holds_another() {
    let fx = Fx::new();
    let local = fx.local();
    let same = volume("lab", Some("Lab"), "/var/lib/hune/dashboards", "");
    assert_eq!(
        fx.one(&with_volume(&local, &same)),
        "storage.volume \"lab\".path, storage.volume \"main\".path: `storage.volume \"lab\".path` (\"/var/lib/hune/dashboards\") and `storage.volume \"main\".path` (\"/var/lib/hune/dashboards\"): the second is the same path"
    );
    let inside = volume("lab", Some("Lab"), "/var/lib/hune/dashboards/lab", "");
    assert_eq!(
        fx.one(&with_volume(&local, &inside)),
        "storage.volume \"lab\".path, storage.volume \"main\".path: `storage.volume \"lab\".path` (\"/var/lib/hune/dashboards/lab\") and `storage.volume \"main\".path` (\"/var/lib/hune/dashboards\"): the second holds the first path"
    );
    let outside = volume("lab", Some("Lab"), "/var/lib/hune", "");
    assert_eq!(
        fx.one(&with_volume(&local, &outside)),
        "storage.volume \"lab\".path, storage.volume \"main\".path: `storage.volume \"lab\".path` (\"/var/lib/hune\") and `storage.volume \"main\".path` (\"/var/lib/hune/dashboards\"): the second is inside the first path"
    );
    // A name that only starts like the other path is a different path.
    let near = volume("lab", Some("Lab"), "/var/lib/hune/dashboards2", "");
    assert!(fx.errors(&with_volume(&local, &near)).is_empty());
}

#[test]
fn c33_a_volume_cannot_have_the_label_postgres() {
    let fx = Fx::new();
    let text = with_volume(&fx.local(), &volume("postgres", Some("Pg"), "/var/lib/hune/pg", ""));
    assert_eq!(
        fx.one(&text),
        "storage.volume \"postgres\": the label \"postgres\" is the id of the PostgreSQL store; give another label"
    );
}

#[test]
fn c34_two_read_only_stores_have_two_titles() {
    let fx = Fx::new();
    let a = volume("a", Some("Lab"), "/var/lib/hune/a", "");
    let b = volume("b", Some("Lab"), "/var/lib/hune/b", "");
    let text = with_volume(&with_volume(&fx.local(), &a), &b);
    assert_eq!(
        fx.one(&text),
        "storage.volume \"a\".title, storage.volume \"b\".title: `storage.volume \"a\".title` (\"Lab\") and `storage.volume \"b\".title` (\"Lab\") name the same root folder"
    );
}

#[test]
fn the_label_of_a_volume_follows_the_label_rule() {
    let fx = Fx::new();
    for bad in ["Lab", "a_b", "-a", "", &"a".repeat(41)] {
        let text = with_volume(&fx.local(), &volume(bad, Some("Lab"), "/var/lib/hune/lab", ""));
        let e = fx.errors(&text);
        assert!(
            e.iter().any(|m| m.contains("expected a label that matches ^[a-z0-9][a-z0-9-]{0,39}$")),
            "{bad:?}: {e:?}"
        );
    }
    let text = with_volume(&fx.local(), &volume("lab-2", Some("Lab"), "/var/lib/hune/lab", ""));
    assert!(fx.errors(&text).is_empty());
}

#[test]
fn a_file_with_more_than_sixteen_volumes_stops_the_start() {
    let fx = Fx::new();
    let mut text = fx.local();
    for n in 0..16 {
        text =
            with_volume(&text, &volume(&format!("v{n}"), Some(&format!("V{n}")), &format!("/var/lib/hune/v{n}"), ""));
    }
    let e = fx.errors(&text);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with("storage.volume: expected 0 to 16 blocks `storage.volume \"<label>\"`, found 17"),
        "{}",
        e[0]
    );
}
