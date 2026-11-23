//! The resource tree of 11.15: `service / folder / dashboard` and `service / source`.
//!
//! Each node has its chain of ancestors (11.16). The chain comes from the catalog of Hune, never
//! from a value of the request (P 4.5).

use procurauth_pep::operation::{NodePath, node};

/// Builds the nodes of one service.
#[derive(Debug, Clone)]
pub struct Tree {
    service: String,
}

impl Tree {
    /// The tree of the service `service_id` (11.14).
    pub fn new(service_id: &str) -> Tree {
        Tree { service: service_id.to_owned() }
    }

    /// The service id.
    pub fn service_id(&self) -> &str {
        &self.service
    }

    /// The node of the service.
    pub fn service(&self) -> NodePath {
        NodePath::service(self.service.clone())
    }

    /// `service / folder:<id>`. An id that breaks the path rules gives the service node: the callers
    /// build a node only from an id that the catalog holds.
    pub fn folder(&self, id: &str) -> NodePath {
        node(&self.service, &[("folder", id)]).unwrap_or_else(|_| self.service())
    }

    /// `service / folder:<folder> / dashboard:<uid>`.
    pub fn dashboard(&self, folder: &str, uid: &str) -> NodePath {
        node(&self.service, &[("folder", folder), ("dashboard", uid)]).unwrap_or_else(|_| self.service())
    }

    /// `service / source:<id>`.
    pub fn source(&self, id: &str) -> NodePath {
        node(&self.service, &[("source", id)]).unwrap_or_else(|_| self.service())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::node_text;

    #[test]
    fn the_four_forms_of_11_16() {
        let t = Tree::new("dashboards");
        assert_eq!(node_text(&t.service()), "service:dashboards");
        assert_eq!(node_text(&t.folder("transit")), "service:dashboards/folder:transit");
        assert_eq!(node_text(&t.dashboard("transit", "score")), "service:dashboards/folder:transit/dashboard:score");
        assert_eq!(node_text(&t.source("prod")), "service:dashboards/source:prod");
    }
}
