// SPDX-License-Identifier: MIT OR Apache-2.0
//! The reader of the answer (8.4): a state machine that takes the bytes when they arrive.
//! It checks each line and builds the frame row by row.

use hune_core::frame::{Frame, FrameBuilder, FrameStats};

use crate::error::{ErrorCtx, SourceError};

/// The size limit of the text of an error answer that Hune reads, in bytes.
pub(crate) const ERROR_BODY_CAP: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// The next line holds the column names.
    Names,
    /// The next line holds the column types.
    Types(Vec<String>),
    /// The next lines are rows.
    Rows,
    /// An empty line ended the rows. The lines that follow hold totals or extremes.
    Trailer,
}

/// Reads the body of a successful answer.
///
/// The parser keeps at most one partial line in memory, and the frame. It stops at the first
/// error. The caller closes the connection after an error.
#[derive(Debug)]
pub(crate) struct AnswerParser<'a> {
    ectx: ErrorCtx<'a>,
    state: State,
    builder: Option<FrameBuilder>,
    pending: Vec<u8>,
    total: u64,
    /// True after the marker `__exception__`: the next line `Code: N` is the error.
    in_exception_block: bool,
}

/// Reads `Code: <n>.` at the start of a line of a ClickHouse exception text.
pub(crate) fn exception_code(line: &str) -> Option<i64> {
    let rest = line.strip_prefix("Code: ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with('.') {
        return None;
    }
    digits.parse().ok()
}

impl<'a> AnswerParser<'a> {
    pub(crate) fn new(ectx: ErrorCtx<'a>) -> Self {
        AnswerParser {
            ectx,
            state: State::Names,
            builder: None,
            pending: Vec::new(),
            total: 0,
            in_exception_block: false,
        }
    }

    /// Takes the next bytes of the body.
    pub(crate) fn feed(&mut self, mut chunk: &[u8]) -> Result<(), SourceError> {
        self.total += chunk.len() as u64;
        if self.total > self.ectx.limits.max_result_bytes {
            return Err(self.ectx.bytes());
        }
        while let Some(pos) = chunk.iter().position(|&b| b == b'\n') {
            let (head, tail) = chunk.split_at(pos);
            if self.pending.is_empty() {
                self.line(head)?;
            } else {
                let mut line = std::mem::take(&mut self.pending);
                line.extend_from_slice(head);
                self.line(&line)?;
                line.clear();
                self.pending = line;
            }
            chunk = &tail[1..];
        }
        self.pending.extend_from_slice(chunk);
        Ok(())
    }

    fn line(&mut self, raw: &[u8]) -> Result<(), SourceError> {
        let Ok(line) = std::str::from_utf8(raw) else {
            return Err(self.ectx.bad_response("A line of the answer is not valid UTF-8."));
        };
        let trimmed = line.trim_end_matches('\r');
        // An exception text can follow the rows after the status line was sent.
        if self.in_exception_block || (!trimmed.starts_with('[') && !trimmed.is_empty()) {
            if trimmed == "__exception__" {
                self.in_exception_block = true;
                return Ok(());
            }
            if let Some(code) = exception_code(trimmed) {
                return Err(self.ectx.map_clickhouse(code, trimmed));
            }
            if self.in_exception_block {
                // The tag line comes before the message line.
                return Ok(());
            }
            if self.state == State::Trailer {
                return Ok(());
            }
            return Err(self.ectx.bad_response("A line of the answer is not a JSON array."));
        }
        match &mut self.state {
            State::Names => {
                if trimmed.is_empty() {
                    return Err(self.ectx.bad_response("The answer has fewer than two header lines."));
                }
                match hune_core::frame::parse_header_line(line, "names") {
                    Ok(names) => self.state = State::Types(names),
                    Err(e) => return Err(self.ectx.bad_response(e.to_string())),
                }
            }
            State::Types(names) => {
                let names = std::mem::take(names);
                let built = hune_core::frame::parse_header_line(line, "types")
                    .and_then(|types| FrameBuilder::new(names, types));
                match built {
                    Ok(b) => {
                        self.builder = Some(b);
                        self.state = State::Rows;
                    }
                    Err(e) => return Err(self.ectx.bad_response(e.to_string())),
                }
            }
            State::Rows => {
                if trimmed.is_empty() {
                    self.state = State::Trailer;
                    return Ok(());
                }
                let Some(builder) = self.builder.as_mut() else {
                    return Err(self.ectx.bad_response("The answer has no header."));
                };
                builder.push_row(line).map_err(|e| self.ectx.bad_response(e.to_string()))?;
                if builder.rows() as u64 > self.ectx.limits.max_result_rows {
                    return Err(self.ectx.rows());
                }
            }
            State::Trailer => {}
        }
        Ok(())
    }

    /// Ends the answer and gives the frame.
    pub(crate) fn finish(mut self, query_id: String, stats: FrameStats) -> Result<Frame, SourceError> {
        if !self.pending.is_empty() {
            let last = std::mem::take(&mut self.pending);
            self.line(&last)?;
        }
        if self.in_exception_block {
            return Err(self.ectx.bad_response("The answer ends with an exception marker and no error text."));
        }
        match self.builder {
            Some(b) => Ok(b.finish(query_id, stats)),
            None => Err(self.ectx.bad_response("The answer has fewer than two header lines.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EffectiveLimits, Uuid};
    use hune_core::frame::FrameValue;
    use hune_core::ids::SourceId;
    use std::time::Duration;

    fn limits(rows: u64, bytes: u64) -> EffectiveLimits {
        EffectiveLimits {
            max_execution_time: Duration::from_secs(60),
            max_result_rows: rows,
            max_result_bytes: bytes,
            max_query_size: 1 << 20,
            rows_from_ceiling: false,
            time_from_ceiling: false,
        }
    }

    const BODY: &str = "[\"t\",\"v\",\"s\"]\n[\"DateTime\",\"LowCardinality(String)\",\"Decimal(38, 9)\"]\n\
        [\"1794211200\",\"a1\",\"12.500000000\"]\n[\"1794211220\",null,\"3.000000000\"]\n";

    fn run(body: &[u8], chunk: usize, l: &EffectiveLimits) -> Result<Frame, SourceError> {
        let id = SourceId::parse("prod").unwrap();
        let mut p = AnswerParser::new(ErrorCtx { source: &id, query_id: Uuid::now_v7(), limits: l });
        for c in body.chunks(chunk) {
            p.feed(c)?;
        }
        p.finish("q".into(), FrameStats::default())
    }

    #[test]
    fn parses_the_example_of_8_4_in_any_chunk_size() {
        let l = limits(100, 1 << 20);
        for chunk in [1, 2, 3, 7, 16, 1000] {
            let f = run(BODY.as_bytes(), chunk, &l).unwrap();
            assert_eq!(f.rows, 2, "chunk {chunk}");
            assert_eq!(f.columns[0].value(1), Some(FrameValue::Number("1794211220")));
            assert_eq!(f.columns[1].value(1), Some(FrameValue::Null));
            assert_eq!(f.columns[2].value(0), Some(FrameValue::Text("12.500000000")));
        }
    }

    #[test]
    fn an_empty_line_ends_the_rows_and_the_rest_is_ignored() {
        let l = limits(100, 1 << 20);
        let body = format!("{BODY}\n[\"totals\"]\n[1,2,3]\n");
        assert_eq!(run(body.as_bytes(), 5, &l).unwrap().rows, 2);
    }

    #[test]
    fn no_final_line_end_is_accepted_and_no_row_is_accepted() {
        let l = limits(100, 1 << 20);
        let body = BODY.trim_end_matches('\n');
        assert_eq!(run(body.as_bytes(), 9, &l).unwrap().rows, 2);
        let head_only = "[\"a\"]\n[\"UInt8\"]\n";
        assert_eq!(run(head_only.as_bytes(), 9, &l).unwrap().rows, 0);
    }

    #[test]
    fn shape_errors_are_bad_responses() {
        let l = limits(100, 1 << 20);
        for body in [
            "",
            "[\"a\"]\n",
            "\n",
            "[]\n[]\n",
            "[\"a\"]\n[\"UInt8\",\"String\"]\n",
            "[\"a\"]\n[\"UInt8\"]\n[1,2]\n",
            "[\"a\"]\n[\"UInt8\"]\n[]\n",
            "[\"a\"]\n[\"UInt8\"]\n{\"a\":1}\n",
            "[\"a\"]\n[\"UInt8\"]\n[\"x\"]\n",
            "[\"a\"]\n[\"UInt8\"]\nsome text\n",
            "[\"a\"]\n[\"UInt8\"]\n[1,\n",
            "[1]\n[\"UInt8\"]\n",
            "text\n",
        ] {
            let e = run(body.as_bytes(), 4, &l).expect_err(body);
            assert_eq!(e.code(), "source_bad_response", "{body:?}");
            assert_eq!(e.http_status(), 502);
        }
    }

    #[test]
    fn invalid_utf8_is_a_bad_response() {
        let l = limits(100, 1 << 20);
        let mut body = b"[\"a\"]\n[\"String\"]\n[\"".to_vec();
        body.extend_from_slice(&[0xff, 0xfe]);
        body.extend_from_slice(b"\"]\n");
        assert_eq!(run(&body, 100, &l).unwrap_err().code(), "source_bad_response");
    }

    #[test]
    fn the_row_limit() {
        let body = "[\"a\"]\n[\"UInt8\"]\n[1]\n[2]\n[3]\n";
        let e = run(body.as_bytes(), 100, &limits(2, 1 << 20)).unwrap_err();
        assert_eq!((e.code(), e.limit_name(), e.limit_value()), ("query_limit", Some("max_result_rows"), Some(2)));
        assert_eq!(run(body.as_bytes(), 100, &limits(3, 1 << 20)).unwrap().rows, 3);
        let mut l = limits(2, 1 << 20);
        l.rows_from_ceiling = true;
        let e = run(body.as_bytes(), 100, &l).unwrap_err();
        assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_rows"), Some(2)));
    }

    #[test]
    fn the_byte_limit_counts_the_whole_body() {
        let body = BODY.as_bytes();
        let exact = limits(100, body.len() as u64);
        assert!(run(body, 10, &exact).is_ok());
        let e = run(body, 10, &limits(100, body.len() as u64 - 1)).unwrap_err();
        assert_eq!((e.code(), e.limit_name()), ("query_limit", Some("max_result_bytes")));
        assert_eq!(e.limit_value(), Some(body.len() as u64 - 1));
        // The limit stops the parser before the end of a long line.
        let e = run(&vec![b'['; 5000], 100, &limits(100, 1000)).unwrap_err();
        assert_eq!(e.limit_name(), Some("max_result_bytes"));
    }

    #[test]
    fn an_exception_text_after_the_rows() {
        let l = limits(100, 1 << 20);
        let body =
            format!("{BODY}Code: 241. DB::Exception: Memory limit (for query) exceeded (MEMORY_LIMIT_EXCEEDED)\n");
        let e = run(body.as_bytes(), 6, &l).unwrap_err();
        assert_eq!(e.code(), "query_memory_limit");
        // An exception text without a final line end.
        let body = format!("{BODY}Code: 159. DB::Exception: Timeout exceeded");
        assert_eq!(run(body.as_bytes(), 6, &l).unwrap_err().code(), "query_timeout");
        // Before the header, and for an unknown code.
        let e = run(b"Code: 62. DB::Exception: Syntax error\n", 6, &l).unwrap_err();
        assert_eq!((e.code(), e.clickhouse_code()), ("query_error", Some(62)));
    }

    #[test]
    fn the_exception_marker_of_recent_releases() {
        let l = limits(100, 1 << 20);
        let body = format!(
            "{BODY}\r\n__exception__\r\nTAG123456789\r\nCode: 241. DB::Exception: Memory limit exceeded\r\n55 TAG123456789\r\n__exception__\r\n"
        );
        assert_eq!(run(body.as_bytes(), 7, &l).unwrap_err().code(), "query_memory_limit");
        // A marker with no message is a bad response.
        let body = format!("{BODY}__exception__\n");
        assert_eq!(run(body.as_bytes(), 7, &l).unwrap_err().code(), "source_bad_response");
    }

    #[test]
    fn exception_code_reader() {
        assert_eq!(exception_code("Code: 241. DB::Exception: x"), Some(241));
        assert_eq!(exception_code("Code: 241 x"), None);
        assert_eq!(exception_code("Code: . x"), None);
        assert_eq!(exception_code("code: 1."), None);
    }

    #[test]
    fn a_value_error_never_shows_the_row() {
        let l = limits(100, 1 << 20);
        let e = run(b"[\"a\"]\n[\"UInt64\"]\n[\"not-a-number-secret\"]\n", 100, &l).unwrap_err();
        assert_eq!(e.code(), "source_bad_response");
        assert!(!e.message().contains("secret"));
    }
}
