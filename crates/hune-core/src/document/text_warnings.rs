//! Warning 6 of 7.18: a placeholder in a display text whose name is not a variable of the document.
//!
//! The texts are the `title` and the `description` of a panel, the `title` of a section, and the
//! `title` of a panel link and of a data link (9.13). A name that starts with `__` is a built-in
//! name: it gives no warning. A `${` that does not match the grammar of 7.6 gives no warning.
//!
//! The list of warnings has a cap (`MAX_WARNINGS`, 4.35). The walk makes a warning only when the list has
//! room, and it stops when the list stops. Thus the memory stays low for any document.

use super::model::{Document, Link, PropertySet};
use crate::report::{SaveReport, Warning, shown};
use crate::sql::placeholders::{self, Found};

/// One warning 6: the JSON pointer of the text and the message. It is the warning of 6.13.
pub type TextWarning = Warning;

/// Lists the warnings 6 of a document, in document order: the sections and their panels, with the
/// header links and the data links of each panel. The list holds at most `MAX_WARNINGS` items (4.35). When
/// the document has more, the last item is [`Warning::list_stops`].
pub fn text_warnings(doc: &Document) -> Vec<TextWarning> {
    let mut report = SaveReport::new();
    text_warnings_into(doc, &mut report);
    report.into_parts().1
}

/// Puts the warnings 6 of a document into the list of `report`, in document order. The walk makes an item
/// only when the list has room, and it stops when the list stops.
pub fn text_warnings_into(doc: &Document, report: &mut SaveReport) {
    if report.warnings_stopped() {
        return;
    }
    let mut w = Walk { doc, report };
    for (si, section) in doc.sections().iter().enumerate() {
        if let Some(title) = &section.title
            && !w.check(|| format!("/sections/{si}/title"), title)
        {
            return;
        }
        for (pi, p) in section.panels.iter().enumerate() {
            let base = || format!("/sections/{si}/panels/{pi}");
            if !w.check(|| format!("{}/title", base()), &p.title)
                || !w.check(|| format!("{}/description", base()), &p.description)
                || !w.links(&base, &p.links)
                || !w.set_links(&|| format!("{}/fields/defaults", base()), &p.fields.defaults)
            {
                return;
            }
            for (oi, o) in p.fields.overrides.iter().enumerate() {
                if !w.set_links(&|| format!("{}/fields/overrides/{oi}/set", base()), &o.set) {
                    return;
                }
            }
        }
    }
}

/// The walk of the display texts. Each method gives `false` when the list stopped: then no other text can
/// add an item, and the walk ends.
struct Walk<'a> {
    doc: &'a Document,
    report: &'a mut SaveReport,
}

impl Walk<'_> {
    /// Checks the links of a property set. `at` makes the pointer of the set.
    fn set_links(&mut self, at: &dyn Fn() -> String, set: &PropertySet) -> bool {
        match &set.links {
            Some(l) => self.links(at, l),
            None => true,
        }
    }

    /// Checks the title of each link. `at` makes the pointer of the owner of the links.
    fn links(&mut self, at: &dyn Fn() -> String, links: &[Link]) -> bool {
        for (li, l) in links.iter().enumerate() {
            if !self.check(|| format!("{}/links/{li}/title", at()), &l.title) {
                return false;
            }
        }
        true
    }

    /// Checks one display text. `pointer` makes the pointer of the text, only for a warning that the list
    /// takes.
    fn check(&mut self, pointer: impl Fn() -> String, text: &str) -> bool {
        let mut at = 0;
        while let Some(found) = placeholders::next(text, at, text.len()) {
            at = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.name.starts_with("__")
                && self.doc.variable(ph.name).is_none()
            {
                self.report.warning_with(|| Warning {
                    pointer: pointer(),
                    message: format!("{} is not a variable of the document", shown(&text[ph.start..ph.end])),
                });
                if self.report.warnings_stopped() {
                    return false;
                }
            }
        }
        true
    }
}
