//! Read access to a Grafana JSON value with its JSON pointer. The walk of an object reports
//! each member that no row of 15.6 names.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::item::{Ctx, escape_key};

/// A value of the input and its JSON pointer.
#[derive(Debug, Clone)]
pub(crate) struct Node<'a> {
    pub v: &'a Value,
    pub ptr: String,
}

impl<'a> Node<'a> {
    pub(crate) fn root(v: &'a Value, ptr: &str) -> Node<'a> {
        Node { v, ptr: ptr.to_owned() }
    }

    /// The member `key` of an object.
    pub(crate) fn get(&self, key: &str) -> Option<Node<'a>> {
        self.v.get(key).map(|v| Node { v, ptr: format!("{}/{}", self.ptr, escape_key(key)) })
    }

    /// The item `i` of an array.
    pub(crate) fn at(&self, i: usize) -> Option<Node<'a>> {
        self.v.get(i).map(|v| Node { v, ptr: format!("{}/{i}", self.ptr) })
    }

    /// The pointer of a member that may be absent.
    pub(crate) fn child_ptr(&self, key: &str) -> String {
        format!("{}/{}", self.ptr, escape_key(key))
    }

    /// The items of an array. A value that is not an array has no item.
    pub(crate) fn items(&self) -> Vec<Node<'a>> {
        match self.v.as_array() {
            Some(a) => a.iter().enumerate().map(|(i, v)| Node { v, ptr: format!("{}/{i}", self.ptr) }).collect(),
            None => Vec::new(),
        }
    }

    pub(crate) fn str(&self) -> Option<&'a str> {
        self.v.as_str()
    }

    pub(crate) fn is_null(&self) -> bool {
        self.v.is_null()
    }
}

/// The walk of one object. It records the members that the code reads. `finish` reports each
/// member that the code did not read.
pub(crate) struct Walk<'a> {
    pub node: Node<'a>,
    map: &'a Map<String, Value>,
    seen: BTreeSet<&'a str>,
}

impl<'a> Walk<'a> {
    /// Starts the walk. A value that is not an object gives `None`.
    pub(crate) fn new(node: &Node<'a>) -> Option<Walk<'a>> {
        let map = node.v.as_object()?;
        Some(Walk { node: node.clone(), map, seen: BTreeSet::new() })
    }

    /// Reads a member and marks it as read.
    pub(crate) fn take(&mut self, key: &str) -> Option<Node<'a>> {
        let (k, v) = self.map.get_key_value(key)?;
        self.seen.insert(k.as_str());
        Some(Node { v, ptr: self.node.child_ptr(key) })
    }

    /// Marks members as metadata: they have no behaviour in Hune.
    pub(crate) fn meta(&mut self, ctx: &mut Ctx, keys: &[&str]) {
        for key in keys {
            if self.take(key).is_some() {
                ctx.meta();
            }
        }
    }

    /// Gives each member that the code did not read, with its key.
    pub(crate) fn unread(&self) -> Vec<Node<'a>> {
        self.map
            .iter()
            .filter(|(key, _)| !self.seen.contains(key.as_str()))
            .map(|(key, v)| Node { v, ptr: self.node.child_ptr(key) })
            .collect()
    }

    /// Reports each member that the code did not read.
    pub(crate) fn finish(self, ctx: &mut Ctx) {
        for (key, value) in self.map {
            if !self.seen.contains(key.as_str()) {
                ctx.unknown_member(&self.node.child_ptr(key), value);
            }
        }
    }
}
