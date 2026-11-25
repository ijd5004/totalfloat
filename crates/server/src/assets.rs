//! The built UI, and what `/` shows without it.

use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{ALLOW, CONTENT_TYPE};
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::{Shell, error, is_api};

/// The built UI's files, usually embedded in the binary.
pub trait Assets: Send + Sync + 'static {
    /// The bytes and content type of the file at `path`, which has no leading
    /// slash (`index.html`, `assets/app.js`), or `None` if there's no such file.
    fn get(&self, path: &str) -> Option<(&'static [u8], &'static str)>;
}

/// Shown at every page path when the binary was built without the UI.
const PLACEHOLDER: &str = "<!doctype html>
<html lang=\"en\">
<head><meta charset=\"utf-8\"><title>TotalFloat</title></head>
<body>
<h1>TotalFloat</h1>
<p>The server is running, but this build has no web interface.</p>
<p>Build the interface with <code>cargo xtask ui</code>, then build TotalFloat again.</p>
</body>
</html>
";

/// File types that are never a page, so a missing one is a 404 rather than
/// `index.html`, which a browser would refuse to run as script or style.
const FILES: [&str; 12] = [
    "js", "mjs", "css", "map", "json", "wasm", "png", "svg", "ico", "woff", "woff2", "txt",
];

/// Every request no route took: a 404 under `/api/`, otherwise the asset at the
/// path, or `index.html` so client-side routes load the app. A missing file of
/// a type in [`FILES`] is a 404.
pub(crate) async fn fallback(
    State(shell): State<Arc<Shell>>,
    method: Method,
    uri: Uri,
) -> Response {
    let path = uri.path();
    if is_api(path) {
        return error(
            StatusCode::NOT_FOUND,
            "not-found",
            &format!("There's no API route at {path}."),
            "API routes start with /api/1/.",
        );
    }
    if method != Method::GET && method != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, [(ALLOW, "GET, HEAD")]).into_response();
    }
    let file = match path.trim_start_matches('/') {
        "" => "index.html",
        file => file,
    };
    let Some(assets) = &shell.assets else {
        return placeholder();
    };
    if let Some((bytes, content_type)) = assets.get(file) {
        return ([(CONTENT_TYPE, content_type)], bytes).into_response();
    }
    let extension = file
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'));
    if extension
        .is_some_and(|(_, extension)| FILES.contains(&extension.to_ascii_lowercase().as_str()))
    {
        return (
            StatusCode::NOT_FOUND,
            [(CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Not found.\n",
        )
            .into_response();
    }
    let found = assets.get("index.html");
    match found {
        Some((bytes, content_type)) => ([(CONTENT_TYPE, content_type)], bytes).into_response(),
        None => placeholder(),
    }
}

fn placeholder() -> Response {
    ([(CONTENT_TYPE, "text/html; charset=utf-8")], PLACEHOLDER).into_response()
}
