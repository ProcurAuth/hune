// SPDX-License-Identifier: MIT OR Apache-2.0
//! The mapping of ClickHouse type texts to kinds (7.16, first table).

use hune_core::frame::{Kind, ValueForm, kind_of_type, map_type, strip_wrappers};

fn kind(t: &str) -> &'static str {
    kind_of_type(t).as_str()
}

#[test]
fn small_numbers_are_numbers() {
    for t in ["UInt8", "UInt16", "UInt32", "Int8", "Int16", "Int32", "Float32", "Float64", "BFloat16"] {
        assert_eq!(kind(t), "number", "{t}");
        assert_eq!(map_type(t), ValueForm::Number, "{t}");
    }
}

#[test]
fn large_integers_are_number_text() {
    for t in ["UInt64", "Int64", "UInt128", "Int128", "UInt256", "Int256"] {
        assert_eq!(kind(t), "number_text", "{t}");
        assert_eq!(map_type(t), ValueForm::Integer, "{t}");
    }
}

#[test]
fn decimals_are_number_text() {
    for t in ["Decimal(38, 9)", "Decimal(10,2)", "Decimal32(4)", "Decimal64(8)", "Decimal128(18)", "Decimal256(40)"] {
        assert_eq!(kind(t), "number_text", "{t}");
        assert_eq!(map_type(t), ValueForm::Decimal, "{t}");
    }
}

#[test]
fn bool_date_and_time_types() {
    assert_eq!(kind("Bool"), "bool");
    assert_eq!(map_type("Date"), ValueForm::Date);
    assert_eq!(map_type("Date32"), ValueForm::Date);
    assert_eq!(kind("Date"), "time");
    for t in ["DateTime", "DateTime('UTC')", "DateTime64(3)", "DateTime64(9, 'Europe/Paris')"] {
        assert_eq!(kind(t), "time", "{t}");
        assert_eq!(map_type(t), ValueForm::DateTime, "{t}");
    }
}

#[test]
fn text_types() {
    for t in
        ["String", "FixedString(16)", "UUID", "IPv4", "IPv6", "Enum8('a' = 1, 'b' = 2)", "Enum16('x(' = 1, ')y' = 2)"]
    {
        assert_eq!(kind(t), "text", "{t}");
    }
}

#[test]
fn nothing_is_null() {
    assert_eq!(kind("Nothing"), "null");
    assert_eq!(kind("Nullable(Nothing)"), "null");
    assert_eq!(map_type("Nothing"), ValueForm::Null);
}

#[test]
fn other_types_are_text() {
    for t in [
        "Array(UInt8)",
        "Tuple(a UInt8, b String)",
        "Map(String, UInt64)",
        "Nested(a UInt8)",
        "JSON",
        "Object('json')",
        "Variant(String, UInt64)",
        "Dynamic",
        "Point",
        "Ring",
        "Polygon",
        "MultiPolygon",
        "IntervalDay",
        "Time",
        "Time64(3)",
        "AggregateFunction(sum, UInt64)",
        "SomethingNew(1)",
    ] {
        assert_eq!(kind(t), "text", "{t}");
    }
}

#[test]
fn wrappers_are_removed_at_any_depth_and_order() {
    assert_eq!(kind("Nullable(UInt32)"), "number");
    assert_eq!(kind("LowCardinality(String)"), "text");
    assert_eq!(kind("LowCardinality(Nullable(String))"), "text");
    assert_eq!(kind("Nullable(LowCardinality(UInt64))"), "number_text");
    assert_eq!(kind("LowCardinality(Nullable(Nullable(Decimal(10, 2))))"), "number_text");
    assert_eq!(kind("Nullable(DateTime64(3, 'UTC'))"), "time");
    assert_eq!(strip_wrappers("LowCardinality(Nullable(String))"), "String");
    assert_eq!(strip_wrappers(" Nullable( Int8 ) "), "Int8");
}

#[test]
fn a_wrapper_around_a_container_keeps_the_container() {
    // `Nullable` cannot wrap an `Array`, but the rule is still the same: only the outer text counts.
    assert_eq!(kind("Array(Nullable(UInt8))"), "text");
    assert_eq!(strip_wrappers("Array(Nullable(UInt8))"), "Array(Nullable(UInt8))");
    assert_eq!(kind("Map(String, Nullable(UInt8))"), "text");
}

#[test]
fn simple_aggregate_function_takes_the_kind_of_its_type() {
    assert_eq!(kind("SimpleAggregateFunction(sum, UInt64)"), "number_text");
    assert_eq!(kind("SimpleAggregateFunction(max, Float64)"), "number");
    assert_eq!(kind("SimpleAggregateFunction(anyLast, Nullable(String))"), "text");
    assert_eq!(kind("SimpleAggregateFunction(max, DateTime)"), "time");
    assert_eq!(kind("LowCardinality(SimpleAggregateFunction(min, Int32))"), "number");
    assert_eq!(kind("SimpleAggregateFunction(groupArrayArray, Array(UInt8))"), "text");
    assert_eq!(kind("Nullable(SimpleAggregateFunction(sum, Decimal(18, 4)))"), "number_text");
}

#[test]
fn unbalanced_text_is_not_unwrapped() {
    assert_eq!(strip_wrappers("Nullable(Int8"), "Nullable(Int8");
    assert_eq!(kind("Nullable(Int8"), "text");
    assert_eq!(kind(""), "text");
    assert_eq!(Kind::Null.as_str(), "null");
}
