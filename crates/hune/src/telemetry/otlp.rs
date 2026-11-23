//! The OTLP JSON encoding of `ExportTraceServiceRequest` (14.9).
//!
//! The writer is by hand so that the member order is fixed. The text of each string goes through
//! `serde_json`, which escapes it.

use std::fmt::Write as _;

use super::span::{AttrValue, SpanData, hex};

/// Encodes the batches of one service. The resource part is built one time.
#[derive(Debug, Clone)]
pub(crate) struct Encoder {
    prefix: String,
}

const SUFFIX: &str = "]}]}]}";

fn push_str(out: &mut String, text: &str) {
    match serde_json::to_string(text) {
        Ok(s) => out.push_str(&s),
        Err(_) => out.push_str("\"\""),
    }
}

fn push_attr(out: &mut String, key: &str, value: &AttrValue) {
    out.push_str("{\"key\":");
    push_str(out, key);
    out.push_str(",\"value\":{");
    match value {
        AttrValue::Str(s) => {
            out.push_str("\"stringValue\":");
            push_str(out, s);
        }
        AttrValue::Int(i) => {
            let _ = write!(out, "\"intValue\":\"{i}\"");
        }
        AttrValue::Bool(b) => {
            let _ = write!(out, "\"boolValue\":{b}");
        }
    }
    out.push_str("}}");
}

impl Encoder {
    /// Makes an encoder with the three resource attributes of 14.9.
    pub(crate) fn new(service_name: &str, service_version: &str, instance_id: &str) -> Encoder {
        let mut prefix = String::from("{\"resourceSpans\":[{\"resource\":{\"attributes\":[");
        push_attr(&mut prefix, "service.name", &AttrValue::Str(service_name.to_owned()));
        prefix.push(',');
        push_attr(&mut prefix, "service.version", &AttrValue::Str(service_version.to_owned()));
        prefix.push(',');
        push_attr(&mut prefix, "service.instance.id", &AttrValue::Str(instance_id.to_owned()));
        prefix.push_str("]},\"scopeSpans\":[{\"scope\":{\"name\":\"hune\"},\"spans\":[");
        Encoder { prefix }
    }

    /// Encodes a batch of spans as the body of one `POST`.
    pub(crate) fn encode(&self, spans: &[SpanData]) -> Vec<u8> {
        let mut out = String::with_capacity(self.prefix.len() + spans.len() * 320 + SUFFIX.len());
        out.push_str(&self.prefix);
        for (i, s) in spans.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            encode_span(&mut out, s);
        }
        out.push_str(SUFFIX);
        out.into_bytes()
    }
}

fn encode_span(out: &mut String, s: &SpanData) {
    let _ = write!(out, "{{\"traceId\":\"{}\",\"spanId\":\"{}\"", hex(&s.trace_id), hex(&s.span_id));
    if let Some(parent) = &s.parent {
        let _ = write!(out, ",\"parentSpanId\":\"{}\"", hex(parent));
    }
    out.push_str(",\"name\":");
    push_str(out, &s.name);
    let _ = write!(
        out,
        ",\"kind\":{},\"startTimeUnixNano\":\"{}\",\"endTimeUnixNano\":\"{}\",\"attributes\":[",
        s.kind.otlp_number(),
        s.start_unix_nanos,
        s.end_unix_nanos
    );
    for (i, a) in s.attributes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_attr(out, a.key, &a.value);
    }
    let _ = write!(out, "],\"status\":{{\"code\":{}}}}}", s.status.otlp_number());
}
