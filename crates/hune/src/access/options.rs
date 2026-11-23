// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options route (11.26): the children of a node, page by page, for a form of grants.
//!
//! `parse_options_query` checks the query string. `walk_options` asks the display questions of each
//! child (P 5.6) and builds the page and its cursor. The handler gives the children from the catalog.

use std::sync::Arc;

use procurauth_core::options::OptionItem;
use procurauth_core::path::{NodePath, format_below_service};
use procurauth_pep::operation::Permission;
use procurauth_pep::options::OptionsQuery;

use super::{Access, Who};
use crate::config::{base64url_decode, base64url_encode};
use crate::server::error::{ApiError, Code};

/// The most items of one page (11.26).
pub const MAX_ITEMS: usize = 50;
/// The longest search text, in bytes (11.26).
pub const MAX_Q_BYTES: usize = 256;
/// The longest cursor, in bytes (11.26).
pub const MAX_CURSOR_BYTES: usize = 2048;

/// One child that the catalog holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionChild {
    /// The id of the child.
    pub id: String,
    /// The label, cut to 60 characters (11.26).
    pub label: String,
    /// True for a folder.
    pub has_children: bool,
}

/// The query of a request, after the checks of 11.26.
#[derive(Debug, Clone)]
pub struct ParsedOptions {
    /// The query that the library read.
    pub query: OptionsQuery,
    /// The path form of the parent, without the service: `folder:transit`, or empty.
    pub parent_text: String,
    /// The id after which the page starts: the last visible id of the previous page, or `None`.
    pub after_id: Option<String>,
    /// The count of the candidates that the previous page examined and hid after `after_id`.
    pub after_skip: usize,
}

/// The answer of the walk.
#[derive(Debug, Clone)]
pub struct OptionsWalk {
    /// The items that the subject can see.
    pub items: Vec<OptionItem>,
    /// The cursor of the next page, or `None` on the last page.
    pub next: Option<String>,
    /// The number of questions that the walk asked.
    pub questions: usize,
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError::new(Code::InvalidOptionsRequest, message)
}

/// The most digits in the count of a cursor.
const MAX_SKIP_DIGITS: usize = 9;

/// The cursor (11.26): the base64url text of `<type> \n <parent> \n <last visible id> \n <count>`.
///
/// The id is the last item that the subject can see, or empty before the first visible item. The count is the
/// number of children that the walk examined and hid after it. The cursor never holds the id of a child that
/// the subject cannot see.
pub fn make_cursor(type_name: &str, parent_text: &str, last_visible: Option<&str>, skip: usize) -> String {
    base64url_encode(format!("{type_name}\n{parent_text}\n{}\n{skip}", last_visible.unwrap_or("")).as_bytes())
}

/// Reads the raw query string of the route (11.26). The library reads and decodes the parameters;
/// Hune adds the bounds of `q` and of the cursor, and checks that the cursor is its own.
pub fn parse_options_query(raw: Option<&str>, access: &Access) -> Result<ParsedOptions, ApiError> {
    let query = OptionsQuery::parse(raw.unwrap_or(""), access.pep().document())
        .map_err(|e| bad(format!("The query of the options route is not valid: {e}.")))?;
    if query.q.as_ref().is_some_and(|q| q.len() > MAX_Q_BYTES) {
        return Err(bad("The parameter q has more than 256 bytes."));
    }
    let parent_text = format_below_service(&query.parent.nodes()[1..]);
    let (after_id, after_skip) = match &query.after {
        None => (None, 0),
        Some(cursor) => {
            if cursor.len() > MAX_CURSOR_BYTES {
                return Err(bad("The cursor has more than 2048 bytes."));
            }
            let own = base64url_decode(cursor).and_then(|b| String::from_utf8(b).ok()).and_then(|text| {
                let mut parts = text.splitn(4, '\n');
                let (t, p, id, skip) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
                let canonical = !skip.is_empty()
                    && skip.len() <= MAX_SKIP_DIGITS
                    && skip.bytes().all(|b| b.is_ascii_digit())
                    && (skip == "0" || !skip.starts_with('0'));
                if t != query.type_name || p != parent_text || !canonical {
                    return None;
                }
                Some(((!id.is_empty()).then(|| id.to_owned()), skip.parse::<usize>().ok()?))
            });
            own.ok_or_else(|| bad("The cursor is not a cursor of this request."))?
        }
    };
    Ok(ParsedOptions { query, parent_text, after_id, after_skip })
}

fn fold(text: &str) -> String {
    text.to_ascii_lowercase()
}

/// Cuts a label to 60 characters.
pub fn cut_label(text: &str) -> String {
    text.chars().take(60).collect()
}

/// Walks the children in the order of the id, and asks the display questions (11.26). The children
/// are all the children of the parent with the type of the request, in byte order of the id.
pub async fn walk_options(
    access: &Arc<Access>,
    who: &Who,
    parsed: &ParsedOptions,
    children: Vec<OptionChild>,
) -> Result<OptionsWalk, ApiError> {
    let type_name = parsed.query.type_name.as_str();
    let actions: Vec<&str> = match (type_name, access.editing()) {
        ("source", false) => Vec::new(),
        ("source", true) => vec!["source.query"],
        (_, false) => vec!["dashboard.read"],
        (_, true) => vec!["dashboard.read", "dashboard.write"],
    };
    let empty = OptionsWalk { items: Vec::new(), next: None, questions: 0 };
    if actions.is_empty() {
        return Ok(empty);
    }
    let q = parsed.query.q.as_deref().map(fold);
    let candidates: Vec<OptionChild> = children
        .into_iter()
        .filter(|c| parsed.after_id.as_deref().is_none_or(|a| c.id.as_bytes() > a.as_bytes()))
        .filter(|c| q.as_deref().is_none_or(|q| fold(&c.id).contains(q) || fold(&c.label).contains(q)))
        .skip(parsed.after_skip)
        .collect();
    let page_size = parsed.query.limit.map_or(MAX_ITEMS, |l| l.min(MAX_ITEMS));
    let budget = access.pep().display_max_questions();
    let node_of = |child: &OptionChild| -> NodePath {
        parsed.query.parent.child(procurauth_pep::operation::NodeRef::new(type_name, &child.id))
    };
    let mut items: Vec<OptionItem> = Vec::new();
    let mut questions = 0usize;
    let mut idx = 0usize;
    // The index of the last visible candidate of this page.
    let mut last_visible: Option<usize> = None;
    while items.len() < page_size && idx < candidates.len() && questions + actions.len() <= budget {
        // The worst case asks each action for each child, so the budget always covers a window.
        let take = (page_size - items.len()).min((budget - questions) / actions.len()).min(candidates.len() - idx);
        let window = &candidates[idx..idx + take];
        let perms: Vec<Permission> = window.iter().map(|c| Permission::new(actions[0], node_of(c))).collect();
        let mut visible = access.display(who, perms).await?;
        questions += take;
        let mut end = take;
        for action in &actions[1..] {
            let mut pending: Vec<usize> = (0..end).filter(|&i| !visible[i]).collect();
            if pending.is_empty() {
                break;
            }
            let allowed = pending.len().min(budget - questions);
            if allowed < pending.len() {
                end = pending[allowed];
                pending.truncate(allowed);
            }
            if pending.is_empty() {
                break;
            }
            let perms: Vec<Permission> =
                pending.iter().map(|&i| Permission::new(*action, node_of(&window[i]))).collect();
            let answers = access.display(who, perms).await?;
            questions += pending.len();
            for (i, seen) in pending.into_iter().zip(answers) {
                visible[i] = seen;
            }
        }
        for (k, (child, seen)) in window[..end].iter().zip(&visible).enumerate() {
            if *seen {
                items.push(OptionItem {
                    id: child.id.clone(),
                    label: child.label.clone(),
                    has_children: child.has_children,
                });
                last_visible = Some(idx + k);
            }
        }
        idx += end;
        if end < take {
            break;
        }
    }
    // The cursor holds the last visible id and the count of the hidden children after it. A page with no visible
    // item keeps the position of the request and adds the children that it examined.
    let next = if idx >= candidates.len() || idx == 0 {
        None
    } else if let Some(v) = last_visible {
        let id = candidates.get(v).map(|c| c.id.as_str());
        Some(make_cursor(type_name, &parsed.parent_text, id, idx - (v + 1)))
    } else {
        Some(make_cursor(type_name, &parsed.parent_text, parsed.after_id.as_deref(), parsed.after_skip + idx))
    };
    Ok(OptionsWalk { items, next, questions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_has_the_form_of_11_26() {
        let c = make_cursor("dashboard", "folder:transit", Some("score-daily"), 3);
        assert_eq!(base64url_decode(&c).unwrap(), b"dashboard\nfolder:transit\nscore-daily\n3");
        let none = make_cursor("dashboard", "folder:transit", None, 7);
        assert_eq!(base64url_decode(&none).unwrap(), b"dashboard\nfolder:transit\n\n7");
    }

    #[test]
    fn a_label_is_cut_to_60_characters() {
        assert_eq!(cut_label(&"é".repeat(70)).chars().count(), 60);
        assert_eq!(cut_label("Score"), "Score");
    }
}
