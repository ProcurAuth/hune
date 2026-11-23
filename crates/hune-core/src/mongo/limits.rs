//! The limits of the MongoDB query parser (7.28). They are constants, not keys of the configuration.
//!
//! A text is valid on every instance. A breach gives `422 sql_invalid` at run time and
//! `422 invalid_document` at save, with the code `too_deep` or `too_large` of 4.37.

/// The depth of objects and arrays. The top object has depth 1.
pub const MAX_QUERY_DEPTH: usize = 32;
/// The values of the tree.
pub const MAX_QUERY_NODES: usize = 100_000;
/// The stages at all depths. A stage of a nested pipeline counts.
pub const MAX_QUERY_STAGES: usize = 64;
/// The levels of nested pipelines (`$facet`, `$lookup`, `$unionWith`).
pub const MAX_PIPELINE_DEPTH: usize = 3;
/// The size of one key, in bytes.
pub const MAX_QUERY_KEY_BYTES: usize = 256;
/// The size of one string value, in bytes (16 KiB).
pub const MAX_QUERY_STRING_BYTES: usize = 16_384;
/// The items of one array.
pub const MAX_QUERY_ARRAY_ITEMS: usize = 10_000;
/// The size of the pattern of one `$regex`, in bytes.
pub const MAX_REGEX_BYTES: usize = 1_024;

/// The keys of a `sort` object (7.29): 1 to this value.
pub const MAX_SORT_KEYS: usize = 32;
/// The largest value of `skip` and of `limit` in a `find` (7.29).
pub const MAX_SKIP_LIMIT: u64 = 1_000_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_the_table_of_7_28() {
        assert_eq!(MAX_QUERY_DEPTH, 32);
        assert_eq!(MAX_QUERY_NODES, 100_000);
        assert_eq!(MAX_QUERY_STAGES, 64);
        assert_eq!(MAX_PIPELINE_DEPTH, 3);
        assert_eq!(MAX_QUERY_KEY_BYTES, 256);
        assert_eq!(MAX_QUERY_STRING_BYTES, 16 * 1024);
        assert_eq!(MAX_QUERY_ARRAY_ITEMS, 10_000);
        assert_eq!(MAX_REGEX_BYTES, 1_024);
    }
}
