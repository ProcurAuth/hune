//! The variable names that the saved queries of a document name (6.14, 7.19).
//!
//! The server is the authority for these names. The page reads them in the answer of `GET /api/dashboards/{uid}`
//! and never scans a query text. The set is a pure function of the document: it runs no query and reads no file.

use super::cross::dependency_names;
use super::model::{Document, PanelData, VariableKind};
use std::collections::HashSet;

/// The variable names that the queries of a document name, for each saved query (6.14).
///
/// Each list holds distinct names of variables of the document, in the order of their first use in the text.
/// A macro name, a name that no variable has and the own name of a query variable never appear in a list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentNames {
    /// For each panel with a query, in document order: the panel id, then one list for each query of the panel.
    /// The first list is `data.query`, then one list for each item of `data.extra_queries` (4.15a).
    pub panels: Vec<(u32, Vec<Vec<String>>)>,
    /// For each `query` variable, in document order: the name of the variable and the names that its SQL names.
    pub variables: Vec<(String, Vec<String>)>,
    /// For each annotation, in document order: the names that its SQL names.
    pub annotations: Vec<Vec<String>>,
}

/// Gives the distinct names of `known` that the lines of one saved query name, without `skip`.
fn names_in(lines: &[String], known: &HashSet<&str>, skip: Option<&str>) -> Vec<String> {
    names_with_adhoc(lines, known, skip, None)
}

/// Gives the names of [`names_in`]. The macro `$__adhocFilters` names `adhoc`: the `adhoc` variable of the source of the
/// query, when the document has one (4.10c, 6.14). The page then runs the query again after each change of its filters.
fn names_with_adhoc(lines: &[String], known: &HashSet<&str>, skip: Option<&str>, adhoc: Option<&str>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for name in dependency_names(&lines.join("\n")) {
        let name = match adhoc {
            Some(variable) if name == crate::sql::adhoc::MACRO_NAME => variable.to_owned(),
            _ => name,
        };
        if known.contains(name.as_str()) && Some(name.as_str()) != skip && seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out
}

impl DocumentNames {
    /// Computes the names of a valid document (7.19). The cost is linear in the size of the saved queries.
    pub fn of(doc: &Document) -> Self {
        let known: HashSet<&str> = doc.variables.iter().map(|v| v.name.as_str()).collect();
        let adhoc_of = |source: &str| {
            doc.variables.iter().find_map(|v| match &v.kind {
                VariableKind::Adhoc { source: s, .. } if s.as_str() == source => Some(v.name.as_str()),
                _ => None,
            })
        };
        let panels = doc
            .sections
            .iter()
            .flat_map(|s| &s.panels)
            .filter_map(|p| match &p.data {
                PanelData::Query(q) => Some((
                    p.id,
                    q.queries()
                        .iter()
                        .map(|view| names_with_adhoc(view.sql, &known, None, adhoc_of(view.source.as_str())))
                        .collect(),
                )),
                PanelData::None | PanelData::Reuse { .. } => None,
            })
            .collect();
        let variables = doc
            .variables
            .iter()
            .filter_map(|v| match &v.kind {
                VariableKind::Query { sql, .. } => Some((v.name.clone(), names_in(sql, &known, Some(&v.name)))),
                _ => None,
            })
            .collect();
        let annotations = doc.annotations.iter().map(|a| names_in(&a.sql, &known, None)).collect();
        Self { panels, variables, annotations }
    }

    /// Writes the member of the answer of 6.14 as compact JSON: `{"panels":{..},"variables":{..},"annotations":[..]}`.
    pub fn to_json(&self) -> String {
        let list = |names: &[String]| {
            let items: Vec<String> = names.iter().map(|n| serde_json::Value::String(n.clone()).to_string()).collect();
            format!("[{}]", items.join(","))
        };
        let panels: Vec<String> = self
            .panels
            .iter()
            .map(|(id, queries)| {
                let lists: Vec<String> = queries.iter().map(|l| list(l)).collect();
                format!("\"{id}\":[{}]", lists.join(","))
            })
            .collect();
        let variables: Vec<String> = self
            .variables
            .iter()
            .map(|(name, names)| format!("{}:{}", serde_json::Value::String(name.clone()), list(names)))
            .collect();
        let annotations: Vec<String> = self.annotations.iter().map(|l| list(l)).collect();
        format!(
            "{{\"panels\":{{{}}},\"variables\":{{{}}},\"annotations\":[{}]}}",
            panels.join(","),
            variables.join(","),
            annotations.join(",")
        )
    }
}
