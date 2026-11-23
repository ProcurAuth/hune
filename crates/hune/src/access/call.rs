//! The context of one call of the PEP library, in a thread-local value (11.12).
//!
//! The library gives the transport no context of the request. Before each call, the task puts the
//! request id and the trace context in this value of its blocking thread. The transport reads it for
//! `X-Request-ID` and `traceparent`, and it records each question and answer that it sees. The task
//! clears the value after the call and reads the record: the log lines `access.decision` and the
//! action of a refusal come from it.

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::Value;

/// The fields of the call that the transport reads.
#[derive(Debug, Clone, Default)]
pub struct CallInfo {
    /// The request id (14.6). The transport sends it as `X-Request-ID`.
    pub request_id: String,
    /// The value of `traceparent` (14.10), when telemetry is on.
    pub traceparent: Option<String>,
}

/// One question and its answer, as the transport saw them on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tapped {
    /// The action name.
    pub action: String,
    /// The text form of the chain and the node (11.16): `service:a/folder:b/dashboard:c`.
    pub resource: String,
    /// `node` or `subtree`.
    pub match_mode: &'static str,
    /// The decision.
    pub decision: bool,
    /// `context.reason` of the answer, for the log only (11.28).
    pub reason: Option<String>,
    /// `context.limits` of the answer.
    pub ceilings: BTreeMap<String, u64>,
}

/// What a call did on the wire.
#[derive(Debug, Clone, Default)]
pub struct CallReport {
    /// The number of requests that the transport sent.
    pub requests: u32,
    /// The questions that the PDP answered, in order.
    pub answers: Vec<Tapped>,
    /// The time that the requests took, in milliseconds.
    pub duration_ms: u64,
}

#[derive(Debug, Default)]
struct Slot {
    info: CallInfo,
    report: CallReport,
}

thread_local! {
    static CURRENT: RefCell<Option<Slot>> = const { RefCell::new(None) };
}

/// Runs `f` with `info` as the context of this thread. It gives the result and the report.
pub fn with_call<T>(info: CallInfo, f: impl FnOnce() -> T) -> (T, CallReport) {
    CURRENT.with(|c| *c.borrow_mut() = Some(Slot { info, report: CallReport::default() }));
    let result = f();
    let slot = CURRENT.with(|c| c.borrow_mut().take()).unwrap_or_default();
    (result, slot.report)
}

/// The context of this thread, for the headers of a request to the PDP.
pub fn current_info() -> CallInfo {
    CURRENT.with(|c| c.borrow().as_ref().map(|s| s.info.clone()).unwrap_or_default())
}

/// Records one exchange with the PDP (a request body, and the body of a valid answer).
pub fn record_exchange(request: &[u8], response: Option<&[u8]>, elapsed_ms: u64) {
    CURRENT.with(|c| {
        if let Some(slot) = c.borrow_mut().as_mut() {
            slot.report.requests += 1;
            slot.report.duration_ms += elapsed_ms;
            if let Some(response) = response {
                slot.report.answers.extend(tap(request, response));
            }
        }
    });
}

fn text(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

/// The text form of the chain of one resource object (11.16).
fn chain_text(resource: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(list) = resource.pointer("/properties/ancestors").and_then(Value::as_array) {
        for a in list {
            parts.push(format!("{}:{}", text(a.get("type")).unwrap_or("?"), text(a.get("id")).unwrap_or("?")));
        }
    }
    parts.push(format!("{}:{}", text(resource.get("type")).unwrap_or("?"), text(resource.get("id")).unwrap_or("?")));
    parts.join("/")
}

fn answer_of(item: &Value) -> Option<(bool, Option<String>, BTreeMap<String, u64>)> {
    let decision = item.get("decision")?.as_bool()?;
    let reason = text(item.pointer("/context/reason")).map(str::to_owned);
    let mut limits = BTreeMap::new();
    if decision && let Some(map) = item.pointer("/context/limits").and_then(Value::as_object) {
        for (k, v) in map {
            if let Some(n) = v.as_u64() {
                limits.insert(k.clone(), n);
            }
        }
    }
    Some((decision, reason, limits))
}

/// Reads a request and its answer. A body that does not parse gives no entry: the library refuses
/// such an answer itself.
fn tap(request: &[u8], response: &[u8]) -> Vec<Tapped> {
    let (Ok(req), Ok(resp)) = (serde_json::from_slice::<Value>(request), serde_json::from_slice::<Value>(response))
    else {
        return Vec::new();
    };
    let mode = if text(req.pointer("/context/match")) == Some("subtree") { "subtree" } else { "node" };
    let make = |question: &Value, answer: &Value| -> Option<Tapped> {
        let (decision, reason, ceilings) = answer_of(answer)?;
        let action = text(question.pointer("/action/name")).or_else(|| text(req.pointer("/action/name")))?;
        let resource = question.get("resource").or_else(|| req.get("resource"))?;
        Some(Tapped {
            action: action.to_owned(),
            resource: chain_text(resource),
            match_mode: mode,
            decision,
            reason,
            ceilings,
        })
    };
    match (req.get("evaluations").and_then(Value::as_array), resp.get("evaluations").and_then(Value::as_array)) {
        (Some(questions), Some(answers)) if questions.len() == answers.len() => {
            questions.iter().zip(answers).filter_map(|(q, a)| make(q, a)).collect()
        }
        (None, None) => make(&req, &resp).into_iter().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_single_exchange_is_recorded() {
        let req = json!({"subject": {"type": "user", "id": "a"}, "action": {"name": "dashboard.read"},
            "resource": {"type": "dashboard", "id": "p", "properties": {"ancestors": [
                {"type": "service", "id": "d"}, {"type": "folder", "id": "f"}]}}});
        let resp = json!({"decision": true, "context": {"limits": {"max_rows": 5}}});
        let ((), report) = with_call(CallInfo { request_id: "r".into(), traceparent: None }, || {
            record_exchange(req.to_string().as_bytes(), Some(resp.to_string().as_bytes()), 3);
        });
        assert_eq!(report.requests, 1);
        assert_eq!(report.answers.len(), 1);
        let a = &report.answers[0];
        assert_eq!(a.resource, "service:d/folder:f/dashboard:p");
        assert_eq!(a.action, "dashboard.read");
        assert_eq!(a.match_mode, "node");
        assert_eq!(a.ceilings.get("max_rows"), Some(&5));
    }

    #[test]
    fn a_batch_is_recorded_item_by_item_with_the_envelope() {
        let req = json!({"subject": {"type": "user", "id": "a"}, "context": {"match": "subtree"},
            "evaluations": [
                {"action": {"name": "dashboard.read"}, "resource": {"type": "folder", "id": "x",
                    "properties": {"ancestors": [{"type": "service", "id": "d"}]}}},
                {"action": {"name": "dashboard.write"}, "resource": {"type": "folder", "id": "y",
                    "properties": {"ancestors": [{"type": "service", "id": "d"}]}}}]});
        let resp = json!({"evaluations": [{"decision": true}, {"decision": false, "context": {"reason": "invalid_resource"}}]});
        let ((), report) = with_call(CallInfo::default(), || {
            record_exchange(req.to_string().as_bytes(), Some(resp.to_string().as_bytes()), 1);
        });
        assert_eq!(report.answers.len(), 2);
        assert_eq!(report.answers[0].match_mode, "subtree");
        assert!(report.answers[0].decision);
        assert_eq!(report.answers[1].reason.as_deref(), Some("invalid_resource"));
        assert_eq!(report.answers[1].resource, "service:d/folder:y");
    }

    #[test]
    fn no_context_outside_a_call() {
        record_exchange(b"{}", None, 1);
        assert_eq!(current_info().request_id, "");
    }
}
