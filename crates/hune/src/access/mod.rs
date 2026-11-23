// SPDX-License-Identifier: MIT OR Apache-2.0
//! The access layer of the mode `authzen` (section 11): the PEP on `procurauth-pep`, the rustls
//! transport to the PDP, the bounded blocking pool, the single flight of equal questions, and the
//! mapping of each refusal to the error table of section 6.
//!
//! `Access` is built one time at start from the typed configuration. It sends nothing on the network
//! at start (11.6). In the mode `open`, no `Access` exists and no route asks a question.
//!
//! The route-level questions (11.21) are in `server/handlers/gate.rs`. This module gives the three
//! calls that they use: `decide` for a use, `check` for a flag, and `display` for the display filter.
//! `decide_fresh` is `decide` with no kept answer and no single flight (11.27).

mod call;
mod inputs;
mod options;
mod rules;
mod transport;
mod tree;

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use http::HeaderMap;
use procurauth_pep::operation::{NodePath, Operation, Permission};
use procurauth_pep::refusal::{AuthFailure, DenialCause, UnavailableCause};
use procurauth_pep::{Allowed, Identity, Outcome, Pep, Refusal};
use tokio::sync::{Semaphore, oneshot};

use crate::config::LogLevel;
use crate::log::Logger;
use crate::metrics::Metrics;
use crate::server::error::{ApiError, Code};
use crate::telemetry::{ClientSpan, Span, Tracing};

pub use call::{CallInfo, CallReport};
pub use inputs::{PepInputs, SourceInfo};
pub use options::{OptionChild, cut_label, parse_options_query, walk_options};
pub use rules::{Rule, rule_of};
pub use transport::PdpLink;
pub use tree::Tree;

/// The reserved texts of the history that `sub` cannot be (11.8, section 5).
const RESERVED_SUBJECTS: [&str; 3] = ["-", "external", "import-grafana"];
/// The largest identity token, in bytes (11.8).
pub const MAX_TOKEN_BYTES: usize = 16_384;

/// The subject of a request and its request id, for the log lines of a question.
#[derive(Debug, Clone)]
pub struct Who {
    /// The identity that the token gave.
    pub identity: Identity,
    /// The request id (14.6).
    pub request_id: String,
    /// The tracing context of the request (14.10). A call to the PDP makes a CLIENT span in it.
    pub trace: Tracing,
}

/// Why a request has no valid token (11.8): the word of the log line `access.unauthenticated`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unauthenticated {
    /// `missing`, `duplicate_header`, `structure`, `header`, `alg`, `typ`, `kid`, `signature`, `claims`,
    /// `iss`, `sub`, `aud`, `iat`, `exp`, `life` or `subject`.
    pub reason: String,
}

/// What one call of the library gave.
enum Done {
    Outcome(Outcome),
    Display(Result<Vec<bool>, Refusal>),
    /// No permit of the pool came within `access.pdp.timeout` (11.13).
    Busy,
}

struct CallResult {
    done: Done,
    report: CallReport,
}

#[derive(Clone)]
enum Work {
    Decide(Operation),
    Check(Operation),
    Display(Vec<Permission>),
}

impl Work {
    fn name(&self) -> &'static str {
        match self {
            Work::Decide(_) => "decide",
            Work::Check(_) => "check",
            Work::Display(_) => "display",
        }
    }

    fn questions(&self) -> usize {
        match self {
            Work::Decide(op) | Work::Check(op) => op.uses().len().min(1) + op.others().len(),
            Work::Display(list) => list.len(),
        }
    }

    fn key_text(&self) -> String {
        let one = |p: &Permission| format!("{}@{}", p.action, node_text(&p.node));
        match self {
            Work::Decide(op) | Work::Check(op) => {
                let mut s = op.uses().iter().map(one).collect::<Vec<_>>().join(",");
                s.push('+');
                s.push_str(&op.others().iter().map(one).collect::<Vec<_>>().join(","));
                s
            }
            Work::Display(list) => list.iter().map(one).collect::<Vec<_>>().join(","),
        }
    }
}

/// The text form of a node with its chain (11.16): `service:dashboards/folder:transit/dashboard:score`.
pub fn node_text(node: &NodePath) -> String {
    node.nodes().iter().map(|n| format!("{}:{}", n.node_type, n.id)).collect::<Vec<_>>().join("/")
}

type Waiters = Vec<oneshot::Sender<Arc<CallResult>>>;

/// The access layer: the PEP, the pool and the logs.
pub struct Access {
    pep: Arc<Pep>,
    /// The PEP of the questions that are never kept (11.27): a second PEP with no cache on the same link. With
    /// `cache_ttl = "0s"` it is `pep`, which keeps no answer either.
    fresh: Arc<Pep>,
    sem: Arc<Semaphore>,
    timeout: Duration,
    max_connections: usize,
    coalesce: bool,
    flights: Mutex<HashMap<String, Waiters>>,
    in_use: AtomicI64,
    logger: Arc<Logger>,
    metrics: Arc<Metrics>,
    editing: bool,
    tree: Tree,
    /// The host of `access.pdp.url`, for the attribute `server.address` of the span `pdp.evaluate`.
    pdp_host: String,
}

impl std::fmt::Debug for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Access").field("service", &self.tree.service_id()).finish_non_exhaustive()
    }
}

/// Ends the span of a call. A call that failed (the pool, the transport or an answer that is not valid)
/// sets the error status. A refusal by the PDP is an answer, not a failure.
fn end_pdp_span(mut span: Span, result: &CallResult) {
    let failed = |r: &Refusal| !matches!(r, Refusal::Forbidden { .. } | Refusal::RateLimited { .. });
    let error = match &result.done {
        Done::Busy => true,
        Done::Outcome(Outcome::Refused(r)) | Done::Display(Err(r)) => failed(r),
        Done::Outcome(_) | Done::Display(Ok(_)) => false,
    };
    if error {
        span.set_error();
    }
    span.end();
}

/// The result of a call whose task ended with no result: an internal error.
fn no_result() -> Arc<CallResult> {
    Arc::new(CallResult {
        done: Done::Outcome(Outcome::Refused(Refusal::Internal("the call ended with no result".to_owned()))),
        report: CallReport::default(),
    })
}

fn pdp_unavailable() -> ApiError {
    ApiError::new(Code::PdpUnavailable, "The access decision service does not answer. Try again later.")
}

/// The body of `403 forbidden` (11.30). Its member `action` is the action of the use of the route, and never the
/// action of the question that the PDP refused.
pub(crate) fn forbidden(action: &str) -> ApiError {
    ApiError::new(Code::Forbidden, "You have no right for this operation.").with("action", action.to_owned())
}

impl Access {
    /// Builds the PEP and the transport from the inputs (11.10 to 11.13). It sends nothing.
    pub fn new(
        inputs: &PepInputs,
        ca_pem: Option<&[u8]>,
        logger: Arc<Logger>,
        metrics: Arc<Metrics>,
    ) -> Result<Access, String> {
        let link = Arc::new(PdpLink::new(
            inputs.plain_http,
            ca_pem,
            inputs.timeout,
            inputs.max_connections,
            inputs::MAX_RESPONSE_BYTES,
            inputs::IDLE_CLOSE_AFTER,
            metrics.clone(),
        )?);
        let config = inputs.build_pep_config(false)?;
        let document = inputs.build_document().map_err(|f| {
            format!("the action document is not valid: {}", f.first().map_or_else(String::new, ToString::to_string))
        })?;
        let transport: Arc<dyn procurauth_pep::transport::PdpTransport> = link.clone();
        let build = |config, document| {
            Pep::new(config, document, transport.clone())
                .map(Arc::new)
                .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "))
        };
        let pep = build(config, document.clone())?;
        let fresh = if inputs.cache_ttl.is_zero() {
            pep.clone()
        } else {
            let mut config = inputs.build_pep_config(false)?;
            config.cache_ttl = Duration::ZERO;
            build(config, document)?
        };
        Ok(Access {
            pep,
            fresh,
            sem: Arc::new(Semaphore::new(inputs.max_connections.max(1))),
            timeout: inputs.timeout,
            max_connections: inputs.max_connections.max(1),
            coalesce: !inputs.cache_ttl.is_zero(),
            flights: Mutex::new(HashMap::new()),
            in_use: AtomicI64::new(0),
            logger,
            metrics,
            editing: inputs.editing,
            tree: Tree::new(&inputs.audience),
            pdp_host: crate::config::parse_http_url(&inputs.pdp_url, true).map(|(u, _)| u.host).unwrap_or_default(),
        })
    }

    /// Builds the access layer from the typed configuration (14.12, step 5). It gives `None` in the
    /// mode `open`. The error is the text for `server.start_failed`.
    pub fn from_config(
        config: &crate::config::Config,
        logger: Arc<Logger>,
        metrics: Arc<Metrics>,
    ) -> Result<Option<Arc<Access>>, String> {
        let Some(inputs) = PepInputs::from_config(config)? else { return Ok(None) };
        let ca = match &config.access {
            crate::config::AccessConfig::Authzen(a) => a.pdp.ca_pem.as_deref(),
            crate::config::AccessConfig::Open => None,
        };
        Access::new(&inputs, ca, logger, metrics).map(|a| Some(Arc::new(a)))
    }

    /// The tree of nodes (11.15).
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// True when `dashboard.write` and `source.query` are published (11.18).
    pub fn editing(&self) -> bool {
        self.editing
    }

    /// The most connections to the PDP, and the size of the pool (11.13).
    #[cfg(test)]
    pub fn pool_size(&self) -> usize {
        self.max_connections
    }

    /// The PEP, for the action document and the options route.
    pub fn pep(&self) -> &Pep {
        &self.pep
    }

    /// The reply for the action document (11.18): the same bytes at each read.
    pub fn actions_body(&self) -> Vec<u8> {
        self.pep.actions_reply("GET").body
    }

    // ---- The identity token (11.8) -------------------------------------------------------------

    /// Checks the identity token of a request. It sends no question, so it runs on the async worker.
    pub fn authenticate(&self, headers: &HeaderMap) -> Result<Identity, Unauthenticated> {
        let name = self.pep.token_header().to_owned();
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for (n, v) in headers {
            if !n.as_str().eq_ignore_ascii_case(&name) {
                continue;
            }
            match v.to_str() {
                Ok(text) if text.len() <= MAX_TOKEN_BYTES => pairs.push((n.as_str(), text)),
                // A token that is not text, or too large, has no valid structure.
                _ => pairs.push((n.as_str(), "!")),
            }
        }
        let identity = self.pep.authenticate(pairs).map_err(|r| Unauthenticated {
            reason: match r {
                Refusal::Unauthenticated(AuthFailure::NoToken) => "missing".to_owned(),
                Refusal::Unauthenticated(AuthFailure::DuplicateHeader) => "duplicate_header".to_owned(),
                Refusal::Unauthenticated(AuthFailure::Token { check, .. }) => check,
                _ => "structure".to_owned(),
            },
        })?;
        let sub = identity.sub();
        if sub.is_empty() || sub.len() > 256 || sub.chars().any(char::is_control) || RESERVED_SUBJECTS.contains(&sub) {
            return Err(Unauthenticated { reason: "subject".to_owned() });
        }
        Ok(identity)
    }

    // ---- The three calls ----------------------------------------------------------------------

    /// Asks for one operation, with its use (11.21). A `true` answer gives the ceilings.
    pub async fn decide(self: &Arc<Self>, who: &Who, op: Operation) -> Result<Allowed, ApiError> {
        self.decide_with(who, op, false).await
    }

    /// Asks for one operation as `decide` does, but each question goes to the PDP: no kept answer is read or kept,
    /// and no other call shares the flight (11.27). The write routes and the question of 11.23 use it.
    pub async fn decide_fresh(self: &Arc<Self>, who: &Who, op: Operation) -> Result<Allowed, ApiError> {
        self.decide_with(who, op, true).await
    }

    async fn decide_with(self: &Arc<Self>, who: &Who, op: Operation, fresh: bool) -> Result<Allowed, ApiError> {
        let work = Work::Decide(op.clone());
        let (res, leader) = self.call(who, work.clone(), fresh).await;
        match &res.done {
            Done::Outcome(Outcome::Allowed(a)) => {
                self.account(who, &work, &res, leader);
                self.check_ceilings(who, a)?;
                Ok(a.clone())
            }
            Done::Outcome(Outcome::Refused(r)) => Err(self.refusal(who, &op, &work, &res, leader, r)),
            Done::Outcome(Outcome::ApprovalRequired(_)) | Done::Display(_) => Err(ApiError::internal()),
            Done::Busy => Err(self.busy(who, &work)),
        }
    }

    /// Asks for one operation without usage, for a flag (11.25). `true` lets the operation run, and
    /// `false` is a refusal 403. A failure of the PDP is an error.
    pub async fn check(self: &Arc<Self>, who: &Who, op: Operation) -> Result<bool, ApiError> {
        let work = Work::Check(op.clone());
        let (res, leader) = self.call(who, work.clone(), false).await;
        match &res.done {
            Done::Outcome(Outcome::Allowed(a)) => {
                self.account(who, &work, &res, leader);
                self.check_ceilings(who, a)?;
                Ok(true)
            }
            Done::Outcome(Outcome::Refused(r @ (Refusal::Forbidden { .. } | Refusal::RateLimited { .. }))) => {
                self.account(who, &work, &res, leader);
                self.log_unenforceable(who, &op, r, &res);
                Ok(false)
            }
            Done::Outcome(Outcome::Refused(r)) => Err(self.refusal(who, &op, &work, &res, leader, r)),
            Done::Outcome(Outcome::ApprovalRequired(_)) | Done::Display(_) => Err(ApiError::internal()),
            Done::Busy => Err(self.busy(who, &work)),
        }
    }

    /// Asks the display questions with the match `subtree` (11.25): one answer for each permission.
    pub async fn display(self: &Arc<Self>, who: &Who, perms: Vec<Permission>) -> Result<Vec<bool>, ApiError> {
        if perms.is_empty() {
            return Ok(Vec::new());
        }
        let work = Work::Display(perms);
        let (res, leader) = self.call(who, work.clone(), false).await;
        match &res.done {
            Done::Display(Ok(list)) => {
                self.account(who, &work, &res, leader);
                Ok(list.clone())
            }
            Done::Display(Err(r)) => {
                let op = match &work {
                    Work::Display(p) => Operation::new(p[0].action.clone(), p[0].node.clone()),
                    _ => return Err(ApiError::internal()),
                };
                Err(self.refusal(who, &op, &work, &res, leader, r))
            }
            Done::Outcome(_) => Err(ApiError::internal()),
            Done::Busy => Err(self.busy(who, &work)),
        }
    }

    /// Runs many `check` calls, `pool_size` at a time. The answers keep the order of the operations.
    pub async fn check_many(self: &Arc<Self>, who: &Who, ops: Vec<Operation>) -> Result<Vec<bool>, ApiError> {
        let mut out = Vec::with_capacity(ops.len());
        for chunk in ops.chunks(self.max_connections) {
            let mut set = tokio::task::JoinSet::new();
            for (i, op) in chunk.iter().cloned().enumerate() {
                let (me, w) = (self.clone(), who.clone());
                set.spawn(async move { (i, me.check(&w, op).await) });
            }
            let mut part: Vec<Option<Result<bool, ApiError>>> = (0..chunk.len()).map(|_| None).collect();
            while let Some(joined) = set.join_next().await {
                let (i, r) = joined.map_err(|_| ApiError::internal())?;
                part[i] = Some(r);
            }
            for r in part {
                out.push(r.ok_or_else(ApiError::internal)??);
            }
        }
        Ok(out)
    }

    // ---- The call: single flight, pool, blocking thread ----------------------------------------

    /// Starts the CLIENT span `pdp.evaluate` of a call that goes to the PDP (14.10). The header
    /// `traceparent` of the request to the PDP comes from it.
    fn pdp_span(&self, who: &Who, work: &Work) -> Span {
        let mut span = who.trace.client(ClientSpan::PdpEvaluate);
        span.set_str("server.address", &self.pdp_host);
        span.set_int("hune.questions", i64::try_from(work.questions()).unwrap_or(i64::MAX));
        span
    }

    /// One call of the library. `fresh` uses the PEP with no cache and no single flight (11.27).
    async fn call(self: &Arc<Self>, who: &Who, work: Work, fresh: bool) -> (Arc<CallResult>, bool) {
        if fresh || !self.coalesce {
            let (me, identity) = (self.clone(), who.identity.clone());
            let span = self.pdp_span(who, &work);
            let info = CallInfo { request_id: who.request_id.clone(), traceparent: span.traceparent() };
            // The call runs in its own task: when the client leaves, it still ends, with its span and the count of
            // the permits in use (11.13).
            let task = tokio::spawn(async move {
                let result = me.run_pooled(identity, info, work, fresh).await;
                end_pdp_span(span, &result);
                Arc::new(result)
            });
            return match task.await {
                Ok(r) => (r, true),
                Err(_) => (no_result(), false),
            };
        }
        // One flight for each question (11.27): equal calls that wait together share one call.
        let key = format!("{}|{}|{}", who.identity.sub(), work.name(), work.key_text());
        let (tx, rx) = oneshot::channel();
        let leader = {
            let mut map = self.flights.lock().unwrap_or_else(PoisonError::into_inner);
            match map.get_mut(&key) {
                Some(waiters) => {
                    waiters.push(tx);
                    false
                }
                None => {
                    map.insert(key.clone(), vec![tx]);
                    true
                }
            }
        };
        if leader {
            let (me, identity) = (self.clone(), who.identity.clone());
            let span = self.pdp_span(who, &work);
            let info = CallInfo { request_id: who.request_id.clone(), traceparent: span.traceparent() };
            // The call cannot stop early: it runs to its end, also when the client leaves (11.13).
            tokio::spawn(async move {
                let result = me.run_pooled(identity, info, work, false).await;
                end_pdp_span(span, &result);
                let result = Arc::new(result);
                let waiters =
                    me.flights.lock().unwrap_or_else(PoisonError::into_inner).remove(&key).unwrap_or_default();
                for w in waiters {
                    let _ = w.send(result.clone());
                }
            });
        }
        match rx.await {
            Ok(r) => (r, leader),
            Err(_) => (no_result(), false),
        }
    }

    async fn run_pooled(self: &Arc<Self>, identity: Identity, info: CallInfo, work: Work, fresh: bool) -> CallResult {
        let permit = match tokio::time::timeout(self.timeout, self.sem.clone().acquire_owned()).await {
            Ok(Ok(p)) => p,
            _ => {
                self.metrics.pdp_request("pool_busy");
                return CallResult { done: Done::Busy, report: CallReport::default() };
            }
        };
        let now = self.in_use.fetch_add(1, Ordering::SeqCst) + 1;
        self.metrics.pdp_pool_in_use(now);
        let pep = if fresh { self.fresh.clone() } else { self.pep.clone() };
        let joined = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            call::with_call(info, || match work {
                Work::Decide(op) => Done::Outcome(pep.decide(&identity, &op)),
                Work::Check(op) => Done::Outcome(pep.check(&identity, &op)),
                Work::Display(list) => Done::Display(pep.display(&identity, &list)),
            })
        })
        .await;
        let now = self.in_use.fetch_sub(1, Ordering::SeqCst) - 1;
        self.metrics.pdp_pool_in_use(now);
        match joined {
            Ok((done, report)) => CallResult { done, report },
            Err(_) => CallResult {
                done: Done::Outcome(Outcome::Refused(Refusal::Internal("the blocking call failed".to_owned()))),
                report: CallReport::default(),
            },
        }
    }

    // ---- Logs, metrics and the mapping of refusals ---------------------------------------------

    /// Writes the lines `access.decision` and counts the questions of a call that ended with an answer.
    fn account(&self, who: &Who, work: &Work, res: &CallResult, leader: bool) {
        if !leader {
            return;
        }
        if res.report.requests == 0 {
            for _ in 0..work.questions() {
                self.metrics.pdp_cache_hit();
            }
            return;
        }
        for a in &res.report.answers {
            self.metrics.pdp_question(a.decision);
            let ceilings = serde_json::to_value(&a.ceilings).unwrap_or(serde_json::Value::Null);
            self.logger
                .event(LogLevel::Info, "access.decision")
                .request_id(&who.request_id)
                .field("subject", who.identity.sub())
                .opt_str("owner", who.identity.owner())
                .field("action", a.action.as_str())
                .field("resource", a.resource.as_str())
                .field("match", a.match_mode)
                .field("decision", a.decision)
                .opt_str("reason", a.reason.as_deref())
                .field("ceilings", ceilings)
                .field("duration_ms", res.report.duration_ms)
                .emit();
        }
    }

    /// A `true` answer with a ceiling below the `min` of its declaration is not valid (11.24).
    fn check_ceilings(&self, who: &Who, allowed: &Allowed) -> Result<(), ApiError> {
        let granted = std::iter::once(allowed.ceilings()).chain(allowed.others().iter().map(|g| &g.ceilings));
        for ceilings in granted {
            if let Some((name, _)) = ceilings.iter().find(|(_, v)| *v < 1) {
                self.invalid_answer(who, &format!("the ceiling `{name}` is below the minimum of 1"));
                return Err(pdp_unavailable());
            }
        }
        Ok(())
    }

    fn invalid_answer(&self, who: &Who, detail: &str) {
        self.metrics.pdp_request("invalid");
        self.logger
            .event(LogLevel::Error, "access.pdp_invalid_answer")
            .request_id(&who.request_id)
            .field("detail", detail)
            .emit();
    }

    fn busy(&self, who: &Who, work: &Work) -> ApiError {
        self.logger
            .event(LogLevel::Warn, "access.pdp_failed")
            .request_id(&who.request_id)
            .field("questions", work.questions() as u64)
            .field("error", "pool_busy")
            .emit();
        pdp_unavailable()
    }

    fn log_unenforceable(&self, who: &Who, op: &Operation, r: &Refusal, res: &CallResult) {
        if let Refusal::Forbidden { cause: DenialCause::UnknownCeiling(name), .. } = r {
            let action = res
                .report
                .answers
                .iter()
                .find(|a| a.ceilings.contains_key(name))
                .map_or_else(|| op.uses()[0].action.clone(), |a| a.action.clone());
            self.logger
                .event(LogLevel::Warn, "access.ceiling_unenforceable")
                .request_id(&who.request_id)
                .field("action", action)
                .field("ceiling", name.as_str())
                .emit();
        }
    }

    /// Maps a refusal to the error of 11.28 and writes its log line.
    fn refusal(
        &self,
        who: &Who,
        op: &Operation,
        work: &Work,
        res: &CallResult,
        leader: bool,
        refusal: &Refusal,
    ) -> ApiError {
        match refusal {
            Refusal::Forbidden { .. } | Refusal::RateLimited { .. } => {
                self.account(who, work, res, leader);
                self.log_unenforceable(who, op, refusal, res);
                // The use names the action of the body. Hune asks no question to find the refused one: the log
                // line `access.decision` gives it, and the body is the same for each refusal of a route (11.30).
                forbidden(op.uses().first().map_or("unknown", |p| p.action.as_str()))
            }
            Refusal::Unavailable(cause) => {
                match cause {
                    UnavailableCause::Transport(text) => {
                        self.logger
                            .event(LogLevel::Warn, "access.pdp_failed")
                            .request_id(&who.request_id)
                            .field("questions", work.questions() as u64)
                            .field("error", text.as_str())
                            .emit();
                    }
                    UnavailableCause::Status(code) => {
                        self.logger
                            .event(LogLevel::Warn, "access.pdp_failed")
                            .request_id(&who.request_id)
                            .field("questions", work.questions() as u64)
                            .field("error", format!("the PDP answered with the HTTP status {code}"))
                            .emit();
                        if matches!(code, 401 | 403) {
                            self.logger
                                .event(LogLevel::Error, "access.pdp_credential_refused")
                                .request_id(&who.request_id)
                                .field("status", u64::from(*code))
                                .emit();
                        }
                    }
                    UnavailableCause::Answer(text) | UnavailableCause::Metadata(text) => {
                        self.invalid_answer(who, text);
                    }
                }
                pdp_unavailable()
            }
            Refusal::Internal(text) => {
                self.logger
                    .event(LogLevel::Error, "http.access_internal")
                    .request_id(&who.request_id)
                    .field("error", text.as_str())
                    .emit();
                ApiError::internal()
            }
            Refusal::Unauthenticated(_) | Refusal::Approval(_) => ApiError::internal(),
        }
    }
}

#[cfg(test)]
mod tests;
