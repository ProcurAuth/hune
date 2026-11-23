//! The router: the table of the routes of 6.9 and the match of a path (6.1, 6.2, 6.25).
//!
//! The router has no dependency. It maps a method and a path, relative to `server.base_path`,
//! to one of three results. The first is a route. The second is `405` with the list of methods.
//! The third is the region of a path that no route names (`404`).

use http::Method;

/// The identity of a route. The dispatcher of the handlers matches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteId {
    Session,
    FoldersList,
    FolderCreate,
    FolderRename,
    FolderDelete,
    DashboardsList,
    DashboardCreate,
    DashboardGet,
    DashboardSave,
    DashboardDelete,
    History,
    HistoryVersion,
    Restore,
    Move,
    PanelQuery,
    VariableOptions,
    AnnotationQuery,
    SourcesList,
    SourcePreview,
    WellKnownActions,
    WellKnownResources,
    PageList,
    PageDashboard,
    Static,
}

/// The kind of body that a route takes. It gives the size limit (6.27).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyClass {
    /// No body.
    None,
    /// A panel query, an options query or an annotation query: 256 KiB.
    Query,
    /// A preview: 512 KiB.
    Preview,
    /// A create or a save of a document: `server.max_request_bytes`.
    Doc,
    /// A restore, a move or a folder route: 16 KiB.
    Small,
}

/// The region of a path (6.9, 6.25). It tells what a path that no route names gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// A path under `api/`: JSON `404`.
    Api,
    /// A path under `static/`: JSON `404`.
    Static,
    /// A path under `.well-known/`: JSON `404`.
    WellKnown,
    /// Another path: the shell with the status `404`.
    Page,
}

impl Region {
    /// Gives the region of a relative path.
    pub fn of(rel: &str) -> Region {
        let first = rel.split('/').next().unwrap_or("");
        match first {
            "api" => Region::Api,
            "static" => Region::Static,
            ".well-known" => Region::WellKnown,
            _ => Region::Page,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seg {
    Lit(&'static str),
    Param(&'static str),
    /// The rest of the path, with `/` inside (the static files).
    Rest(&'static str),
}

use Seg::{Lit, Param, Rest};

/// The methods of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Get,
    Post,
    Put,
    Delete,
}

impl Verb {
    fn matches(self, m: &Method) -> bool {
        match self {
            Verb::Get => *m == Method::GET || *m == Method::HEAD,
            Verb::Post => *m == Method::POST,
            Verb::Put => *m == Method::PUT,
            Verb::Delete => *m == Method::DELETE,
        }
    }
}

/// One line of the route table.
#[derive(Debug)]
pub struct RouteSpec {
    /// The identity.
    pub id: RouteId,
    /// The method. A `GET` route also answers `HEAD` (6.2).
    pub verb: Verb,
    /// The label for the log and the metrics (14.7, 14.8): the method and the template.
    pub label: &'static str,
    pattern: &'static [Seg],
    /// Tells if the route changes the store (6.9): `server.editing` applies to it.
    pub write: bool,
    /// The body class.
    pub body: BodyClass,
    /// The query parameters that the route accepts. `None`: the route checks no parameter.
    pub query: Option<&'static [&'static str]>,
    /// Tells if the route needs `If-Match` (6.3).
    pub needs_if_match: bool,
    /// Tells if the route needs `Idempotency-Key` (6.3).
    pub needs_idempotency: bool,
}

const NO_PARAMS: &[&str] = &[];

// The table reads best with one argument for each column.
#[allow(clippy::too_many_arguments)]
const fn spec(
    id: RouteId,
    verb: Verb,
    label: &'static str,
    pattern: &'static [Seg],
    write: bool,
    body: BodyClass,
    query: Option<&'static [&'static str]>,
    needs_if_match: bool,
    needs_idempotency: bool,
) -> RouteSpec {
    RouteSpec { id, verb, label, pattern, write, body, query, needs_if_match, needs_idempotency }
}

use BodyClass as B;
use RouteId as R;
use Verb::{Delete, Get, Post, Put};

/// The route table of 6.9. Each line is one method on one path template.
pub static ROUTES: &[RouteSpec] = &[
    spec(
        R::Session,
        Get,
        "GET /api/session",
        &[Lit("api"), Lit("session")],
        false,
        B::None,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::FoldersList,
        Get,
        "GET /api/folders",
        &[Lit("api"), Lit("folders")],
        false,
        B::None,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::FolderCreate,
        Post,
        "POST /api/folders",
        &[Lit("api"), Lit("folders")],
        true,
        B::Small,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::FolderRename,
        Put,
        "PUT /api/folders/{id}",
        &[Lit("api"), Lit("folders"), Param("id")],
        true,
        B::Small,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::FolderDelete,
        Delete,
        "DELETE /api/folders/{id}",
        &[Lit("api"), Lit("folders"), Param("id")],
        true,
        B::None,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::DashboardsList,
        Get,
        "GET /api/dashboards",
        &[Lit("api"), Lit("dashboards")],
        false,
        B::None,
        Some(&["folder", "q", "tag", "limit", "after"]),
        false,
        false,
    ),
    spec(
        R::DashboardCreate,
        Post,
        "POST /api/dashboards",
        &[Lit("api"), Lit("dashboards")],
        true,
        B::Doc,
        Some(&["validate_only"]),
        false,
        true,
    ),
    spec(
        R::DashboardGet,
        Get,
        "GET /api/dashboards/{uid}",
        &[Lit("api"), Lit("dashboards"), Param("uid")],
        false,
        B::None,
        Some(&["download"]),
        false,
        false,
    ),
    spec(
        R::DashboardSave,
        Put,
        "PUT /api/dashboards/{uid}",
        &[Lit("api"), Lit("dashboards"), Param("uid")],
        true,
        B::Doc,
        Some(&["validate_only"]),
        true,
        true,
    ),
    spec(
        R::DashboardDelete,
        Delete,
        "DELETE /api/dashboards/{uid}",
        &[Lit("api"), Lit("dashboards"), Param("uid")],
        true,
        B::None,
        Some(NO_PARAMS),
        true,
        true,
    ),
    spec(
        R::History,
        Get,
        "GET /api/dashboards/{uid}/history",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("history")],
        false,
        B::None,
        Some(&["limit", "after"]),
        false,
        false,
    ),
    spec(
        R::HistoryVersion,
        Get,
        "GET /api/dashboards/{uid}/history/{n}",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("history"), Param("n")],
        false,
        B::None,
        Some(&["download"]),
        false,
        false,
    ),
    spec(
        R::Restore,
        Post,
        "POST /api/dashboards/{uid}/restore",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("restore")],
        true,
        B::Small,
        Some(NO_PARAMS),
        true,
        true,
    ),
    spec(
        R::Move,
        Post,
        "POST /api/dashboards/{uid}/move",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("move")],
        true,
        B::Small,
        Some(NO_PARAMS),
        true,
        true,
    ),
    spec(
        R::PanelQuery,
        Post,
        "POST /api/dashboards/{uid}/panels/{id}/query",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("panels"), Param("id"), Lit("query")],
        false,
        B::Query,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::VariableOptions,
        Post,
        "POST /api/dashboards/{uid}/variables/{name}/options",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("variables"), Param("name"), Lit("options")],
        false,
        B::Query,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::AnnotationQuery,
        Post,
        "POST /api/dashboards/{uid}/annotations/{index}/query",
        &[Lit("api"), Lit("dashboards"), Param("uid"), Lit("annotations"), Param("index"), Lit("query")],
        false,
        B::Query,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::SourcesList,
        Get,
        "GET /api/sources",
        &[Lit("api"), Lit("sources")],
        false,
        B::None,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::SourcePreview,
        Post,
        "POST /api/sources/{id}/preview",
        &[Lit("api"), Lit("sources"), Param("id"), Lit("preview")],
        true,
        B::Preview,
        Some(NO_PARAMS),
        false,
        false,
    ),
    spec(
        R::WellKnownActions,
        Get,
        "GET /.well-known/procurauth/actions",
        &[Lit(".well-known"), Lit("procurauth"), Lit("actions")],
        false,
        B::None,
        None,
        false,
        false,
    ),
    spec(
        R::WellKnownResources,
        Get,
        "GET /.well-known/procurauth/resources",
        &[Lit(".well-known"), Lit("procurauth"), Lit("resources")],
        false,
        B::None,
        None,
        false,
        false,
    ),
    spec(R::PageList, Get, "GET /", &[], false, B::None, None, false, false),
    spec(R::PageDashboard, Get, "GET /d/*", &[Lit("d"), Param("uid")], false, B::None, None, false, false),
    spec(
        R::PageDashboard,
        Get,
        "GET /d/*",
        &[Lit("d"), Param("uid"), Param("slug")],
        false,
        B::None,
        None,
        false,
        false,
    ),
    spec(
        R::Static,
        Get,
        "GET /static/*",
        &[Lit("static"), Param("build"), Rest("path")],
        false,
        B::None,
        None,
        false,
        false,
    ),
];

/// The values of the path parameters of a matched route.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Params(Vec<(&'static str, String)>);

impl Params {
    /// Gives the value of a parameter. The value is the raw text of the path: no percent decoding.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }
}

/// The result of the match of a request.
#[derive(Debug)]
pub enum Matched {
    /// A route.
    Route(&'static RouteSpec, Params),
    /// The path exists but not with this method (`405`). The value is the header `Allow`.
    MethodNotAllowed(String),
    /// No route names this path (`404`).
    NotFound(Region),
}

fn match_pattern(pattern: &[Seg], segments: &[&str]) -> Option<Params> {
    let mut params = Vec::new();
    let mut i = 0;
    for seg in pattern {
        match seg {
            Lit(text) => {
                if segments.get(i) != Some(text) {
                    return None;
                }
                i += 1;
            }
            Param(name) => {
                let value = segments.get(i).filter(|v| !v.is_empty())?;
                params.push((*name, (*value).to_owned()));
                i += 1;
            }
            Rest(name) => {
                if segments.get(i).is_none_or(|v| v.is_empty()) {
                    return None;
                }
                params.push((*name, segments[i..].join("/")));
                i = segments.len();
            }
        }
    }
    (i == segments.len()).then_some(Params(params))
}

fn split(rel: &str) -> Vec<&str> {
    if rel.is_empty() { Vec::new() } else { rel.split('/').collect() }
}

/// The header `Allow` for the verbs of a path, in a fixed order. A `GET` also lists `HEAD`.
fn allow_header(verbs: &[Verb]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for (verb, names) in
        [(Get, &["GET", "HEAD"][..]), (Post, &["POST"][..]), (Put, &["PUT"][..]), (Delete, &["DELETE"][..])]
    {
        if verbs.contains(&verb) {
            out.extend_from_slice(names);
        }
    }
    out.join(", ")
}

/// Matches a method and a path relative to the base path (6.1).
///
/// `OPTIONS` always gives `405` (6.2): the header `Allow` lists the methods of the path, or
/// `GET, HEAD` when no route names the path.
pub fn route(method: &Method, rel: &str) -> Matched {
    let segments = split(rel);
    let mut allowed: Vec<Verb> = Vec::new();
    let mut found: Option<(&'static RouteSpec, Params)> = None;
    for spec in ROUTES {
        if let Some(params) = match_pattern(spec.pattern, &segments) {
            if !allowed.contains(&spec.verb) {
                allowed.push(spec.verb);
            }
            if found.is_none() && spec.verb.matches(method) {
                found = Some((spec, params));
            }
        }
    }
    if *method == Method::OPTIONS {
        let allow = if allowed.is_empty() { "GET, HEAD".to_owned() } else { allow_header(&allowed) };
        return Matched::MethodNotAllowed(allow);
    }
    match found {
        Some((spec, params)) => Matched::Route(spec, params),
        None if !allowed.is_empty() => Matched::MethodNotAllowed(allow_header(&allowed)),
        None => Matched::NotFound(Region::of(rel)),
    }
}

/// Removes the base path from a request path. Gives `None` when the path does not start with it (6.1).
///
/// `base_path` is `/` or a text that starts and ends with `/` (13.8).
pub fn strip_base<'a>(base_path: &str, path: &'a str) -> Option<&'a str> {
    path.strip_prefix(base_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_of(method: &Method, rel: &str) -> Option<(RouteId, Params)> {
        match route(method, rel) {
            Matched::Route(s, p) => Some((s.id, p)),
            _ => None,
        }
    }

    #[test]
    fn route_table_has_the_routes_of_6_9() {
        let apis = ROUTES.iter().filter(|r| r.label.contains("/api/") || r.label.contains(".well-known")).count();
        assert_eq!(apis, 21);
        assert_eq!(ROUTES.len(), 25);
        for r in ROUTES {
            assert!(r.label.starts_with(match r.verb {
                Get => "GET ",
                Post => "POST ",
                Put => "PUT ",
                Delete => "DELETE ",
            }));
        }
    }

    #[test]
    fn write_routes_are_the_ones_of_6_9() {
        let writes: Vec<&str> = ROUTES.iter().filter(|r| r.write).map(|r| r.label).collect();
        assert_eq!(
            writes,
            [
                "POST /api/folders",
                "PUT /api/folders/{id}",
                "DELETE /api/folders/{id}",
                "POST /api/dashboards",
                "PUT /api/dashboards/{uid}",
                "DELETE /api/dashboards/{uid}",
                "POST /api/dashboards/{uid}/restore",
                "POST /api/dashboards/{uid}/move",
                "POST /api/sources/{id}/preview",
            ]
        );
    }

    #[test]
    fn patterns_and_params() {
        let (id, p) = id_of(&Method::GET, "api/dashboards/nodes/history/12").unwrap();
        assert_eq!(id, RouteId::HistoryVersion);
        assert_eq!(p.get("uid"), Some("nodes"));
        assert_eq!(p.get("n"), Some("12"));
        let (id, p) = id_of(&Method::POST, "api/dashboards/u/panels/4/query").unwrap();
        assert_eq!(id, RouteId::PanelQuery);
        assert_eq!((p.get("uid"), p.get("id")), (Some("u"), Some("4")));
        let (id, p) = id_of(&Method::POST, "api/dashboards/u/variables/env/options").unwrap();
        assert_eq!(id, RouteId::VariableOptions);
        assert_eq!(p.get("name"), Some("env"));
        let (id, p) = id_of(&Method::POST, "api/dashboards/u/annotations/3/query").unwrap();
        assert_eq!(id, RouteId::AnnotationQuery);
        assert_eq!((p.get("uid"), p.get("index")), (Some("u"), Some("3")));
        assert_eq!(id_of(&Method::GET, "").unwrap().0, RouteId::PageList);
        assert_eq!(id_of(&Method::GET, "d/abc").unwrap().0, RouteId::PageDashboard);
        let (id, p) = id_of(&Method::GET, "d/abc/my-slug").unwrap();
        assert_eq!((id, p.get("slug")), (RouteId::PageDashboard, Some("my-slug")));
        let (id, p) = id_of(&Method::GET, "static/0123456789abcdef/js/vendor/x.js").unwrap();
        assert_eq!(
            (id, p.get("build"), p.get("path")),
            (RouteId::Static, Some("0123456789abcdef"), Some("js/vendor/x.js"))
        );
    }

    #[test]
    fn head_follows_get() {
        assert_eq!(id_of(&Method::HEAD, "api/session").unwrap().0, RouteId::Session);
        assert_eq!(id_of(&Method::HEAD, "").unwrap().0, RouteId::PageList);
        assert!(matches!(route(&Method::HEAD, "api/sources/x/preview"), Matched::MethodNotAllowed(_)));
    }

    #[test]
    fn method_not_allowed_lists_allow() {
        let Matched::MethodNotAllowed(a) = route(&Method::DELETE, "api/session") else { panic!() };
        assert_eq!(a, "GET, HEAD");
        let Matched::MethodNotAllowed(a) = route(&Method::PATCH, "api/folders") else { panic!() };
        assert_eq!(a, "GET, HEAD, POST");
        let Matched::MethodNotAllowed(a) = route(&Method::POST, "api/dashboards/u") else { panic!() };
        assert_eq!(a, "GET, HEAD, PUT, DELETE");
    }

    #[test]
    fn preview_post_is_a_route_and_get_is_405() {
        assert_eq!(id_of(&Method::POST, "api/sources/s/preview").unwrap().0, RouteId::SourcePreview);
        let Matched::MethodNotAllowed(a) = route(&Method::GET, "api/sources/s/preview") else { panic!() };
        assert_eq!(a, "POST");
    }

    #[test]
    fn options_is_always_405() {
        for rel in ["api/session", "", "d/x", "api/unknown", "static/a/b", "nope", "api/dashboards/u"] {
            let Matched::MethodNotAllowed(a) = route(&Method::OPTIONS, rel) else { panic!("{rel}") };
            assert!(!a.is_empty());
        }
        let Matched::MethodNotAllowed(a) = route(&Method::OPTIONS, "api/dashboards") else { panic!() };
        assert_eq!(a, "GET, HEAD, POST");
    }

    #[test]
    fn unknown_paths_by_region() {
        let region = |rel: &str| match route(&Method::GET, rel) {
            Matched::NotFound(r) => r,
            other => panic!("{rel}: {other:?}"),
        };
        assert_eq!(region("api/nothing"), Region::Api);
        assert_eq!(region("api/dashboards/"), Region::Api);
        assert_eq!(region("api"), Region::Api);
        assert_eq!(region("static"), Region::Static);
        assert_eq!(region("static/abc"), Region::Static);
        assert_eq!(region(".well-known/other"), Region::WellKnown);
        assert_eq!(region("whatever"), Region::Page);
        assert_eq!(region("d/a/b/c"), Region::Page);
        assert_eq!(region("favicon.ico"), Region::Page);
        assert_eq!(region("apix"), Region::Page);
    }

    #[test]
    fn base_path_strip() {
        assert_eq!(strip_base("/", "/api/session"), Some("api/session"));
        assert_eq!(strip_base("/", "/"), Some(""));
        assert_eq!(strip_base("/dashboards/", "/dashboards/api/session"), Some("api/session"));
        assert_eq!(strip_base("/dashboards/", "/dashboards/"), Some(""));
        assert_eq!(strip_base("/dashboards/", "/dashboards"), None);
        assert_eq!(strip_base("/dashboards/", "/api/session"), None);
        assert_eq!(strip_base("/dashboards/", "/dashboardsx/api"), None);
    }

    #[test]
    fn if_match_and_idempotency_flags() {
        let by = |id: RouteId| ROUTES.iter().find(|r| r.id == id).unwrap();
        assert!(by(RouteId::DashboardSave).needs_if_match && by(RouteId::DashboardSave).needs_idempotency);
        assert!(by(RouteId::DashboardDelete).needs_if_match);
        assert!(!by(RouteId::DashboardCreate).needs_if_match && by(RouteId::DashboardCreate).needs_idempotency);
        assert!(by(RouteId::Restore).needs_if_match && by(RouteId::Move).needs_idempotency);
        assert!(!by(RouteId::FolderCreate).needs_idempotency);
    }
}
