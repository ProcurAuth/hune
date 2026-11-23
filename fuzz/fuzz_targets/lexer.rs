// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `lexer` (16.16).
//!
//! Property: for any text, `lex_all` does not panic. Its tokens cover the text with no gap and
//! no overlap, and each token has at least one byte, ends on a character boundary and has a
//! start that is the end of the token before. `lex` agrees with `lex_all`: it gives an error
//! exactly when one token has a fault, and it gives the same tokens when there is none.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::sql::lexer::token_fault;
use hune_core::sql::{lex, lex_all};

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let tokens = lex_all(src);
    let mut pos = 0usize;
    for t in &tokens {
        assert_eq!(t.start, pos, "gap or overlap before the token at byte {}", t.start);
        assert!(t.end > t.start, "empty token at byte {}", t.start);
        assert!(src.is_char_boundary(t.end), "token ends inside a character at byte {}", t.end);
        let _ = t.text(src);
        pos = t.end;
    }
    assert_eq!(pos, src.len(), "the tokens stop before the end of the text");

    let first_fault = tokens.iter().find_map(token_fault);
    match lex(src) {
        Ok(checked) => {
            assert!(first_fault.is_none(), "lex accepted a text that lex_all marks as faulty");
            assert_eq!(checked, tokens);
        }
        Err(e) => {
            assert_eq!(Some(e.kind), first_fault, "lex and lex_all disagree on the first fault");
            // The position of the error must be usable.
            let _ = (e.position.line, e.position.column);
        }
    }
});
