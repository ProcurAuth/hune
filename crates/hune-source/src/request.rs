//! The request of 8.3: the URL parameters, the headers and the body.

use bytes::Bytes;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, HOST, USER_AGENT};
use http::{HeaderName, HeaderValue, Method, Request};
use http_body_util::Full;

use crate::config::ClickHouseSourceConfig;
use crate::error::SourceSetupError;
use crate::types::{TraceParent, Uuid};

/// The format of the answer (8.4).
pub(crate) const FORMAT: &str = "JSONCompactEachRowWithNamesAndTypes";

/// The value of `User-Agent`.
pub(crate) const USER_AGENT_VALUE: &str = concat!("hune/", env!("CARGO_PKG_VERSION"));

/// Percent-encodes one parameter value (8.3): each byte outside `A-Z a-z 0-9 - . _ ~` becomes `%XX`.
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0').to_ascii_uppercase());
            out.push(char::from_digit(u32::from(b & 15), 16).unwrap_or('0').to_ascii_uppercase());
        }
    }
    out
}

/// The values of the settings of one request (8.3).
#[derive(Debug, Clone)]
pub(crate) struct Settings<'a> {
    pub database: &'a str,
    pub query_id: Uuid,
    /// The whole seconds of `max_execution_time`.
    pub max_execution_time: u64,
    pub max_result_rows: u64,
    pub max_result_bytes: u64,
    pub max_query_size: u64,
    pub log_comment: &'a str,
    /// The pairs `&param_<name>=<value>` of the query parameters of 7.6a, percent-encoded, or the empty text.
    pub params: &'a str,
}

/// Builds the path and the query string of the request, with the parameters of the table of 8.3
/// in the order of the table. The pairs of the query parameters of 7.6a come last.
pub(crate) fn path_and_query(s: &Settings<'_>) -> String {
    let mut out = String::with_capacity(768);
    out.push_str("/?database=");
    out.push_str(&percent_encode(s.database));
    out.push_str("&default_format=");
    out.push_str(FORMAT);
    out.push_str("&query_id=");
    out.push_str(&s.query_id.to_string());
    out.push_str(&format!("&max_execution_time={}", s.max_execution_time));
    out.push_str("&timeout_overflow_mode=throw");
    out.push_str(&format!("&max_result_rows={}", s.max_result_rows));
    out.push_str(&format!("&max_result_bytes={}", s.max_result_bytes));
    out.push_str("&result_overflow_mode=throw");
    out.push_str(&format!("&max_query_size={}", s.max_query_size));
    out.push_str("&wait_end_of_query=1");
    out.push_str("&cancel_http_readonly_queries_on_client_close=1");
    out.push_str("&output_format_json_quote_64bit_integers=1");
    out.push_str("&output_format_json_quote_decimals=1");
    out.push_str("&output_format_json_quote_denormals=1");
    out.push_str("&output_format_json_validate_utf8=1");
    out.push_str("&date_time_output_format=unix_timestamp");
    out.push_str("&log_comment=");
    out.push_str(&percent_encode(s.log_comment));
    out.push_str(s.params);
    out
}

/// The headers that do not change from one request to the next.
#[derive(Clone)]
pub(crate) struct RequestTemplate {
    host: HeaderValue,
    user: HeaderValue,
    key: HeaderValue,
}

impl std::fmt::Debug for RequestTemplate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestTemplate").field("host", &self.host).field("key", &"(hidden)").finish()
    }
}

/// The text of the header `Host`: the host and the port, without the port when it is the default.
pub(crate) fn host_header_text(host: &str, port: u16, https: bool) -> String {
    let h = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_owned() };
    let default = if https { 443 } else { 80 };
    if port == default { h } else { format!("{h}:{port}") }
}

impl RequestTemplate {
    pub(crate) fn new(cfg: &ClickHouseSourceConfig) -> Result<Self, SourceSetupError> {
        let host = HeaderValue::from_str(&host_header_text(&cfg.host, cfg.port, cfg.https))
            .map_err(|_| SourceSetupError::new("host: the value cannot go in the HTTP header Host"))?;
        let user = HeaderValue::from_bytes(cfg.user.as_bytes())
            .map_err(|_| SourceSetupError::new("user: the value holds a byte that an HTTP header cannot hold"))?;
        let mut key = HeaderValue::from_bytes(cfg.password.as_bytes()).map_err(|_| {
            SourceSetupError::new("password: the value holds a byte that an HTTP header cannot hold (value hidden)")
        })?;
        key.set_sensitive(true);
        Ok(RequestTemplate { host, user, key })
    }

    /// Builds one request. The body is the SQL text.
    pub(crate) fn build(
        &self,
        path_and_query: &str,
        sql: &str,
        traceparent: Option<&TraceParent>,
    ) -> Result<Request<Full<Bytes>>, String> {
        let mut b = Request::builder()
            .method(Method::POST)
            .uri(path_and_query)
            .header(HOST, self.host.clone())
            .header(HeaderName::from_static("x-clickhouse-user"), self.user.clone())
            .header(HeaderName::from_static("x-clickhouse-key"), self.key.clone())
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .header(CONTENT_LENGTH, sql.len())
            .header(USER_AGENT, USER_AGENT_VALUE);
        if let Some(tp) = traceparent {
            b = b.header(HeaderName::from_static("traceparent"), tp.as_str());
        }
        b.body(Full::new(Bytes::copy_from_slice(sql.as_bytes()))).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d";

    #[test]
    fn percent_encoding_follows_8_3() {
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(percent_encode(" /?&=+%\"{}"), "%20%2F%3F%26%3D%2B%25%22%7B%7D");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode(""), "");
    }

    #[test]
    fn the_example_of_8_3() {
        let log = r#"{"app":"hune","kind":"panel","uid":"nodes","panel":4,"request_id":"r-5f2c","subject":"-"}"#;
        let s = Settings {
            database: "analytics",
            query_id: Uuid::parse(ID).unwrap(),
            max_execution_time: 60,
            max_result_rows: 100_000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            log_comment: log,
            params: "",
        };
        let expected = "/?database=analytics&default_format=JSONCompactEachRowWithNamesAndTypes\
            &query_id=019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d&max_execution_time=60&timeout_overflow_mode=throw\
            &max_result_rows=100000&max_result_bytes=33554432&result_overflow_mode=throw&max_query_size=1048576\
            &wait_end_of_query=1&cancel_http_readonly_queries_on_client_close=1\
            &output_format_json_quote_64bit_integers=1&output_format_json_quote_decimals=1\
            &output_format_json_quote_denormals=1&output_format_json_validate_utf8=1\
            &date_time_output_format=unix_timestamp&log_comment=%7B%22app%22%3A%22hune%22%2C%22kind%22%3A%22panel%22\
            %2C%22uid%22%3A%22nodes%22%2C%22panel%22%3A4%2C%22request_id%22%3A%22r-5f2c%22%2C%22subject%22\
            %3A%22-%22%7D";
        assert_eq!(path_and_query(&s), expected);
    }

    #[test]
    fn the_database_is_encoded() {
        let s = Settings {
            database: "a b&c",
            query_id: Uuid::parse(ID).unwrap(),
            max_execution_time: 1,
            max_result_rows: 1,
            max_result_bytes: 1,
            max_query_size: 1,
            log_comment: "{}",
            params: "",
        };
        assert!(path_and_query(&s).starts_with("/?database=a%20b%26c&default_format="));
    }

    #[test]
    fn no_setting_that_8_3_forbids() {
        let s = Settings {
            database: "d",
            query_id: Uuid::parse(ID).unwrap(),
            max_execution_time: 1,
            max_result_rows: 1,
            max_result_bytes: 1,
            max_query_size: 1,
            log_comment: "{}",
            params: "",
        };
        let q = path_and_query(&s);
        for forbidden in ["&readonly=", "session_id", "max_rows_to_read", "max_memory_usage", "password", "&user="] {
            assert!(!q.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn the_pairs_of_the_query_parameters_come_after_log_comment() {
        use hune_core::sql::{
            ExpandRequest, RequestValues, TextValueType, VariableDef, check_time_range, expand_prepared, resolve_values,
        };
        let defs = [VariableDef::text("t", TextValueType::Text), VariableDef::query("ids", true, false, None)];
        let mut req = RequestValues::new();
        req.insert("t", false, &["a b&c=\\d'é"]);
        req.insert("ids", false, &["1", "2"]);
        let values = resolve_values(&defs, &req).unwrap();
        let sql = "SELECT {t:String} AS t, has({ids:Array(UInt64)}, 2) AS h";
        let prepared = expand_prepared(
            hune_core::sql::Dialect::ClickHouse,
            &ExpandRequest {
                sql,
                values: &values,
                range: check_time_range(0, 1000).unwrap(),
                interval: None,
                max_query_size: 4096,
            },
        )
        .unwrap();
        assert_eq!(prepared.sql, sql, "the SQL body holds no value");
        let encoded = prepared.params.query_string();
        let s = Settings {
            database: "d",
            query_id: Uuid::parse(ID).unwrap(),
            max_execution_time: 1,
            max_result_rows: 1,
            max_result_bytes: 1,
            max_query_size: 1,
            log_comment: "{}",
            params: &encoded,
        };
        let q = path_and_query(&s);
        assert!(q.ends_with("&log_comment=%7B%7D&param_ids=%5B1%2C2%5D&param_t=a%20b%26c%3D%5C%5Cd%27%C3%A9"), "{q}");
        // The pairs follow the rule of 8.3: the core encoder gives the same bytes as `percent_encode`.
        for (name, text) in prepared.params.pairs() {
            assert!(q.contains(&format!("&param_{name}={}", percent_encode(text))));
        }
    }

    #[test]
    fn host_header() {
        assert_eq!(host_header_text("clickhouse", 8443, true), "clickhouse:8443");
        assert_eq!(host_header_text("clickhouse", 443, true), "clickhouse");
        assert_eq!(host_header_text("127.0.0.1", 8123, false), "127.0.0.1:8123");
        assert_eq!(host_header_text("127.0.0.1", 80, false), "127.0.0.1");
        assert_eq!(host_header_text("::1", 8123, false), "[::1]:8123");
    }
}
