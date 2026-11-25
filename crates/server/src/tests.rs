use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header::{
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, HOST, LOCATION, ORIGIN, REFERRER_POLICY,
    SET_COOKIE, X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

use crate::{Assets, Bound, EVENTS_PATH, Events, SESSION_HEADER, Server, ServerError};

/// A server with one API route each way, and its session cookie.
fn bound() -> Bound {
    bound_with(Server::new(api()))
}

fn api() -> Router {
    Router::new()
        .route("/api/1/ping", get(|| async { "pong" }))
        .route("/api/1/echo", post(|body: String| async move { body }))
}

fn bound_with(server: Server) -> Bound {
    server.bind(None).unwrap()
}

fn host(bound: &Bound) -> String {
    format!("127.0.0.1:{}", bound.port())
}

/// The session cookie, as the browser sends it.
fn cookie(bound: &Bound) -> String {
    format!("totalfloat-{}={}", bound.port(), bound.shell.cookie_value)
}

/// The token in the sign-in URL, which the page sends back.
fn token(bound: &Bound) -> String {
    bound.url().split_once("?token=").unwrap().1.to_owned()
}

/// A request to `bound` with the right `Host`, signed in.
fn signed_in(bound: &Bound, method: &str, path: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header(HOST, host(bound))
        .header(COOKIE, cookie(bound))
        .header(SESSION_HEADER, token(bound))
}

async fn send(bound: &Bound, request: Request<Body>) -> Response {
    bound.app().oneshot(request).await.unwrap()
}

async fn text(response: Response) -> String {
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn error_code(response: Response) -> String {
    assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
    let doc: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(doc["schema"], "totalfloat.error/1");
    assert!(doc["error"]["hint"].is_string());
    doc["error"]["code"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_signed_in_api_call_reaches_the_route() {
    let bound = bound();
    let response = send(
        &bound,
        signed_in(&bound, "GET", "/api/1/ping")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(text(response).await, "pong");
}

#[tokio::test]
async fn an_api_call_without_the_session_gets_401() {
    let bound = bound();
    let port = bound.port();
    let token = token(&bound);
    let good = cookie(&bound);
    for (cookie, secret) in [
        (None, None),
        (None, Some(token.clone())),
        // The cookie alone: what another server on 127.0.0.1 would receive.
        (Some(good.clone()), None),
        (Some(good.clone()), Some("nope".to_owned())),
        (
            Some(format!("totalfloat-1={}", bound.shell.cookie_value)),
            Some(token.clone()),
        ),
        (Some(format!("totalfloat-{port}=nope")), Some(token.clone())),
        // The token is not the cookie's value.
        (
            Some(format!("totalfloat-{port}={token}")),
            Some(token.clone()),
        ),
    ] {
        let mut request = Request::get("/api/1/ping").header(HOST, host(&bound));
        if let Some(cookie) = &cookie {
            request = request.header(COOKIE, cookie);
        }
        if let Some(secret) = &secret {
            request = request.header(SESSION_HEADER, secret);
        }
        let response = send(&bound, request.body(Body::empty()).unwrap()).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{cookie:?} {secret:?}"
        );
        assert_eq!(error_code(response).await, "not-signed-in");
    }
    // The query form is only for the event stream.
    let request = Request::get(format!("/api/1/ping?session={token}"))
        .header(HOST, host(&bound))
        .header(COOKIE, good)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&bound, request).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // Unknown API paths are refused before anyone learns they don't exist.
    let response = send(
        &bound,
        Request::get("/api/1/nothing")
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_cookie_is_found_among_others() {
    let bound = bound();
    let cookies = format!("theme=dark; {}; other=1", cookie(&bound));
    let request = Request::get("/api/1/ping")
        .header(HOST, host(&bound))
        .header(COOKIE, cookies)
        .header(SESSION_HEADER, token(&bound))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&bound, request).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_wrong_host_gets_421() {
    let bound = bound();
    let port = bound.port();
    for host in [
        format!("evil.example:{port}"),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        "127.0.0.1".to_owned(),
        format!("127.0.0.2:{port}"),
    ] {
        let request = Request::get("/")
            .header(HOST, host)
            .body(Body::empty())
            .unwrap();
        let response = send(&bound, request).await;
        assert_eq!(response.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(error_code(response).await, "wrong-host");
    }
    let response = send(&bound, Request::get("/").body(Body::empty()).unwrap()).await;
    assert_eq!(
        response.status(),
        StatusCode::MISDIRECTED_REQUEST,
        "no Host at all"
    );
    let request = Request::get("/")
        .header(HOST, format!("LocalHost:{port}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&bound, request).await.status(),
        StatusCode::TEMPORARY_REDIRECT
    );
}

#[tokio::test]
async fn pages_on_localhost_move_to_127_0_0_1() {
    let bound = bound();
    let port = bound.port();
    let request = Request::get("/projects/alpha?view=grid")
        .header(HOST, format!("localhost:{port}"))
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers()[LOCATION],
        format!("http://127.0.0.1:{port}/projects/alpha?view=grid")
    );
    // API calls aren't redirected; they need the session as anywhere else.
    let request = signed_in(&bound, "GET", "/api/1/ping")
        .header(HOST, format!("localhost:{port}"))
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_foreign_origin_on_a_change_gets_403() {
    let bound = bound();
    let port = bound.port();
    for method in ["POST", "PUT", "PATCH", "DELETE", "PROPFIND"] {
        for origin in [
            "https://evil.example".to_owned(),
            "null".to_owned(),
            format!("https://127.0.0.1:{port}"),
            format!("http://127.0.0.1:{}", port.wrapping_add(1)),
        ] {
            let request = signed_in(&bound, method, "/api/1/echo")
                .header(ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            let response = send(&bound, request).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
            assert_eq!(error_code(response).await, "wrong-origin");
        }
    }
    for origin in [
        Some(format!("http://127.0.0.1:{port}")),
        Some(format!("http://localhost:{port}")),
        None,
    ] {
        let mut request = signed_in(&bound, "POST", "/api/1/echo");
        if let Some(origin) = origin {
            request = request.header(ORIGIN, origin);
        }
        let response = send(&bound, request.body(Body::from("hi")).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(text(response).await, "hi");
    }
    // A read from another origin isn't refused here; the session and the
    // absence of CORS headers keep its answer from that origin.
    let request = signed_in(&bound, "GET", "/api/1/ping")
        .header(ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .keys()
            .all(|name| !name.as_str().starts_with("access-control-"))
    );
}

#[tokio::test]
async fn the_token_sets_the_cookie_and_a_wrong_token_does_not() {
    let bound = bound();
    let path = bound.url().replace(&format!("http://{}", host(&bound)), "");
    let request = Request::get(&path)
        .header(HOST, host(&bound))
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers()[LOCATION],
        format!("/#session={}", token(&bound))
    );
    let set = response.headers()[SET_COOKIE].to_str().unwrap();
    assert_eq!(
        set,
        format!("{}; HttpOnly; SameSite=Strict; Path=/", cookie(&bound))
    );

    let token = path.split_once("?token=").unwrap().1;
    let mut wrong = token.to_owned();
    wrong.replace_range(..1, if token.starts_with('0') { "1" } else { "0" });
    for query in [
        format!("token={wrong}"),
        "token=".to_owned(),
        "tokenx=1&token=nope".to_owned(),
        format!("token={token}x"),
    ] {
        let request = Request::get(format!("/?{query}"))
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap();
        let response = send(&bound, request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{query}");
        assert!(response.headers().get(SET_COOKIE).is_none());
    }
}

#[test]
fn each_bind_makes_a_new_token() {
    let a = bound().url();
    let b = bound().url();
    let token = |url: &str| url.split_once("?token=").unwrap().1.to_owned();
    assert_ne!(token(&a), token(&b));
    assert_eq!(token(&a).len(), 64);
    assert!(
        token(&a)
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert!(a.starts_with("http://127.0.0.1:"));
}

#[tokio::test]
async fn every_response_has_the_security_headers() {
    let bound = bound();
    let requests = [
        Request::get("/")
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap(),
        Request::get("/")
            .header(HOST, "evil.example")
            .body(Body::empty())
            .unwrap(),
        Request::get("/api/1/ping")
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap(),
        signed_in(&bound, "GET", "/api/1/ping")
            .body(Body::empty())
            .unwrap(),
        signed_in(&bound, "GET", "/api/1/nothing")
            .body(Body::empty())
            .unwrap(),
        signed_in(&bound, "POST", "/api/1/echo")
            .header(ORIGIN, "null")
            .body(Body::empty())
            .unwrap(),
        Request::get("/?token=bad")
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap(),
    ];
    for request in requests {
        let response = send(&bound, request).await;
        let headers = response.headers();
        assert_eq!(
            headers[CONTENT_SECURITY_POLICY],
            "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; \
             connect-src 'self'; frame-ancestors 'none'"
        );
        assert_eq!(headers[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[REFERRER_POLICY], "no-referrer");
    }
}

#[tokio::test]
async fn an_unknown_api_path_is_a_json_404() {
    let bound = bound();
    let response = send(
        &bound,
        signed_in(&bound, "GET", "/api/1/nothing")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(error_code(response).await, "not-found");
}

#[tokio::test]
async fn a_wrong_method_on_an_api_route_is_a_json_405() {
    let bound = bound();
    let request = signed_in(&bound, "GET", "/api/1/echo")
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(error_code(response).await, "method-not-allowed");
}

#[tokio::test]
async fn a_caller_route_outside_api_still_needs_the_session() {
    let api = api().route("/export.xer", get(|| async { "schedule" }));
    let bound = bound_with(Server::new(api));
    let request = Request::get("/export.xer")
        .header(HOST, host(&bound))
        .header(COOKIE, cookie(&bound))
        .body(Body::empty())
        .unwrap();
    let response = send(&bound, request).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(response).await, "not-signed-in");
    let response = send(
        &bound,
        signed_in(&bound, "GET", "/export.xer")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(text(response).await, "schedule");
}

#[test]
fn a_server_without_api_routes_binds() {
    let bound = bound_with(Server::new(Router::new()));
    assert!(bound.port() > 0);
}

struct Built;

impl Assets for Built {
    fn get(&self, path: &str) -> Option<(&'static [u8], &'static str)> {
        match path {
            "index.html" => Some((b"<p>app</p>", "text/html; charset=utf-8")),
            "assets/app.js" => Some((b"run()", "text/javascript")),
            _ => None,
        }
    }
}

#[tokio::test]
async fn assets_are_served_and_an_unknown_path_serves_the_index() {
    let bound = bound_with(Server::new(api()).with_assets(Built));
    let get = |path: &str| {
        Request::get(path)
            .header(HOST, host(&bound))
            .body(Body::empty())
            .unwrap()
    };
    let response = send(&bound, get("/assets/app.js")).await;
    assert_eq!(response.headers()[CONTENT_TYPE], "text/javascript");
    assert_eq!(text(response).await, "run()");
    for path in [
        "/",
        "/index.html",
        "/projects/alpha/grid",
        "/projects/alpha.v2",
    ] {
        let response = send(&bound, get(path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(response.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(text(response).await, "<p>app</p>", "{path}");
    }
    // A missing script or style is a 404, not the index, which the browser
    // would refuse to run.
    for path in ["/assets/app-old123.js", "/assets/STYLE.CSS", "/favicon.ico"] {
        assert_eq!(
            send(&bound, get(path)).await.status(),
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let request = Request::post("/projects")
        .header(HOST, host(&bound))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&bound, request).await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn without_assets_pages_show_how_to_build_the_ui() {
    let bound = bound();
    for path in ["/", "/projects/alpha"] {
        let response = send(
            &bound,
            Request::get(path)
                .header(HOST, host(&bound))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_TYPE], "text/html; charset=utf-8");
        assert!(text(response).await.contains("cargo xtask ui"));
    }
}

#[tokio::test]
async fn events_stream_to_a_signed_in_page() {
    let events = Events::default();
    let bound = bound_with(Server::new(api()).with_events(events.clone()));
    let unsigned = Request::get(EVENTS_PATH)
        .header(HOST, host(&bound))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(&bound, unsigned).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let response = send(
        &bound,
        signed_in(&bound, "GET", EVENTS_PATH)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "text/event-stream");
    let mut body = response.into_body().into_data_stream();
    assert_eq!(
        events.send("changeset", "{\"id\":1}\nsecond line").unwrap(),
        1
    );
    let frame = body.next().await.unwrap().unwrap();
    assert_eq!(
        std::str::from_utf8(&frame).unwrap(),
        "event: changeset\ndata: {\"id\":1}\ndata: second line\n\n"
    );
    events.send("changeset", "a\r\nb\rc").unwrap();
    let frame = body.next().await.unwrap().unwrap();
    assert_eq!(
        std::str::from_utf8(&frame).unwrap(),
        "event: changeset\ndata: a\ndata: b\ndata: c\n\n"
    );
}

#[tokio::test]
async fn the_event_stream_takes_the_token_in_its_query() {
    let bound = bound();
    let token = token(&bound);
    let request = |query: &str| {
        Request::get(format!("{EVENTS_PATH}?{query}"))
            .header(HOST, host(&bound))
            .header(COOKIE, cookie(&bound))
            .body(Body::empty())
            .unwrap()
    };
    let response = send(&bound, request(&format!("session={token}"))).await;
    assert_eq!(response.status(), StatusCode::OK);
    for query in [
        "session=nope".to_owned(),
        format!("token={token}"),
        format!("xsession={token}"),
    ] {
        let response = send(&bound, request(&query)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{query}");
    }
}

#[tokio::test]
async fn a_page_that_falls_behind_is_told_to_resync() {
    let events = Events::new(2);
    let bound = bound_with(Server::new(api()).with_events(events.clone()));
    let response = send(
        &bound,
        signed_in(&bound, "GET", EVENTS_PATH)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let mut body = response.into_body().into_data_stream();
    for n in 0..5 {
        events.send("changeset", n.to_string()).unwrap();
    }
    let frame = body.next().await.unwrap().unwrap();
    assert_eq!(
        std::str::from_utf8(&frame).unwrap(),
        "event: resync\ndata: {\"missed\":3}\n\n"
    );
    let frame = body.next().await.unwrap().unwrap();
    assert_eq!(
        std::str::from_utf8(&frame).unwrap(),
        "event: changeset\ndata: 3\n\n"
    );
}

#[test]
fn an_event_kind_must_be_a_plain_name() {
    let events = Events::default();
    assert_eq!(
        events.send("branch.moved", "{}").unwrap(),
        0,
        "nobody listening is fine"
    );
    for kind in ["", "two words", "line\nbreak", "carriage\r", "é"] {
        assert!(
            matches!(events.send(kind, "{}"), Err(ServerError::EventKind(_))),
            "{kind:?}"
        );
    }
    assert!(matches!(
        events.send("changeset", ""),
        Err(ServerError::EventData(_))
    ));
}

#[test]
fn a_port_in_use_is_a_bind_error() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let error = Server::new(api()).bind(Some(port)).err().unwrap();
    assert!(matches!(error, ServerError::Bind { .. }), "{error}");
    assert!(error.to_string().contains(&format!("127.0.0.1:{port}")));
}

/// Write `request` to the server at `port` and read until `until` appears in
/// the response or the connection closes.
async fn exchange(port: u16, request: &str, until: &str) -> (tokio::net::TcpStream, String) {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut seen = Vec::new();
    let mut buffer = [0u8; 4096];
    while !String::from_utf8_lossy(&seen).contains(until) {
        let read = stream.read(&mut buffer).await.unwrap();
        if read == 0 {
            break;
        }
        seen.extend_from_slice(&buffer[..read]);
    }
    (stream, String::from_utf8(seen).unwrap())
}

#[tokio::test]
async fn a_real_port_answers_and_shuts_down_with_a_page_listening() {
    let bound = bound();
    let port = bound.port();
    let cookie = cookie(&bound);
    let token = token(&bound);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(bound.serve(async {
        let _ = stopped.await;
    }));

    let request = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let (_, response) = exchange(port, &request, "</html>").await;
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("x-content-type-options: nosniff")
    );
    assert!(response.contains("cargo xtask ui"));

    let request = format!(
        "GET {EVENTS_PATH}?session={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\r\n"
    );
    let (_listening, response) = exchange(port, &request, "\r\n\r\n").await;
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");

    stop.send(()).unwrap();
    let finished = tokio::time::timeout(Duration::from_secs(10), serving).await;
    finished
        .expect("serving stops though a page is still listening")
        .unwrap()
        .unwrap();
}
