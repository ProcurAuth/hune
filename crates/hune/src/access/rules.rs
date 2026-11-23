//! The access rule of each route (11.21, 11.29).
//!
//! The table has one line for each route of the router. A route without a line answers
//! `403 forbidden` with the log line `access.no_rule`. A test of the server compares this table
//! with the route table of section 6.

use crate::server::route::RouteId;

/// What a route needs in the mode `authzen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// No token and no question: the action document (11.29).
    NoToken,
    /// A valid token and no question: the pages, the static files and the session (11.29).
    TokenOnly,
    /// A valid token and the questions of 11.21. The handler asks them.
    Questions,
}

/// The rule of a route, or `None` for a route with no line in 11.21.
pub fn rule_of(id: RouteId) -> Option<Rule> {
    Some(match id {
        RouteId::WellKnownActions => Rule::NoToken,
        RouteId::Session | RouteId::PageList | RouteId::PageDashboard | RouteId::Static => Rule::TokenOnly,
        RouteId::FoldersList
        | RouteId::FolderCreate
        | RouteId::FolderRename
        | RouteId::FolderDelete
        | RouteId::DashboardsList
        | RouteId::DashboardCreate
        | RouteId::DashboardGet
        | RouteId::DashboardSave
        | RouteId::DashboardDelete
        | RouteId::History
        | RouteId::HistoryVersion
        | RouteId::Restore
        | RouteId::Move
        | RouteId::PanelQuery
        | RouteId::VariableOptions
        | RouteId::AnnotationQuery
        | RouteId::SourcesList
        | RouteId::SourcePreview
        | RouteId::WellKnownResources => Rule::Questions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::route::ROUTES;

    #[test]
    fn each_route_of_the_router_has_a_line() {
        for spec in ROUTES {
            assert!(rule_of(spec.id).is_some(), "{} has no access rule", spec.label);
        }
    }

    #[test]
    fn only_the_action_document_needs_no_token() {
        let no_token: Vec<&str> =
            ROUTES.iter().filter(|s| rule_of(s.id) == Some(Rule::NoToken)).map(|s| s.label).collect();
        assert_eq!(no_token, ["GET /.well-known/procurauth/actions"]);
    }
}
