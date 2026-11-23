//! The bounded lists of the checks at save (6.13, 7.18): the faults and the warnings.
//!
//! The checks of 7.18 and 7.36 and the warnings 6 put their items into one [`SaveReport`]. Each list has a
//! cap (4.35): `MAX_ERRORS` faults and `MAX_WARNINGS` warnings, the last item included. When a list has its
//! cap minus 1 items and one more item comes, the list stops: one last item says so, and the next items are
//! not made. Thus the work and the memory of a hostile document stay bounded, and the caller can send the
//! two lists as they are.

use crate::error::{ErrorCode, ValidationError};
use crate::limits::{MAX_ERRORS, MAX_WARNINGS};
use serde::Serialize;
use std::borrow::Cow;

/// One warning of 6.13: the JSON pointer and the English message. A warning never stops a write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    /// The pointer of the member, for example `/sections/0/panels/2/data/query/sql/5`. `""` for the last item
    /// of a list that stopped.
    pub pointer: String,
    /// The English message.
    pub message: String,
}

impl Warning {
    /// The last item of a list of warnings that stopped (6.13, 7.18).
    pub fn list_stops() -> Warning {
        Warning {
            pointer: String::new(),
            message: format!("the list stops here; it has {MAX_WARNINGS} warnings at most"),
        }
    }

    /// Tells if this item is the last item of a list that stopped.
    pub fn is_list_stop(&self) -> bool {
        self.pointer.is_empty() && *self == Warning::list_stops()
    }
}

/// The most characters of a value that the message of a warning shows, as for an error (4.37).
const MAX_SHOWN: usize = 200;

/// Cuts a value of a message to 200 characters. A cut value ends with three dots inside the limit. Thus a
/// list of `MAX_WARNINGS` warnings stays small for any document.
pub(crate) fn shown(text: &str) -> Cow<'_, str> {
    match text.char_indices().nth(MAX_SHOWN) {
        None => Cow::Borrowed(text),
        Some(_) => {
            let cut = text.char_indices().nth(MAX_SHOWN - 3).map_or(text.len(), |(i, _)| i);
            Cow::Owned(format!("{}...", &text[..cut]))
        }
    }
}

/// The last item of a list of faults that stopped: the code `too_many_errors` (4.36, step 7).
pub fn too_many_errors() -> ValidationError {
    ValidationError::new(
        "",
        ErrorCode::TooManyErrors,
        format!("the list stops here; it has {MAX_ERRORS} errors at most"),
    )
}

/// The faults and the warnings of the checks at save, each list with its cap (4.35).
///
/// The faults are the items of `invalid_document` (4.37). The warnings are the member `warnings` of 6.13 and
/// 6.15. A list holds at most its cap of items. When it has its cap minus 1 items and one more item comes, the
/// list gets its last item (code `too_many_errors`, or [`Warning::list_stops`]) and takes no more items.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveReport {
    errors: Vec<ValidationError>,
    errors_stopped: bool,
    warnings: Vec<Warning>,
    warnings_stopped: bool,
}

impl SaveReport {
    /// Makes two empty lists.
    pub fn new() -> SaveReport {
        SaveReport::default()
    }

    /// Gives the number of faults that the list can still take before its last item. It is 0 when the list
    /// stopped.
    pub fn error_room(&self) -> usize {
        if self.errors_stopped { 0 } else { (MAX_ERRORS - 1).saturating_sub(self.errors.len()) }
    }

    /// Gives the number of warnings that the list can still take before its last item. It is 0 when the list
    /// stopped.
    pub fn warning_room(&self) -> usize {
        if self.warnings_stopped { 0 } else { (MAX_WARNINGS - 1).saturating_sub(self.warnings.len()) }
    }

    /// Adds one fault. Without room, the list stops: the item becomes `too_many_errors`.
    pub fn push_error(&mut self, error: ValidationError) {
        self.error_with(|| error);
    }

    /// Adds the fault that `make` gives. `make` runs only when the list has room, so a full list makes
    /// nothing. Without room, the list stops.
    pub fn error_with(&mut self, make: impl FnOnce() -> ValidationError) {
        if self.error_room() > 0 {
            self.errors.push(make());
        } else {
            self.stop_errors();
        }
    }

    /// Adds one warning. Without room, the list stops: the item becomes [`Warning::list_stops`].
    pub fn push_warning(&mut self, warning: Warning) {
        self.warning_with(|| warning);
    }

    /// Adds the warning that `make` gives. `make` runs only when the list has room. Without room, the list
    /// stops.
    pub fn warning_with(&mut self, make: impl FnOnce() -> Warning) {
        if self.warning_room() > 0 {
            self.warnings.push(make());
        } else {
            self.stop_warnings();
        }
    }

    /// Stops the list of faults: it adds the last item `too_many_errors` one time. A check calls it when it
    /// found more faults than [`SaveReport::error_room`] and did not make them.
    pub fn stop_errors(&mut self) {
        if !self.errors_stopped {
            self.errors_stopped = true;
            self.errors.push(too_many_errors());
        }
    }

    /// Stops the list of warnings: it adds the last item [`Warning::list_stops`] one time.
    pub fn stop_warnings(&mut self) {
        if !self.warnings_stopped {
            self.warnings_stopped = true;
            self.warnings.push(Warning::list_stops());
        }
    }

    /// Tells if the list of faults stopped. Then the write fails, and no other check changes the answer.
    pub fn errors_stopped(&self) -> bool {
        self.errors_stopped
    }

    /// Tells if the list of warnings stopped.
    pub fn warnings_stopped(&self) -> bool {
        self.warnings_stopped
    }

    /// The faults, in the order of the checks.
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// The warnings, in the order of the checks.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Gives the two lists: the faults, then the warnings.
    pub fn into_parts(self) -> (Vec<ValidationError>, Vec<Warning>) {
        (self.errors, self.warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_stops_at_its_cap_with_one_last_item() {
        let mut r = SaveReport::new();
        assert_eq!((r.error_room(), r.warning_room()), (MAX_ERRORS - 1, MAX_WARNINGS - 1));
        for i in 0..1000 {
            r.push_error(ValidationError::new(format!("/e/{i}"), ErrorCode::BadFormat, "x"));
            r.push_warning(Warning { pointer: format!("/w/{i}"), message: "y".to_owned() });
        }
        assert_eq!(r.errors().len(), MAX_ERRORS);
        assert_eq!(r.warnings().len(), MAX_WARNINGS);
        assert_eq!(r.errors()[MAX_ERRORS - 2].pointer, format!("/e/{}", MAX_ERRORS - 2));
        assert_eq!(r.errors().last().unwrap().code, ErrorCode::TooManyErrors);
        assert!(r.warnings().last().unwrap().is_list_stop());
        assert!(r.errors_stopped() && r.warnings_stopped());
        assert_eq!((r.error_room(), r.warning_room()), (0, 0));
    }

    #[test]
    fn a_value_in_a_message_has_200_characters_at_most() {
        assert_eq!(shown("$abc"), "$abc");
        let exact = "é".repeat(200);
        assert_eq!(shown(&exact), exact);
        let long = format!("${}", "é".repeat(5000));
        let cut = shown(&long);
        assert_eq!(cut.chars().count(), 200);
        assert!(cut.starts_with("$é") && cut.ends_with("..."));
    }

    #[test]
    fn a_list_with_room_makes_no_last_item_and_a_full_list_makes_nothing() {
        let mut r = SaveReport::new();
        for i in 0..MAX_WARNINGS - 1 {
            r.push_warning(Warning { pointer: format!("/w/{i}"), message: "y".to_owned() });
        }
        assert!(!r.warnings_stopped());
        assert_eq!(r.warnings().len(), MAX_WARNINGS - 1);
        r.warning_with(|| panic!("a full list makes no item"));
        r.warning_with(|| panic!("a stopped list makes no item"));
        assert_eq!(r.warnings().len(), MAX_WARNINGS);
        r.stop_warnings();
        assert_eq!(r.warnings().len(), MAX_WARNINGS);
        let (errors, warnings) = r.into_parts();
        assert!(errors.is_empty());
        assert!(warnings.last().unwrap().is_list_stop());
    }
}
