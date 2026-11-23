//! The walk of the eleven transforms of milestone F (4.25a). The two older kinds are in `walk.rs`.
//!
//! Each kind has a closed set of necessary members. A member of another kind is `unknown_member`.

use super::model::*;
use super::pattern::PatternForm;
use super::read::{Ctx, Field, Obj, TextRule};
use super::walk::{pattern, unique};
use crate::error::ErrorCode;
use crate::limits::{
    MAX_CONVERSIONS, MAX_LIMIT_COUNT, MAX_PARTITION_FIELDS, MAX_SORT_KEYS, MAX_TRANSFORM_ITEMS,
    MAX_TRANSFORM_NAME_CHARS, MAX_TRANSFORM_REDUCERS, MAX_VALUE_BYTES,
};

/// A field name of 4.25a: a current name of 1 to 1024 bytes.
const NAME: TextRule = TextRule::bytes(1, MAX_VALUE_BYTES);
/// A new name or an alias: 1 to 256 characters.
const NEW_NAME: TextRule = TextRule::chars(1, MAX_TRANSFORM_NAME_CHARS);
/// The replacement text of `rename_by_regex`: 0 to 256 characters.
const REPLACEMENT: TextRule = TextRule::chars(0, MAX_TRANSFORM_NAME_CHARS);

/// Reads the members of a transform after `kind`, for one of the eleven kinds. The member `kind` is read.
pub(super) fn more(cx: &mut Ctx, mut obj: Obj<'_>, tag: TransformTag) -> Option<Transform> {
    // The result and the list of the members that the kind permits, for the error `unknown_member`.
    let (out, permitted): (Option<Transform>, &[&str]) = match tag {
        TransformTag::Organize => {
            let hide = obj.get(cx, "hide").and_then(|f| names(cx, &f, 0, MAX_TRANSFORM_ITEMS));
            let order = obj.get(cx, "order").and_then(|f| names(cx, &f, 0, MAX_TRANSFORM_ITEMS));
            let rename = obj.get(cx, "rename").and_then(|f| renames(cx, &f));
            let out = (|| Some(Transform::Organize { hide: hide?, order: order?, rename: rename? }))();
            (out, &["kind", "hide", "order", "rename"])
        }
        TransformTag::CalculateField => return calculate_field(cx, obj),
        TransformTag::SortBy => {
            let sort = obj.get(cx, "sort").and_then(|f| sort_keys(cx, &f));
            (sort.map(|sort| Transform::SortBy { sort }), &["kind", "sort"])
        }
        TransformTag::Limit => {
            let count = obj.int(cx, "count", 0, MAX_LIMIT_COUNT);
            let end = obj.en::<LimitEnd>(cx, "end");
            let out = (|| Some(Transform::Limit { count: u32::try_from(count?).ok()?, end: end? }))();
            (out, &["kind", "count", "end"])
        }
        TransformTag::Merge => (Some(Transform::Merge), &["kind"]),
        TransformTag::Reduce => {
            let mode = obj.en::<ReduceMode>(cx, "mode");
            let reducers = obj.get(cx, "reducers").and_then(|f| reducers(cx, &f));
            let out = (|| Some(Transform::Reduce { mode: mode?, reducers: reducers? }))();
            (out, &["kind", "mode", "reducers"])
        }
        TransformTag::PartitionByValues => {
            let fields = obj.get(cx, "fields").and_then(|f| names(cx, &f, 1, MAX_PARTITION_FIELDS));
            let keep_fields = obj.boolean(cx, "keep_fields");
            let out = (|| Some(Transform::PartitionByValues { fields: fields?, keep_fields: keep_fields? }))();
            (out, &["kind", "fields", "keep_fields"])
        }
        TransformTag::JoinByField => {
            let field = obj.get(cx, "field").and_then(|f| f.nullable(|f| f.text(cx, NAME)));
            let mode = obj.en::<JoinMode>(cx, "mode");
            let out = (|| Some(Transform::JoinByField { field: field?, mode: mode? }))();
            (out, &["kind", "field", "mode"])
        }
        TransformTag::FilterFieldsByName => {
            let include = obj.get(cx, "include").and_then(|f| name_filter(cx, &f, "include"));
            let exclude = obj.get(cx, "exclude").and_then(|f| name_filter(cx, &f, "exclude"));
            let out = (|| Some(Transform::FilterFieldsByName { include: include?, exclude: exclude? }))();
            (out, &["kind", "include", "exclude"])
        }
        TransformTag::RenameByRegex => {
            let regex = obj.get(cx, "regex").and_then(|f| pattern(cx, &f, PatternForm::Grafana));
            let rename_pattern = obj.text(cx, "rename_pattern", REPLACEMENT);
            let out = (|| Some(Transform::RenameByRegex { regex: regex?, rename_pattern: rename_pattern? }))();
            (out, &["kind", "regex", "rename_pattern"])
        }
        TransformTag::ConvertFieldType => {
            let conversions = obj.get(cx, "conversions").and_then(|f| conversions(cx, &f));
            (conversions.map(|conversions| Transform::ConvertFieldType { conversions }), &["kind", "conversions"])
        }
        // The two older kinds have their own walk.
        TransformTag::FilterByValue | TransformTag::GroupingToMatrix => (None, &["kind"]),
    };
    // Each kind takes its members above: the other members are unknown.
    obj.finish(cx, permitted);
    out
}

/// An array of names: `min` to `max` items of 1 to 1024 bytes, no two equal.
fn names(cx: &mut Ctx, f: &Field<'_>, min: usize, max: usize) -> Option<Vec<String>> {
    let items = f.items(cx, min, max, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<String>> = items.iter().map(|i| i.text(cx, NAME)).collect();
    unique(cx, &items, &parsed);
    parsed.into_iter().collect()
}

fn renames(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Rename>> {
    let items = f.items(cx, 0, MAX_TRANSFORM_ITEMS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<Rename>> = items
        .iter()
        .map(|i| {
            let mut o = i.object(cx, "rename")?;
            let field = o.text(cx, "field", NAME);
            let name = o.text(cx, "name", NEW_NAME);
            o.finish(cx, &["field", "name"]);
            Some(Rename { field: field?, name: name? })
        })
        .collect();
    let keys: Vec<Option<&String>> = parsed.iter().map(|p| p.as_ref().map(|r| &r.field)).collect();
    unique(cx, &items, &keys);
    parsed.into_iter().collect()
}

fn sort_keys(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<SortKey>> {
    let items = f.items(cx, 1, MAX_SORT_KEYS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<SortKey>> = items
        .iter()
        .map(|i| {
            let mut o = i.object(cx, "sort key")?;
            let field = o.text(cx, "field", NAME);
            let desc = o.boolean(cx, "desc");
            o.finish(cx, &["field", "desc"]);
            Some(SortKey { field: field?, desc: desc? })
        })
        .collect();
    unique(cx, &items, &parsed);
    parsed.into_iter().collect()
}

fn reducers(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<ReduceCalc>> {
    let items = f.items(cx, 1, MAX_TRANSFORM_REDUCERS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<ReduceCalc>> = items.iter().map(|i| i.en::<ReduceCalc>(cx)).collect();
    unique(cx, &items, &parsed);
    parsed.into_iter().collect()
}

/// The member `include` or `exclude` of `filter_fields_by_name`.
fn name_filter(cx: &mut Ctx, f: &Field<'_>, label: &'static str) -> Option<NameFilter> {
    let mut obj = f.object(cx, label)?;
    let list = obj.get(cx, "names").and_then(|f| names(cx, &f, 0, MAX_TRANSFORM_ITEMS));
    let pattern = obj.get(cx, "pattern").and_then(|f| f.nullable(|f| pattern(cx, f, PatternForm::Grafana)));
    obj.finish(cx, &["names", "pattern"]);
    Some(NameFilter { names: list?, pattern: pattern? })
}

fn conversions(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Conversion>> {
    let items = f.items(cx, 1, MAX_CONVERSIONS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<Conversion>> = items
        .iter()
        .map(|i| {
            let mut o = i.object(cx, "conversion")?;
            let field = o.text(cx, "field", NAME);
            let to = o.en::<ConvertTo>(cx, "to");
            o.finish(cx, &["field", "to"]);
            Some(Conversion { field: field?, to: to? })
        })
        .collect();
    let keys: Vec<Option<&String>> = parsed.iter().map(|p| p.as_ref().map(|c| &c.field)).collect();
    unique(cx, &items, &keys);
    parsed.into_iter().collect()
}

// ------------------------------------------------------------- calculate_field

/// Kind `calculate_field`: the members `mode`, `alias` and `replace_fields`, then the object of the mode.
fn calculate_field(cx: &mut Ctx, mut obj: Obj<'_>) -> Option<Transform> {
    let mode = obj.en::<CalcMode>(cx, "mode");
    let alias = obj.get(cx, "alias").and_then(|f| f.nullable(|f| f.text(cx, NEW_NAME)));
    let replace_fields = obj.boolean(cx, "replace_fields");
    let Some(mode) = mode else {
        // The mode is not known: the object of the mode cannot be checked, and none is unknown.
        obj.skip_all();
        return None;
    };
    // The object of the mode is necessary. The objects of the other modes are `unknown_member`.
    let (key, calc) = match mode {
        CalcMode::Binary => ("binary", obj.get(cx, "binary").and_then(|f| binary(cx, &f))),
        CalcMode::ReduceRow => ("reduce", obj.get(cx, "reduce").and_then(|f| reduce_row(cx, &f))),
        CalcMode::Unary => ("unary", obj.get(cx, "unary").and_then(|f| unary(cx, &f))),
        CalcMode::Index => ("index", obj.get(cx, "index").and_then(|f| index(cx, &f))),
    };
    obj.finish(cx, &["kind", "mode", "alias", "replace_fields", key]);
    Some(Transform::CalculateField { alias: alias?, replace_fields: replace_fields?, calc: calc? })
}

fn binary(cx: &mut Ctx, f: &Field<'_>) -> Option<Calc> {
    let mut obj = f.object(cx, "binary")?;
    let left = obj.get(cx, "left").and_then(|f| operand(cx, &f));
    let right = obj.get(cx, "right").and_then(|f| operand(cx, &f));
    let operator = obj.en::<BinaryOperator>(cx, "operator");
    obj.finish(cx, &["left", "right", "operator"]);
    let (left, right, operator) = (left?, right?, operator?);
    if matches!((&left, &right), (Operand::Number(_), Operand::Number(_))) {
        f.fail(cx, ErrorCode::NotPermitted, "must have at least one operand that is a field (4.25a)");
        return None;
    }
    Some(Calc::Binary { left, right, operator })
}

fn operand(cx: &mut Ctx, f: &Field<'_>) -> Option<Operand> {
    let mut obj = f.object(cx, "operand")?;
    let Some(kind) = obj.en::<OperandKind>(cx, "kind") else {
        obj.skip_all();
        return None;
    };
    match kind {
        OperandKind::Field => {
            let field = obj.text(cx, "field", NAME);
            obj.finish(cx, &["kind", "field"]);
            field.map(Operand::Field)
        }
        OperandKind::Number => {
            let value = obj.num(cx, "value");
            obj.finish(cx, &["kind", "value"]);
            value.map(Operand::Number)
        }
    }
}

fn reduce_row(cx: &mut Ctx, f: &Field<'_>) -> Option<Calc> {
    let mut obj = f.object(cx, "reduce")?;
    let reducer = obj.en::<ReduceCalc>(cx, "reducer");
    let include = obj.get(cx, "include").and_then(|f| names(cx, &f, 0, MAX_TRANSFORM_ITEMS));
    obj.finish(cx, &["reducer", "include"]);
    Some(Calc::ReduceRow { reducer: reducer?, include: include? })
}

fn unary(cx: &mut Ctx, f: &Field<'_>) -> Option<Calc> {
    let mut obj = f.object(cx, "unary")?;
    let operator = obj.en::<UnaryOperator>(cx, "operator");
    let field = obj.text(cx, "field", NAME);
    obj.finish(cx, &["operator", "field"]);
    Some(Calc::Unary { operator: operator?, field: field? })
}

fn index(cx: &mut Ctx, f: &Field<'_>) -> Option<Calc> {
    let mut obj = f.object(cx, "index")?;
    let as_percentile = obj.boolean(cx, "as_percentile");
    obj.finish(cx, &["as_percentile"]);
    Some(Calc::Index { as_percentile: as_percentile? })
}
