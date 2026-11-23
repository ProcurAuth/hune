// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `lexer_pg` (16.16, 7.20, 7.25).
//!
//! Property: for any text, the lexer of the dialect `postgres` does not panic. Its tokens cover the
//! text with no gap and no overlap, and each token has at least one byte, ends on a character
//! boundary and has a start that is the end of the token before. `lex_for` agrees with `lex_all_for`:
//! it gives an error exactly when one token has a fault, and it gives the same tokens when there is
//! none. The content range of a string or a quoted name is inside the token and on character
//! boundaries. The statement check of 7.25 does not panic on the tokens of a text that lexes well,
//! and it refuses a text that has a token that does not close (UnclosedToken). The lexer of `clickhouse` stays total
//! on the same text.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::sql::lexer::token_fault;
use hune_core::sql::statement::check_statement_for;
use hune_core::sql::{Dialect, lex_all_for, lex_for};

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let tokens = lex_all_for(Dialect::Postgres, src);
    let mut pos = 0usize;
    for t in &tokens {
        assert_eq!(t.start, pos, "gap or overlap before the token at byte {}", t.start);
        assert!(t.end > t.start, "empty token at byte {}", t.start);
        assert!(src.is_char_boundary(t.end), "token ends inside a character at byte {}", t.end);
        let _ = t.text(src);
        if let Some((from, to)) = t.content_range(src) {
            assert!(t.start <= from && from <= to && to <= t.end, "content range outside the token at byte {}", t.start);
            assert!(src.get(from..to).is_some(), "content range inside a character at byte {}", t.start);
        }
        pos = t.end;
    }
    assert_eq!(pos, src.len(), "the tokens stop before the end of the text");

    let first_fault = tokens.iter().find_map(token_fault);
    match lex_for(Dialect::Postgres, src) {
        Ok(checked) => {
            assert!(first_fault.is_none(), "lex_for accepted a text that lex_all_for marks as faulty");
            assert_eq!(checked, tokens);
            // The statement check reads only tokens of a text that lexes well. It must not panic.
            let _ = check_statement_for(Dialect::Postgres, src, &tokens);
        }
        Err(e) => {
            assert_eq!(Some(e.kind), first_fault, "lex_for and lex_all_for disagree on the first fault");
            let _ = (e.position.line, e.position.column);
            if tokens.iter().any(|t| !t.closed) {
                assert!(
                    check_statement_for(Dialect::Postgres, src, &tokens).is_err(),
                    "the statement check accepted a text with a token that does not close"
                );
            }
        }
    }

    // The other dialect stays total on the same text.
    let other = lex_all_for(Dialect::ClickHouse, src);
    assert_eq!(other.last().map_or(0, |t| t.end), src.len());
});
