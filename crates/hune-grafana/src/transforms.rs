//! The transformations of a panel (4.25), with the transformation table of 15.6.

use hune_core::document::{PatternForm, check_pattern};
use hune_core::limits::{MAX_FILTERS, MAX_TRANSFORMS};

use crate::dashboard::Form;
use crate::item::{Ctx, Fallback, is_trivial};
use crate::out::Out;
use crate::transforms_more as more;
use crate::util::{as_flag, as_num};
use crate::walk::{Node, Walk};

/// The result of the mapping of the transformations of one panel.
pub(crate) struct Transforms {
    /// The items of the member `transforms`.
    pub items: Vec<Out>,
    /// The format that a transformation sets for the query (`prepareTimeSeries`, `labelsToFields`).
    pub format: Option<&'static str>,
}

fn removed() -> Fallback {
    Fallback::other("transformation removed")
}

fn field_text(n: Option<&Node>) -> Option<String> {
    n.and_then(Node::str).filter(|t| !t.is_empty() && t.len() <= 1024).map(str::to_owned)
}

fn filter_by_value(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let Some(options) = options.filter(|o| o.v.is_object()) else {
        ctx.unsupported(&at.ptr, Some(at.v), "filterByValue needs options", removed());
        return None;
    };
    let mut w = Walk::new(&options)?;
    let kind = w.take("type");
    let matching = w.take("match");
    let filters = w.take("filters");
    w.finish(ctx);
    let mode = match kind.as_ref().map(|k| (k, k.str())) {
        None => "include",
        Some((_, Some(m @ ("include" | "exclude")))) => m,
        Some((k, _)) => {
            ctx.unsupported(&k.ptr, Some(k.v), "type is not include or exclude", removed());
            return None;
        }
    };
    let match_text = match matching.as_ref().map(|k| (k, k.str())) {
        None => "any",
        Some((_, Some(m @ ("any" | "all")))) => m,
        Some((k, _)) => {
            ctx.unsupported(&k.ptr, Some(k.v), "match is not any or all", removed());
            return None;
        }
    };
    let Some(filters) = filters.filter(|f| f.v.is_array()) else {
        ctx.unsupported(&options.ptr, Some(options.v), "filterByValue needs an array filters", removed());
        return None;
    };
    let list = filters.items();
    if list.is_empty() || list.len() > MAX_FILTERS {
        ctx.unsupported(&filters.ptr, Some(filters.v), "filterByValue needs 1 to 32 filters", removed());
        return None;
    }
    let mut out = Vec::new();
    for f in &list {
        let mut fw = Walk::new(f)?;
        let name = fw.take("fieldName");
        let config = fw.take("config");
        fw.finish(ctx);
        let Some(field) = field_text(name.as_ref()) else {
            ctx.unsupported(&f.ptr, Some(f.v), "a filter needs a fieldName of 1 to 1024 bytes", removed());
            return None;
        };
        let matcher = config.as_ref().and_then(|c| matcher(ctx, c))?;
        out.push(Out::obj([("field", Out::Str(field)), ("matcher", matcher)]));
    }
    ctx.ok();
    Some(Out::obj([
        ("kind", Out::s("filter_by_value")),
        ("mode", Out::s(mode)),
        ("match", Out::s(match_text)),
        ("filters", Out::Arr(out)),
    ]))
}

fn matcher(ctx: &mut Ctx, config: &Node) -> Option<Out> {
    let Some(mut w) = Walk::new(config) else {
        ctx.unsupported(&config.ptr, Some(config.v), "config must be an object", removed());
        return None;
    };
    let id = w.take("id");
    let options = w.take("options");
    w.finish(ctx);
    let id_node = id?;
    let id_text = id_node.str().unwrap_or("");
    let mut ow = options.as_ref().and_then(Walk::new);
    let mut value = ow.as_mut().and_then(|w| w.take("value"));
    let from = ow.as_mut().and_then(|w| w.take("from"));
    let to = ow.as_mut().and_then(|w| w.take("to"));
    let number = |ctx: &mut Ctx, n: Option<&Node>| -> Option<f64> {
        match n.and_then(|n| as_num(n.v)) {
            Some(v) => Some(v),
            None => {
                let at = n.unwrap_or(&id_node);
                ctx.unsupported(&at.ptr, Some(at.v), "this matcher needs a number", removed());
                None
            }
        }
    };
    let out = match id_text {
        "equal" | "notEqual" => {
            let kind = if id_text == "equal" { "equal" } else { "not_equal" };
            let v = match value.take() {
                Some(n) if n.v.is_number() => Out::Num(as_num(n.v)?),
                Some(n) if n.v.is_string() => Out::s(n.str().unwrap_or_default()),
                other => {
                    let at = other.unwrap_or_else(|| id_node.clone());
                    ctx.unsupported(&at.ptr, Some(at.v), "this matcher needs a number or a text", removed());
                    return None;
                }
            };
            Out::obj([("kind", Out::s(kind)), ("value", v)])
        }
        "greater" | "greaterOrEqual" | "lower" | "lowerOrEqual" => {
            let kind = match id_text {
                "greater" => "greater",
                "greaterOrEqual" => "greater_or_equal",
                "lower" => "lower",
                _ => "lower_or_equal",
            };
            let v = number(ctx, value.as_ref())?;
            Out::obj([("kind", Out::s(kind)), ("value", Out::Num(v))])
        }
        "between" => {
            let f = number(ctx, from.as_ref())?;
            let t = number(ctx, to.as_ref())?;
            if f > t {
                ctx.unsupported(&config.ptr, Some(config.v), "from must be at most to", removed());
                return None;
            }
            Out::obj([("kind", Out::s("range")), ("from", Out::Num(f)), ("to", Out::Num(t))])
        }
        "regex" => {
            let Some(p) = value.as_ref().and_then(Node::str) else {
                ctx.unsupported(&config.ptr, Some(config.v), "a regex matcher needs a text value", removed());
                return None;
            };
            if let Err(e) = check_pattern(p, PatternForm::Plain) {
                ctx.unsupported(
                    &config.ptr,
                    Some(config.v),
                    &format!("the pattern is refused by 4.24: {e:?}"),
                    removed(),
                );
                return None;
            }
            Out::obj([("kind", Out::s("regex")), ("pattern", Out::s(p))])
        }
        "isNull" => Out::obj([("kind", Out::s("is_null"))]),
        "isNotNull" => Out::obj([("kind", Out::s("is_not_null"))]),
        _ => {
            ctx.unsupported(&id_node.ptr, Some(id_node.v), "this matcher has no equal in format 1", removed());
            return None;
        }
    };
    if let Some(ow) = ow {
        ow.finish(ctx);
    }
    ctx.ok();
    Some(out)
}

fn grouping_to_matrix(ctx: &mut Ctx, options: Option<Node>, at: &Node) -> Option<Out> {
    let Some(options) = options.filter(|o| o.v.is_object()) else {
        ctx.unsupported(&at.ptr, Some(at.v), "groupingToMatrix needs options", removed());
        return None;
    };
    let mut w = Walk::new(&options)?;
    let column = w.take("columnField");
    let row = w.take("rowField");
    let value = w.take("valueField");
    let empty = w.take("emptyValue");
    w.finish(ctx);
    let (Some(c), Some(r), Some(v)) =
        (field_text(column.as_ref()), field_text(row.as_ref()), field_text(value.as_ref()))
    else {
        ctx.unsupported(
            &options.ptr,
            Some(options.v),
            "columnField, rowField and valueField need 1 to 1024 bytes",
            removed(),
        );
        return None;
    };
    let empty_out = match empty.as_ref().map(|e| (e, e.str())) {
        None => "null",
        Some((_, Some(e @ ("null" | "true" | "false" | "empty")))) => e,
        Some((e, _)) => {
            ctx.unsupported(&e.ptr, Some(e.v), "this emptyValue has no equal in format 1", removed());
            return None;
        }
    };
    ctx.ok();
    Some(Out::obj([
        ("kind", Out::s("grouping_to_matrix")),
        ("column_field", Out::Str(c)),
        ("row_field", Out::Str(r)),
        ("value_field", Out::Str(v)),
        ("empty_value", Out::s(empty_out)),
    ]))
}

/// Maps the member `transformations` of a panel. `has_query` is false for a panel that reuses a frame.
pub(crate) fn transforms(ctx: &mut Ctx, form: Form, node: Option<Node>, has_query: bool) -> Transforms {
    let mut result = Transforms { items: Vec::new(), format: None };
    let Some(node) = node.filter(|n| !n.is_null()) else { return result };
    if node.v.as_array().is_none() {
        ctx.unsupported(&node.ptr, Some(node.v), "transformations must be an array", Fallback::MemberRemoved);
        return result;
    }
    for item in node.items() {
        let Some(mut w) = Walk::new(&item) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a transformation must be an object", removed());
            continue;
        };
        // v2: `kind` is the Grafana id and `spec` holds the members.
        let (kind, mut sw) = if form.is_v2() {
            let kind = w.take("kind");
            let spec = w.take("spec");
            w.finish(ctx);
            let Some(sw) = spec.as_ref().and_then(Walk::new) else {
                ctx.unsupported(&item.ptr, Some(item.v), "a transformation needs a spec object", removed());
                continue;
            };
            (kind, sw)
        } else {
            (None, w)
        };
        let id = sw.take("id").or(kind);
        let options = sw.take("options");
        let disabled = sw.take("disabled");
        let filter = sw.take("filter");
        let topic = sw.take("topic");
        sw.finish(ctx);
        if let Some(t) = topic {
            ctx.unknown_member(&t.ptr, t.v);
        }
        if let Some(f) = &filter {
            if !is_trivial(f.v) {
                ctx.unsupported(
                    &f.ptr,
                    Some(f.v),
                    "the filter by refId has no equal in format 1",
                    Fallback::MemberRemoved,
                );
            } else {
                ctx.meta();
            }
        }
        if let Some(d) = disabled.as_ref() {
            if as_flag(d.v) == Some(true) {
                ctx.dropped(&d.ptr, Some(d.v), "the transformation is disabled: it is removed");
                continue;
            }
            ctx.meta();
        }
        let Some(id) = id.as_ref().and_then(Node::str).map(str::to_owned) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a transformation needs an id", removed());
            continue;
        };
        match id.as_str() {
            "filterByValue" => {
                if let Some(out) = filter_by_value(ctx, options, &item) {
                    result.items.push(out);
                }
            }
            "groupingToMatrix" => {
                if let Some(out) = grouping_to_matrix(ctx, options, &item) {
                    result.items.push(out);
                }
            }
            // The eleven transformations of milestone F (15.6, 4.25a).
            "organize" | "calculateField" | "sortBy" | "limit" | "merge" | "reduce" | "partitionByValues"
            | "joinByField" | "seriesToColumns" | "filterFieldsByName" | "renameByRegex" | "convertFieldType" => {
                let out = match id.as_str() {
                    "organize" => more::organize(ctx, options, &item),
                    "calculateField" => more::calculate_field(ctx, options, &item),
                    "sortBy" => more::sort_by(ctx, options, &item),
                    "limit" => more::limit(ctx, options, &item),
                    "merge" => more::merge(ctx, options, &item),
                    "reduce" => more::reduce(ctx, options, &item),
                    "partitionByValues" => more::partition_by_values(ctx, options, &item),
                    "joinByField" | "seriesToColumns" => more::join_by_field(ctx, options, &item),
                    "filterFieldsByName" => more::filter_fields_by_name(ctx, options, &item),
                    "renameByRegex" => more::rename_by_regex(ctx, options, &item),
                    _ => more::convert_field_type(ctx, options, &item),
                };
                result.items.extend(out);
            }
            "prepareTimeSeries" | "labelsToFields" => {
                if !has_query {
                    ctx.unsupported(
                        &item.ptr,
                        Some(item.v),
                        "the panel reuses a frame: it has no query format to set",
                        removed(),
                    );
                    continue;
                }
                if id == "prepareTimeSeries" {
                    let format =
                        options.as_ref().and_then(|o| o.get("format")).and_then(|f| f.str().map(str::to_owned));
                    // `many` is the older name of `multi`: Grafana 12.4.2 treats both the same (15.6).
                    if matches!(format.as_deref(), Some("multi" | "many")) {
                        ctx.changed(
                            &item.ptr,
                            Some(item.v),
                            "the multi-frame form is the long-to-wide of format time_series",
                        );
                        result.format = Some("time_series");
                    } else {
                        ctx.unsupported(
                            &item.ptr,
                            Some(item.v),
                            "only the multi-frame form has an equal in format 1",
                            removed(),
                        );
                    }
                } else {
                    ctx.changed(
                        &item.ptr,
                        Some(item.v),
                        "after long-to-wide, the result is the table form: format table",
                    );
                    result.format = Some("table");
                }
            }
            _ => ctx.unsupported(&item.ptr, Some(item.v), "this transformation has no equal in format 1", removed()),
        }
    }
    if result.items.len() > MAX_TRANSFORMS {
        ctx.unsupported(
            &node.ptr,
            None,
            "more transformations than MAX_TRANSFORMS",
            Fallback::other(&format!("the first {MAX_TRANSFORMS} items stay")),
        );
        result.items.truncate(MAX_TRANSFORMS);
    }
    result
}
