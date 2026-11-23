//! Hune core: the document format 1, the ids, the canonical JSON, the etag, the limits,
//! the ClickHouse dialect (lexer, macros, variables, escape rules, interval rule) and the
//! frame types.
//!
//! This crate does no I/O (rule R1 of 3.4). It does not read files, the network, the
//! environment, a clock or a random source. Time values and random bytes are arguments.
#![forbid(unsafe_code)]

pub mod annotation;
pub mod document;
pub mod error;
pub mod etag;
pub mod frame;
pub mod ids;
pub mod limits;
pub mod mongo;
pub mod report;
pub mod sql;
pub mod time;
