// SPDX-License-Identifier: MIT OR Apache-2.0
//! The pre-pass of a create and of a save (11.20, 11.21): the ids of the sources that a document names, read from
//! the bytes of the document before the decision and before the validation of 4.36.
//!
//! The pre-pass reads four fixed paths and nothing else: `variables[].source`, `annotations[].source`,
//! `sections[].panels[].data.query.source` and `sections[].panels[].data.extra_queries[].source`. A hidden query and
//! a disabled annotation count. It skips each other value without a tree, so its work is linear in the bytes. Each
//! list has the budget of 4.35, so the work and the memory stay bounded. It keeps only the ids that the configuration
//! declares, without duplicates, in the order of `gate::document_sources`: the variables, the annotations, then the
//! panels. Thus an undeclared id never goes to the PDP.
//!
//! A document that the pre-pass cannot read (bad JSON, a wrong type, a list above its budget) gives `None`. The
//! handler then asks the use alone, and the validation gives the error (11.21).

use hune_core::limits::{MAX_ANNOTATIONS, MAX_PANELS, MAX_QUERIES_PER_PANEL, MAX_SECTIONS, MAX_VARIABLES};
use serde::de::{DeserializeSeed, Deserializer, Error as _, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::fmt;

/// The source ids of each part of the document, in document order, with the declared ids only.
#[derive(Default)]
struct Found {
    variables: Vec<String>,
    annotations: Vec<String>,
    panels: Vec<String>,
    /// The panels read so far, in all the sections (`MAX_PANELS`).
    panel_count: usize,
    /// The member `uid` of the document, for the fault injection of the tests.
    #[cfg(test)]
    uid: Option<String>,
}

/// The pre-pass: the declared source ids that the document names, or `None` when the bytes cannot be read.
pub fn declared_sources(bytes: &[u8], declared: &[&str]) -> Option<Vec<String>> {
    let mut found = Found::default();
    let mut de = serde_json::Deserializer::from_slice(bytes);
    DocSeed { found: &mut found, declared }.deserialize(&mut de).ok()?;
    de.end().ok()?;
    let mut out: Vec<String> = Vec::new();
    for id in found.variables.into_iter().chain(found.annotations).chain(found.panels) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    #[cfg(test)]
    if !tests_hook::apply(found.uid.as_deref(), &mut out) {
        return None;
    }
    Some(out)
}

/// Keeps a source id when the configuration declares it.
fn keep(list: &mut Vec<String>, declared: &[&str], id: String) {
    if declared.contains(&id.as_str()) && !list.contains(&id) {
        list.push(id);
    }
}

/// The part of the document that a list belongs to.
#[derive(Clone, Copy)]
enum Part {
    Variables,
    Annotations,
}

struct DocSeed<'a, 'd> {
    found: &'a mut Found,
    declared: &'d [&'d str],
}

impl<'de> DeserializeSeed<'de> for DocSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for DocSeed<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a document object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "variables" => map.next_value_seed(SourceList {
                    found: &mut *self.found,
                    declared: self.declared,
                    part: Part::Variables,
                })?,
                "annotations" => map.next_value_seed(SourceList {
                    found: &mut *self.found,
                    declared: self.declared,
                    part: Part::Annotations,
                })?,
                "sections" => map.next_value_seed(Sections { found: &mut *self.found, declared: self.declared })?,
                #[cfg(test)]
                "uid" => self.found.uid = map.next_value::<serde_json::Value>()?.as_str().map(str::to_owned),
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(())
    }
}

/// The list `variables` or `annotations`: objects whose member `source` names a source.
struct SourceList<'a, 'd> {
    found: &'a mut Found,
    declared: &'d [&'d str],
    part: Part,
}

impl<'de> DeserializeSeed<'de> for SourceList<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for SourceList<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of objects")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let (list, budget) = match self.part {
            Part::Variables => (&mut self.found.variables, MAX_VARIABLES),
            Part::Annotations => (&mut self.found.annotations, MAX_ANNOTATIONS),
        };
        let mut count = 0usize;
        while let Some(source) = seq.next_element_seed(SourceOf)? {
            count += 1;
            if count > budget {
                return Err(A::Error::custom("the list is above its budget"));
            }
            if let Some(id) = source {
                keep(list, self.declared, id);
            }
        }
        Ok(())
    }
}

/// An object: the value of its member `source` when it is a text. Each other member is skipped.
struct SourceOf;

impl<'de> DeserializeSeed<'de> for SourceOf {
    type Value = Option<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<String>, D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for SourceOf {
    type Value = Option<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Option<String>, A::Error> {
        let mut source = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "source" {
                source = Some(map.next_value::<String>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(source)
    }
}

/// The list `sections`.
struct Sections<'a, 'd> {
    found: &'a mut Found,
    declared: &'d [&'d str],
}

impl<'de> DeserializeSeed<'de> for Sections<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Sections<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of sections")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut count = 0usize;
        while seq.next_element_seed(Section { found: &mut *self.found, declared: self.declared })?.is_some() {
            count += 1;
            if count > MAX_SECTIONS {
                return Err(A::Error::custom("the list is above its budget"));
            }
        }
        Ok(())
    }
}

/// One section: its member `panels`.
struct Section<'a, 'd> {
    found: &'a mut Found,
    declared: &'d [&'d str],
}

impl<'de> DeserializeSeed<'de> for Section<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Section<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a section object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        while let Some(key) = map.next_key::<String>()? {
            if key == "panels" {
                map.next_value_seed(Panels { found: &mut *self.found, declared: self.declared })?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(())
    }
}

/// The list `panels` of a section. The count of all the panels has the budget `MAX_PANELS`.
struct Panels<'a, 'd> {
    found: &'a mut Found,
    declared: &'d [&'d str],
}

impl<'de> DeserializeSeed<'de> for Panels<'_, '_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Panels<'_, '_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of panels")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(sources) = seq.next_element_seed(PanelSeed)? {
            self.found.panel_count += 1;
            if self.found.panel_count > MAX_PANELS {
                return Err(A::Error::custom("the list is above its budget"));
            }
            for id in sources {
                keep(&mut self.found.panels, self.declared, id);
            }
        }
        Ok(())
    }
}

/// One panel: the sources of `data.query` and of `data.extra_queries`, in this order.
struct PanelSeed;

impl<'de> DeserializeSeed<'de> for PanelSeed {
    type Value = Vec<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<String>, D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for PanelSeed {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a panel object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<String>, A::Error> {
        let mut out = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            if key == "data" {
                out.extend(map.next_value_seed(DataSeed)?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(out)
    }
}

/// The member `data` of a panel.
struct DataSeed;

impl<'de> DeserializeSeed<'de> for DataSeed {
    type Value = Vec<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<String>, D::Error> {
        d.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for DataSeed {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a data object")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<String>, A::Error> {
        let mut first: Vec<String> = Vec::new();
        let mut extra: Vec<String> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "query" => first.extend(map.next_value_seed(SourceOf)?),
                "extra_queries" => extra = map.next_value_seed(ExtraSeed)?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        first.extend(extra);
        Ok(first)
    }
}

/// The list `extra_queries` of a panel: `MAX_QUERIES_PER_PANEL` minus 1 items at most (4.15a).
struct ExtraSeed;

impl<'de> DeserializeSeed<'de> for ExtraSeed {
    type Value = Vec<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Vec<String>, D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ExtraSeed {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of queries")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
        let mut out = Vec::new();
        let mut count = 0usize;
        while let Some(source) = seq.next_element_seed(SourceOf)? {
            count += 1;
            if count > MAX_QUERIES_PER_PANEL - 1 {
                return Err(A::Error::custom("the list is above its budget"));
            }
            out.extend(source);
        }
        Ok(out)
    }
}

/// The fault injection of the tests: a pre-pass that misses one source of one document, or that cannot read it.
#[cfg(test)]
pub(crate) mod tests_hook {
    use std::sync::{Mutex, PoisonError};

    /// `(uid, source id)`: the pre-pass of the document with this uid leaves out this source id.
    pub static MISS: Mutex<Option<(String, String)>> = Mutex::new(None);
    /// The uid of a document that the pre-pass reports as unreadable.
    pub static UNREADABLE: Mutex<Option<String>> = Mutex::new(None);
    /// The uid of each document that the pre-pass read, in order.
    pub static READ: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Applies the faults to the list of one document. It gives `false` for a document to report as unreadable.
    pub(super) fn apply(uid: Option<&str>, out: &mut Vec<String>) -> bool {
        let Some(uid) = uid else { return true };
        READ.lock().unwrap_or_else(PoisonError::into_inner).push(uid.to_owned());
        if let Some((u, s)) = MISS.lock().unwrap_or_else(PoisonError::into_inner).as_ref()
            && u == uid
        {
            out.retain(|x| x != s);
        }
        UNREADABLE.lock().unwrap_or_else(PoisonError::into_inner).as_deref() != Some(uid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::handlers::gate::document_sources;

    const DECLARED: [&str; 6] = ["prod", "conf", "pg", "mongo", "ch2", "s"];

    fn fixtures() -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        for dir in ["tests/fixtures/dashboards", "fuzz/corpus/document_parse", "fuzz/regression"] {
            let Ok(entries) = std::fs::read_dir(format!("{root}/{dir}")) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file()
                    && let Ok(bytes) = std::fs::read(&p)
                {
                    out.push((p.display().to_string(), bytes));
                }
            }
        }
        out
    }

    #[test]
    fn the_pre_pass_equals_the_sources_of_each_valid_document_of_the_corpus() {
        let mut valid = 0;
        for (name, bytes) in fixtures() {
            let Ok(doc) = hune_core::document::validate(&bytes, None) else { continue };
            valid += 1;
            let all = document_sources(doc.document());
            let all_ids: Vec<&str> = all.iter().map(String::as_str).collect();
            // Each id declared: the pre-pass gives the list of 11.21, in the same order.
            assert_eq!(declared_sources(&bytes, &all_ids).as_ref(), Some(&all), "{name}");
            // A fixed list of declared ids: the same list without the other ids.
            let expected: Vec<String> = all.iter().filter(|s| DECLARED.contains(&s.as_str())).cloned().collect();
            assert_eq!(declared_sources(&bytes, &DECLARED), Some(expected), "{name}");
            // No declared id: no source.
            assert_eq!(declared_sources(&bytes, &[]), Some(Vec::new()), "{name}");
        }
        eprintln!("the corpus has {valid} valid documents");
        assert!(valid >= 15, "the corpus has {valid} valid documents");
    }

    #[test]
    fn the_pre_pass_reads_the_four_paths_only_and_refuses_what_it_cannot_read() {
        let doc = br#"{"sections":[{"panels":[{"data":{"extra_queries":[{"source":"b"},{"source":"a"}],"query":{"source":"c"}}},
            {"data":{"reuse":{"panel":1}},"source":"x"}]}],"annotations":[{"source":"a","enabled":false}],
            "variables":[{"kind":"query","source":"d"},{"kind":"custom"}],"source":"y"}"#;
        let all = ["a", "b", "c", "d", "x", "y"];
        assert_eq!(declared_sources(doc, &all), Some(vec!["d".into(), "a".into(), "c".into(), "b".into()]));
        assert_eq!(declared_sources(doc, &["b"]), Some(vec!["b".to_owned()]));
        for bad in [
            &b"[]"[..],
            b"{\"variables\":{}}",
            b"{\"variables\":[{\"source\":1}]}",
            b"{\"sections\":[{\"panels\":[{\"data\":{\"query\":{\"source\":null}}}]}]}",
            b"{\"sections\":[{\"panels\":[{\"data\":{\"extra_queries\":null}}]}]}",
            b"{\"variables\":[]",
            b"{} x",
        ] {
            assert_eq!(declared_sources(bad, &all), None, "{}", String::from_utf8_lossy(bad));
        }
        let many: Vec<String> = (0..=MAX_VARIABLES).map(|_| "{}".to_owned()).collect();
        assert_eq!(declared_sources(format!("{{\"variables\":[{}]}}", many.join(",")).as_bytes(), &all), None);
        let panels: Vec<String> = (0..=MAX_PANELS).map(|_| "{}".to_owned()).collect();
        let text = format!("{{\"sections\":[{{\"panels\":[{}]}}]}}", panels.join(","));
        assert_eq!(declared_sources(text.as_bytes(), &all), None);
    }

    #[test]
    fn the_pre_pass_is_linear_on_a_large_document() {
        // 4 MiB of skipped text, deep values and a long list: one pass, no tree.
        let filler = format!("[{}]", vec!["{\"a\":[[[[1]]]],\"b\":\"$x\"}"; 140_000].join(","));
        let text = format!(
            "{{\"links\":{filler},\"sections\":[{{\"panels\":[{{\"data\":{{\"query\":{{\"source\":\"prod\"}}}}}}]}}]}}"
        );
        assert!(text.len() > 3_000_000);
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let t = std::time::Instant::now();
            assert_eq!(declared_sources(text.as_bytes(), &["prod"]), Some(vec!["prod".to_owned()]));
            best = best.min(t.elapsed());
        }
        eprintln!("pre-pass of {} bytes: {best:?}", text.len());
        assert!(best < std::time::Duration::from_secs(2), "{best:?}");
    }
}
