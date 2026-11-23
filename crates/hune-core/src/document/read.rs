// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers that read typed members from the JSON tree and collect errors (4.36, step 5).

use super::json::Json;
use super::model::TextEnum;
use crate::error::{ErrorCode, ValidationError, pointer_token};
use crate::limits::MAX_ERRORS;
use std::borrow::Cow;
use std::rc::Rc;

/// The most characters of a value that a message shows (4.37).
const MAX_SHOWN: usize = 200;

/// Cuts a text to `MAX_SHOWN` characters. A cut text ends with three dots inside the limit.
pub(crate) fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_SHOWN {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(MAX_SHOWN - 3).collect();
    out.push_str("...");
    out
}

/// Gives a text for a JSON value in a message: a string in quotes, a number as its text.
pub(crate) fn show(j: &Json) -> String {
    match j {
        Json::Null => "null".to_owned(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => clip(n.text()),
        Json::Str(s) => clip(&format!("{s:?}")),
        Json::Arr(a) => format!("an array of {} items", a.len()),
        Json::Obj(o) => format!("an object of {} members", o.len()),
    }
}

/// A source id, a name or another value in quotes for a message.
pub(crate) fn quoted(s: &str) -> String {
    clip(&format!("{s:?}"))
}

/// The collected errors and the facts that the rules across members need.
#[derive(Default)]
pub(crate) struct Ctx {
    pub errors: Vec<ValidationError>,
    stopped: bool,
    pub panels: Vec<PanelMeta>,
    pub variables: Vec<VariableMeta>,
    /// The facts about each annotation that the rules across members need (4.5b, 4.34).
    pub annotations: Vec<AnnotationMeta>,
    pub panel_total: usize,
    /// The number of queries of the panels, hidden queries included (4.15a, rule 6).
    pub query_total: usize,
    /// The number of sections, when the array read. A placement checks its section index against it.
    pub section_count: Option<usize>,
    /// The facts about each section that the rules of the repeat need (4.13a).
    pub sections: Vec<SectionMeta>,
}

/// What the rules across panels need from one panel (4.34).
pub(crate) struct PanelMeta {
    pub pointer: String,
    pub section: usize,
    /// The type of the panel, when it read.
    pub panel_type: Option<super::model::PanelType>,
    /// The pointer of the member `data/extra_queries`, when the panel has it (4.15a).
    pub extra_pointer: Option<String>,
    pub id: Option<u32>,
    pub grid: Option<super::model::Grid>,
    /// `Some(true)`: the panel has `query`. `Some(false)`: it has `reuse`.
    pub is_query: Option<bool>,
    /// `true`: the panel has the type `text`. It has no query, so `is_query` is `None` (4.33a).
    pub is_text: bool,
    pub reuse_target: Option<u32>,
    /// The member `repeat`, when it read as a valid object (4.13a).
    pub repeat: Option<RepeatMeta>,
}

/// What the rules across annotations need from one annotation (4.34).
pub(crate) struct AnnotationMeta {
    pub pointer: String,
    pub name: Option<String>,
    /// The ids of the member `panels` that read, each with its pointer.
    pub panels: Vec<(String, u32)>,
}

/// A `repeat` that read as a valid object: the rules across members check its variable (4.34).
pub(crate) struct RepeatMeta {
    /// The pointer of the object `repeat`.
    pub pointer: String,
    /// The name of the variable.
    pub variable: String,
    /// The direction of a panel repeat. `None` for a section.
    pub direction: Option<super::model::RepeatDirection>,
}

/// What the rules of the repeat need from one section (4.13a).
pub(crate) struct SectionMeta {
    pub index: usize,
    /// `Some(true)`: the title is `null`. `None`: the title did not read.
    pub title_null: Option<bool>,
    /// The member `repeat`, when it read as a valid object.
    pub repeat: Option<RepeatMeta>,
}

/// The place of a variable control that the rules across members check (4.34).
pub(crate) struct PlacementMeta {
    pub section: u32,
    pub grid: super::model::Grid,
}

/// What the rules across variables need from one variable (4.34).
pub(crate) struct VariableMeta {
    pub pointer: String,
    pub name: Option<String>,
    /// The joined SQL text of a `query` variable.
    pub sql: Option<String>,
    /// The member `placement`, when it read as a valid object.
    pub placement: Option<PlacementMeta>,
    /// `Some(true)`: the kind is `query` or `custom` and `multi` is `true`, so the variable can repeat a
    /// panel or a section (4.13a). `None`: the kind or `multi` did not read.
    pub repeatable: Option<bool>,
    /// The source of an `adhoc` variable, when it read (4.10c, rule 1).
    pub adhoc_source: Option<String>,
}

impl Ctx {
    /// Adds an error. At `MAX_ERRORS` minus 1 errors, the next error becomes `too_many_errors`
    /// and the list is closed (4.36, step 7).
    pub fn push(&mut self, pointer: &str, code: ErrorCode, message: String) {
        if self.stopped {
            return;
        }
        if self.errors.len() + 1 >= MAX_ERRORS {
            self.stopped = true;
            self.errors.push(ValidationError::new(
                "",
                ErrorCode::TooManyErrors,
                format!("the list stops here; it has {} errors at most", MAX_ERRORS),
            ));
            return;
        }
        self.errors.push(ValidationError::new(pointer, code, message));
    }

    /// Tells if the list is closed.
    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

/// How to count and check a text.
#[derive(Clone, Copy)]
pub(crate) struct TextRule {
    pub min: usize,
    pub max: usize,
    /// `true`: the limits are in bytes. `false`: in characters.
    pub bytes: bool,
    /// `true`: U+0009 is permitted (a line of SQL).
    pub allow_tab: bool,
}

impl TextRule {
    pub const fn chars(min: usize, max: usize) -> Self {
        TextRule { min, max, bytes: false, allow_tab: false }
    }

    pub const fn bytes(min: usize, max: usize) -> Self {
        TextRule { min, max, bytes: true, allow_tab: false }
    }
}

/// The pointer and the name of the items of one array. All the items share it.
struct ItemBase {
    /// The pointer of the array.
    ptr: String,
    /// The name of an item for the messages: `<array name> item`.
    name: String,
}

/// Where a field is in the document.
enum At {
    /// A member of an object, with its pointer and its name.
    Member { ptr: String, name: String },
    /// An item of an array. The pointer is made only when a message or a fact needs it, so an array of many
    /// items costs no text for each item.
    Item { base: Rc<ItemBase>, index: usize },
}

/// A member or an item with its pointer and its name for the messages.
pub(crate) struct Field<'a> {
    pub json: &'a Json,
    at: At,
}

impl<'a> Field<'a> {
    /// The JSON pointer of the field.
    pub fn ptr(&self) -> Cow<'_, str> {
        match &self.at {
            At::Member { ptr, .. } => Cow::Borrowed(ptr),
            At::Item { base, index } => Cow::Owned(format!("{}/{index}", base.ptr)),
        }
    }

    /// The name of the field for the messages.
    pub fn name(&self) -> &str {
        match &self.at {
            At::Member { name, .. } => name,
            At::Item { base, .. } => &base.name,
        }
    }

    /// Adds an error about this field. The message is the name, the rule and the value. A closed list makes
    /// no text.
    pub fn fail(&self, cx: &mut Ctx, code: ErrorCode, rule: &str) {
        if cx.stopped() {
            return;
        }
        cx.push(&self.ptr(), code, format!("{} {}; value {}", self.name(), rule, show(self.json)));
    }

    pub fn is_null(&self) -> bool {
        matches!(self.json, Json::Null)
    }

    /// Reads a text with a rule on its length (4.2). A control character is `bad_format`.
    pub fn text(&self, cx: &mut Ctx, rule: TextRule) -> Option<String> {
        self.check_text(cx, rule).map(str::to_owned)
    }

    /// Checks a text as [`Field::text`] does, and gives it with no copy.
    pub fn check_text(&self, cx: &mut Ctx, rule: TextRule) -> Option<&'a str> {
        let Json::Str(s) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be text");
            return None;
        };
        let forbidden = |c: char| (c <= '\u{1f}' && !(rule.allow_tab && c == '\t')) || c == '\u{7f}';
        if let Some(c) = s.chars().find(|&c| forbidden(c)) {
            let what = if rule.allow_tab { "a control character other than a tab" } else { "a control character" };
            cx.push(
                &self.ptr(),
                ErrorCode::BadFormat,
                format!(
                    "{} must not contain {what}; found U+{:04X} in value {}",
                    self.name(),
                    c as u32,
                    show(self.json)
                ),
            );
            return None;
        }
        let len = if rule.bytes { s.len() } else { s.chars().count() };
        if len < rule.min || len > rule.max {
            let unit = if rule.bytes { "bytes" } else { "characters" };
            let range = if rule.max == usize::MAX {
                format!("at least {}", rule.min)
            } else {
                format!("{} to {}", rule.min, rule.max)
            };
            self.fail(cx, ErrorCode::OutOfRange, &format!("must be {range} {unit}"));
            return None;
        }
        Some(s)
    }

    /// Reads an integer in `min..=max`. A number with a fraction or an exponent is `wrong_type`.
    pub fn int(&self, cx: &mut Ctx, min: i64, max: i64) -> Option<i64> {
        let Json::Num(n) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be an integer");
            return None;
        };
        if !n.is_integer() {
            self.fail(cx, ErrorCode::WrongType, "must be an integer");
            return None;
        }
        match n.as_i64() {
            Some(v) if (min..=max).contains(&v) => Some(v),
            _ => {
                self.fail(cx, ErrorCode::OutOfRange, &format!("must be {min} to {max}"));
                None
            }
        }
    }

    /// Reads a number. The double MUST be finite (4.2).
    pub fn num(&self, cx: &mut Ctx) -> Option<f64> {
        let Json::Num(n) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be a number");
            return None;
        };
        match n.as_f64() {
            Some(v) => Some(v),
            None => {
                self.fail(cx, ErrorCode::OutOfRange, "must be a finite number");
                None
            }
        }
    }

    pub fn boolean(&self, cx: &mut Ctx) -> Option<bool> {
        match self.json {
            Json::Bool(b) => Some(*b),
            _ => {
                self.fail(cx, ErrorCode::WrongType, "must be true or false");
                None
            }
        }
    }

    /// Reads an enum. A text outside the list is `bad_format`.
    pub fn en<E: TextEnum>(&self, cx: &mut Ctx) -> Option<E> {
        let Json::Str(s) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be text");
            return None;
        };
        let parsed = E::parse(s);
        if parsed.is_none() {
            self.fail(cx, ErrorCode::BadFormat, &format!("must be one of {}", E::NAMES.join(", ")));
        }
        parsed
    }

    /// Reads an array with a count rule. A count above `max` gives `over`. A count below
    /// `min` gives `out_of_range`.
    ///
    /// The items share one pointer and one name: an item makes its pointer only when a message needs it. Thus
    /// an array of many items costs no memory for each item (4.36).
    pub fn items(&self, cx: &mut Ctx, min: usize, max: usize, over: ErrorCode) -> Option<Items<'a>> {
        let Json::Arr(a) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be an array");
            return None;
        };
        if a.len() > max || a.len() < min {
            let code = if a.len() > max { over } else { ErrorCode::OutOfRange };
            let name = self.name();
            cx.push(&self.ptr(), code, format!("{name} must have {min} to {max} items; value {} items", a.len()));
            return None;
        }
        let base = ItemBase { ptr: self.ptr().into_owned(), name: format!("{} item", self.name()) };
        Some(Items { json: a, base: Rc::new(base) })
    }

    /// Reads an object.
    pub fn object(&self, cx: &mut Ctx, label: &'static str) -> Option<Obj<'a>> {
        let Json::Obj(items) = self.json else {
            self.fail(cx, ErrorCode::WrongType, "must be an object");
            return None;
        };
        Some(Obj { items, taken: vec![false; items.len()], ptr: self.ptr().into_owned(), label })
    }

    /// Reads a member that has the value `null` or a value of a type: `null` gives `Some(None)`.
    pub fn nullable<T>(&self, f: impl FnOnce(&Self) -> Option<T>) -> Option<Option<T>> {
        if self.is_null() { Some(None) } else { f(self).map(Some) }
    }
}

/// The items of an array (4.36). A field of an item is made when the walk reads it, with no copy of a text.
pub(crate) struct Items<'a> {
    json: &'a [Json],
    base: Rc<ItemBase>,
}

impl<'a> Items<'a> {
    /// The number of items.
    pub fn len(&self) -> usize {
        self.json.len()
    }

    /// The field of the item at `index`.
    pub fn get(&self, index: usize) -> Option<Field<'a>> {
        let json = self.json.get(index)?;
        Some(Field { json, at: At::Item { base: Rc::clone(&self.base), index } })
    }

    /// The fields of the items, in order.
    pub fn iter(&self) -> ItemsIter<'a, '_> {
        ItemsIter { items: self, next: 0 }
    }
}

/// The iterator of [`Items::iter`].
pub(crate) struct ItemsIter<'a, 'b> {
    items: &'b Items<'a>,
    next: usize,
}

impl<'a> Iterator for ItemsIter<'a, '_> {
    type Item = Field<'a>;

    fn next(&mut self) -> Option<Field<'a>> {
        let field = self.items.get(self.next)?;
        self.next += 1;
        Some(field)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.items.len().saturating_sub(self.next);
        (left, Some(left))
    }
}

impl ExactSizeIterator for ItemsIter<'_, '_> {}

impl<'a, 'b> IntoIterator for &'b Items<'a> {
    type Item = Field<'a>;
    type IntoIter = ItemsIter<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An object of the tree. The members that the walk reads are marked. The others are unknown.
pub(crate) struct Obj<'a> {
    items: &'a [(String, Json)],
    taken: Vec<bool>,
    pub ptr: String,
    label: &'static str,
}

impl<'a> Obj<'a> {
    /// Makes the object of the whole document.
    pub fn root(json: &'a Json, label: &'static str) -> Option<Obj<'a>> {
        match json {
            Json::Obj(items) => Some(Obj { items, taken: vec![false; items.len()], ptr: String::new(), label }),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    fn find(&mut self, name: &str) -> Option<Field<'a>> {
        let index = self.items.iter().position(|(k, _)| k == name)?;
        if let Some(t) = self.taken.get_mut(index) {
            *t = true;
        }
        let (_, json) = self.items.get(index)?;
        let at = At::Member { ptr: format!("{}/{}", self.ptr, pointer_token(name)), name: name.to_owned() };
        Some(Field { json, at })
    }

    /// Takes a member of the table that MUST be present.
    pub fn get(&mut self, cx: &mut Ctx, name: &str) -> Option<Field<'a>> {
        let found = self.find(name);
        if found.is_none() {
            cx.push(&self.ptr, ErrorCode::MissingMember, format!("member {name} is necessary in {}", self.label));
        }
        found
    }

    /// Takes a member that can be absent.
    pub fn opt(&mut self, name: &str) -> Option<Field<'a>> {
        self.find(name)
    }

    /// Marks every member as read. Use it when the walk cannot go on.
    pub fn skip_all(&mut self) {
        self.taken.iter_mut().for_each(|t| *t = true);
    }

    /// Adds one `unknown_member` error for each member that no table member took, in input
    /// order (4.36).
    pub fn finish(self, cx: &mut Ctx, permitted: &[&str]) {
        for ((key, _), taken) in self.items.iter().zip(&self.taken) {
            if !taken {
                cx.push(
                    &format!("{}/{}", self.ptr, pointer_token(key)),
                    ErrorCode::UnknownMember,
                    format!(
                        "member {} is not permitted in {}; permitted members: {}",
                        clip(key),
                        self.label,
                        permitted.join(", ")
                    ),
                );
            }
        }
    }
}

/// Shortcuts: take a necessary member and read it with one type.
impl<'a> Obj<'a> {
    /// Takes a necessary text member.
    pub fn text(&mut self, cx: &mut Ctx, name: &str, rule: TextRule) -> Option<String> {
        self.get(cx, name).and_then(|f| f.text(cx, rule))
    }

    /// Takes a necessary boolean member.
    pub fn boolean(&mut self, cx: &mut Ctx, name: &str) -> Option<bool> {
        self.get(cx, name).and_then(|f| f.boolean(cx))
    }

    /// Takes a necessary enum member.
    pub fn en<E: TextEnum>(&mut self, cx: &mut Ctx, name: &str) -> Option<E> {
        self.get(cx, name).and_then(|f| f.en::<E>(cx))
    }

    /// Takes a necessary integer member.
    pub fn int(&mut self, cx: &mut Ctx, name: &str, min: i64, max: i64) -> Option<i64> {
        self.get(cx, name).and_then(|f| f.int(cx, min, max))
    }

    /// Takes a necessary number member.
    pub fn num(&mut self, cx: &mut Ctx, name: &str) -> Option<f64> {
        self.get(cx, name).and_then(|f| f.num(cx))
    }
}
