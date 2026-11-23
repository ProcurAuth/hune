//! The parsed top object of a MongoDB query (7.27 to 7.29).
//!
//! The query text is one JSON object. It has the members `collection`, `database` and exactly one
//! of `aggregate` and `find`. The parser of 7.28 (next wave) makes a [`MongoQuery`] from the text.
//! The tree below the top object uses [`MongoValue`] with the eleven types of 7.28
//! ([`MongoValue::is_query_value`]). A typed leaf, such as a Date, is never an operator object.

use crate::mongo::value::{Document, MongoValue};

/// The members of the top object, in the order of the table of 7.29. An unknown member is an error.
pub const TOP_MEMBERS: [&str; 4] = ["collection", "database", "aggregate", "find"];

/// The members of the object `find`, in the order of the table of 7.29.
pub const FIND_MEMBERS: [&str; 5] = ["filter", "projection", "sort", "skip", "limit"];

/// A parsed query (7.29). The names are not checked against the lists of the block yet: 7.33 does it.
#[derive(Debug, Clone, PartialEq)]
pub struct MongoQuery {
    /// The collection. It MUST be in the key `collections` of the source (7.33).
    pub collection: String,
    /// The database. `None` means the first name of the key `databases` of the source.
    pub database: Option<String>,
    /// The pipeline or the find.
    pub body: QueryBody,
}

/// Exactly one of the members `aggregate` and `find` (7.29).
#[derive(Debug, Clone, PartialEq)]
pub enum QueryBody {
    /// A pipeline of 1 to 64 stages. Each stage is a document with one key (G6).
    Aggregate(Vec<MongoValue>),
    /// A find.
    Find(FindSpec),
}

impl QueryBody {
    /// Gives the name of the member of the top object: `aggregate` or `find`.
    pub fn member_name(&self) -> &'static str {
        match self {
            QueryBody::Aggregate(_) => "aggregate",
            QueryBody::Find(_) => "find",
        }
    }
}

/// The object `find` (7.29). Every member is optional in the text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FindSpec {
    /// A filter of the context Q (7.30). The default is the empty document.
    pub filter: Document,
    /// An object. Each value is `0`, `1`, `true` or `false`.
    pub projection: Option<Document>,
    /// An object with 1 to 32 keys. Each value is `1` or `-1`.
    pub sort: Option<Document>,
    /// An integer from 0 to 1,000,000,000. The default is 0.
    pub skip: u64,
    /// An integer from 1 to 1,000,000,000. `None` means that the limit of the result applies.
    pub limit: Option<u64>,
}

impl MongoQuery {
    /// Gives the whole query as one tree value, for the walks of the checks (the count of nodes,
    /// the depth, the refused names). The top object holds the members that the text had.
    pub fn to_value(&self) -> MongoValue {
        let mut top: Document = vec![("collection".to_owned(), MongoValue::String(self.collection.clone()))];
        if let Some(db) = &self.database {
            top.push(("database".to_owned(), MongoValue::String(db.clone())));
        }
        match &self.body {
            QueryBody::Aggregate(stages) => top.push(("aggregate".to_owned(), MongoValue::Array(stages.clone()))),
            QueryBody::Find(f) => {
                let mut find: Document = Vec::new();
                if !f.filter.is_empty() {
                    find.push(("filter".to_owned(), MongoValue::Document(f.filter.clone())));
                }
                if let Some(p) = &f.projection {
                    find.push(("projection".to_owned(), MongoValue::Document(p.clone())));
                }
                if let Some(s) = &f.sort {
                    find.push(("sort".to_owned(), MongoValue::Document(s.clone())));
                }
                if f.skip != 0 {
                    find.push(("skip".to_owned(), int(f.skip)));
                }
                if let Some(l) = f.limit {
                    find.push(("limit".to_owned(), int(l)));
                }
                top.push(("find".to_owned(), MongoValue::Document(find)));
            }
        }
        MongoValue::Document(top)
    }
}

fn int(n: u64) -> MongoValue {
    i32::try_from(n).map_or_else(|_| MongoValue::Int64(n as i64), MongoValue::Int32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_members_are_the_four_of_7_29() {
        assert_eq!(TOP_MEMBERS, ["collection", "database", "aggregate", "find"]);
        assert_eq!(FIND_MEMBERS.len(), 5);
    }

    #[test]
    fn aggregate_query_gives_a_tree() {
        let stage = MongoValue::document_from_extended_json(r#"{"$match":{"a":1}}"#).unwrap();
        let q = MongoQuery {
            collection: "node_load".to_owned(),
            database: None,
            body: QueryBody::Aggregate(vec![MongoValue::Document(stage)]),
        };
        assert_eq!(q.body.member_name(), "aggregate");
        let v = q.to_value();
        assert!(v.get("database").is_none());
        assert!(matches!(v.get("aggregate"), Some(MongoValue::Array(a)) if a.len() == 1));
        assert!(v.is_query_value());
        assert_eq!(v.depth(), 4);
    }

    #[test]
    fn find_query_gives_a_tree_with_the_members_that_are_set() {
        let q = MongoQuery {
            collection: "c".to_owned(),
            database: Some("d".to_owned()),
            body: QueryBody::Find(FindSpec { skip: 5_000_000_000 / 10, limit: Some(10), ..FindSpec::default() }),
        };
        let v = q.to_value();
        assert_eq!(v.get("database"), Some(&MongoValue::String("d".to_owned())));
        let find = v.get("find").unwrap();
        assert_eq!(find.get("skip"), Some(&MongoValue::Int32(500_000_000)));
        assert_eq!(find.get("limit"), Some(&MongoValue::Int32(10)));
        assert!(find.get("filter").is_none());
        assert_eq!(QueryBody::Find(FindSpec::default()).member_name(), "find");
    }
}
