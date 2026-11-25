//! The checks every request passes, and the headers every response carries.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, HOST, LOCATION, ORIGIN,
    REFERRER_POLICY, SET_COOKIE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::{EVENTS_PATH, SESSION_HEADER, Shell, error, is_api};

/// The content security policy: nothing from anywhere but the server itself,
/// no inline script or style, and no framing.
const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self'; \
                   script-src 'self'; connect-src 'self'; frame-ancestors 'none'";

/// Add the security headers to every response, refusals included.
pub(crate) async fn headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}

/// Refuse foreign hosts and origins and API calls without a session, send
/// pages on `localhost` to `127.0.0.1`, and swap the sign-in token for a
/// session.
pub(crate) async fn guard(
    State(shell): State<Arc<Shell>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let Some(name) = our_host(&shell, headers) else {
        return error(
            StatusCode::MISDIRECTED_REQUEST,
            "wrong-host",
            "This server only answers requests addressed to 127.0.0.1 or localhost.",
            "Open the URL `totalfloat serve` printed.",
        );
    };
    let path = request.uri().path();
    let reads = safe(request.method());
    if !reads && !origin_is_ours(&shell, headers) {
        return error(
            StatusCode::FORBIDDEN,
            "wrong-origin",
            "Changes are only accepted from pages this server served.",
            "Make the change from the TotalFloat page, not another site.",
        );
    }
    if is_api(path) {
        if !signed_in(&shell, &request) {
            return not_signed_in();
        }
    } else if reads && name != Name::Loopback {
        // Sessions live in the browser per origin, and the sign-in URL is on
        // 127.0.0.1, so pages are only served there.
        let target = request.uri().path_and_query().map_or("/", |p| p.as_str());
        let location = format!("http://127.0.0.1:{}{target}", shell.port);
        return (StatusCode::TEMPORARY_REDIRECT, [(LOCATION, location)]).into_response();
    }
    if path == "/"
        && request.method() == Method::GET
        && let Some(offered) = query_value(request.uri().query(), "token")
    {
        return sign_in(&shell, offered);
    }
    next.run(request).await
}

/// Refuse a request to one of the caller's routes without a session, wherever
/// the route is mounted.
pub(crate) async fn require_session(
    State(shell): State<Arc<Shell>>,
    request: Request,
    next: Next,
) -> Response {
    if signed_in(&shell, &request) {
        next.run(request).await
    } else {
        not_signed_in()
    }
}

fn not_signed_in() -> Response {
    error(
        StatusCode::UNAUTHORIZED,
        "not-signed-in",
        "This request has no session.",
        "Open the URL `totalfloat serve` printed to sign in.",
    )
}

/// Whether `method` only reads. Anything else may change something, so its
/// origin is checked.
fn safe(method: &Method) -> bool {
    [Method::GET, Method::HEAD, Method::OPTIONS].contains(method)
}

/// Which of its names a request used for this server.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Name {
    /// `127.0.0.1`.
    Loopback,
    /// `localhost`.
    Localhost,
}

/// The name in `Host`, if it is `127.0.0.1:<port>` or `localhost:<port>`. A
/// request without a `Host` is refused too.
fn our_host(shell: &Shell, headers: &HeaderMap) -> Option<Name> {
    let host = headers.get(HOST)?.to_str().ok()?;
    our_authority(shell, host)
}

/// The name in `authority`, if it is `127.0.0.1:<port>` or `localhost:<port>`.
fn our_authority(shell: &Shell, authority: &str) -> Option<Name> {
    let (name, port) = authority.rsplit_once(':')?;
    if port.parse() != Ok(shell.port) {
        return None;
    }
    if name == "127.0.0.1" {
        Some(Name::Loopback)
    } else if name.eq_ignore_ascii_case("localhost") {
        Some(Name::Localhost)
    } else {
        None
    }
}

/// Whether `Origin` is absent or this server's own. An origin of `null` is
/// someone else's.
fn origin_is_ours(shell: &Shell, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    origin
        .to_str()
        .ok()
        .and_then(|origin| origin.strip_prefix("http://"))
        .and_then(|authority| our_authority(shell, authority))
        .is_some()
}

/// Whether the request has a session: this run's cookie, and the session
/// secret in [`SESSION_HEADER`], or for the event stream, which a browser
/// opens without custom headers, in the `session` query parameter.
///
/// The cookie alone isn't enough, because browsers send `127.0.0.1` cookies
/// to every port, so any other server on the machine the browser visits gets
/// it. The secret lives in the page's own storage, which is per port.
fn signed_in(shell: &Shell, request: &Request) -> bool {
    let name = shell.cookie();
    let cookie = request
        .headers()
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(key, value)| key == name && same(value, &shell.cookie_value));
    let secret = match request.headers().get(SESSION_HEADER) {
        Some(value) => value.to_str().ok(),
        None if request.uri().path() == EVENTS_PATH => {
            query_value(request.uri().query(), "session")
        }
        None => None,
    };
    cookie && secret.is_some_and(|secret| same(secret, &shell.token))
}

/// The value of the query parameter `key`, if there is one.
fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?.split('&').find_map(|pair| {
        pair.split_once('=')
            .filter(|(name, _)| *name == key)
            .map(|(_, value)| value)
    })
}

/// Swap a good token for the session cookie, and send the page to
/// `/#session=<token>` to keep the secret; refuse a bad token.
fn sign_in(shell: &Shell, offered: &str) -> Response {
    let no_store = (CACHE_CONTROL, "no-store".to_owned());
    if !same(offered, &shell.token) {
        return (
            StatusCode::FORBIDDEN,
            [
                (CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
                no_store,
            ],
            "This sign-in link isn't for this server run. \
             Open the URL `totalfloat serve` printed most recently.\n",
        )
            .into_response();
    }
    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Strict; Path=/",
        shell.cookie(),
        shell.cookie_value
    );
    let location = format!("/#session={}", shell.token);
    (
        StatusCode::SEE_OTHER,
        [(SET_COOKIE, cookie), (LOCATION, location), no_store],
    )
        .into_response()
}

/// Compare secrets in time that doesn't depend on where they first differ.
fn same(offered: &str, secret: &str) -> bool {
    offered.len() == secret.len()
        && offered
            .bytes()
            .zip(secret.bytes())
            .fold(0u8, |differ, (a, b)| differ | (a ^ b))
            == 0
}
