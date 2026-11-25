//! The HTTP shell for TotalFloat's local app (ARCHITECTURE.md, Local app).
//!
//! This crate knows nothing about schedules. It listens on `127.0.0.1`, signs
//! the browser in with a token made for each run, refuses requests from other
//! hosts and origins, serves the built UI, and streams [`Events`] to open
//! pages. The caller supplies the `/api/1/...` routes as an [`axum::Router`],
//! and the shell wraps them in its checks:
//!
//! - **Session.** Each [`Server::bind`] makes a random 32-byte token, valid
//!   until the server stops. [`Bound::url`] carries it. Opening that URL sets a
//!   cookie that is `HttpOnly` and `SameSite=Strict`, holding a second random
//!   value, and redirects to `/#session=<token>`. The page keeps the token in
//!   its own storage and sends it in [`SESSION_HEADER`] (the event stream,
//!   opened without custom headers, takes `?session=<token>` instead). Every
//!   `/api/` request and every caller route needs both the cookie and the
//!   token, or it gets 401 with a `totalfloat.error/1` body, code
//!   `not-signed-in`. The cookie alone isn't enough, because browsers send
//!   `127.0.0.1` cookies to every port, so any other local server the browser
//!   visits receives it; the page's storage is kept per port.
//! - **Host check.** A request whose `Host` isn't `127.0.0.1:<port>` or
//!   `localhost:<port>` gets 421, which stops DNS rebinding. Pages asked for on
//!   `localhost` redirect to `127.0.0.1`, where the session lives.
//! - **Origin check.** A request other than `GET`, `HEAD` or `OPTIONS` whose
//!   `Origin` is present and isn't the server's own gets 403. No CORS headers
//!   are sent.
//! - **Headers.** Every response carries a strict `Content-Security-Policy`,
//!   `X-Content-Type-Options: nosniff` and `Referrer-Policy: no-referrer`.

#![forbid(unsafe_code)]

mod assets;
mod checks;
mod events;
#[cfg(test)]
mod tests;

use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::watch;

pub use assets::Assets;
pub use events::{EVENTS_PATH, Events, RESYNC};

/// The request header that carries the session token from the page.
pub const SESSION_HEADER: &str = "x-totalfloat-session";

/// Something went wrong binding or serving.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The address couldn't be bound, usually because the port is in use.
    #[error("couldn't listen on {address}: {source}")]
    Bind {
        /// The address that was asked for.
        address: SocketAddr,
        /// Why it failed.
        source: std::io::Error,
    },
    /// The operating system couldn't supply random bytes for the session token.
    #[error("couldn't make a session token: {0}")]
    Token(String),
    /// Serving failed after the address was bound.
    #[error("the server stopped: {0}")]
    Serve(std::io::Error),
    /// An event kind that can't be sent as a server-sent event name.
    #[error("event kind {0:?} must be a non-empty run of ASCII letters, digits, '-', '.' and '_'")]
    EventKind(String),
    /// Empty event data, which browsers drop without telling the page.
    #[error("event {0:?} has no data; browsers drop such events")]
    EventData(String),
}

/// The local app's server, before it listens.
///
/// ```no_run
/// # async fn run() -> Result<(), totalfloat_server::ServerError> {
/// use totalfloat_server::Server;
///
/// let api = axum::Router::new().route("/api/1/ping", axum::routing::get(|| async { "pong" }));
/// let bound = Server::new(api).bind(None)?;
/// println!("Open {}", bound.url());
/// bound.serve(std::future::pending()).await
/// # }
/// ```
pub struct Server {
    api: Router,
    assets: Option<Arc<dyn Assets>>,
    events: Events,
}

impl Server {
    /// A server for `api`, the caller's `/api/1/...` routes. Each of them
    /// needs a session, wherever it is mounted.
    ///
    /// The routes must not set a fallback or use [`EVENTS_PATH`]: the shell
    /// owns both.
    #[must_use]
    pub fn new(api: Router) -> Self {
        Server {
            api,
            assets: None,
            events: Events::default(),
        }
    }

    /// Serve the built UI from `assets`. Without them, `/` shows a page saying
    /// how to build it.
    #[must_use]
    pub fn with_assets(mut self, assets: impl Assets) -> Self {
        self.assets = Some(Arc::new(assets));
        self
    }

    /// Stream `events` at [`EVENTS_PATH`]. Keep a clone to send on.
    #[must_use]
    pub fn with_events(mut self, events: Events) -> Self {
        self.events = events;
        self
    }

    /// Listen on `127.0.0.1:<port>`, or on a free port when `port` is `None`,
    /// and make this run's session token. Nothing is served until
    /// [`Bound::serve`].
    ///
    /// # Errors
    ///
    /// [`ServerError::Bind`] if the port can't be bound, and
    /// [`ServerError::Token`] if no random bytes are available.
    ///
    /// # Panics
    ///
    /// If the API router has a fallback or a route at [`EVENTS_PATH`], since
    /// those clash with the shell's own.
    pub fn bind(self, port: Option<u16>) -> Result<Bound, ServerError> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port.unwrap_or(0)));
        let bind_error = |source| ServerError::Bind { address, source };
        let listener = TcpListener::bind(address).map_err(bind_error)?;
        listener.set_nonblocking(true).map_err(bind_error)?;
        let port = listener.local_addr().map_err(bind_error)?.port();
        let (stop, _) = watch::channel(false);
        let shell = Arc::new(Shell {
            port,
            token: token()?,
            cookie_value: token()?,
            assets: self.assets,
            events: self.events,
            stop,
        });
        let mut api = self.api;
        if api.has_routes() {
            api = api.route_layer(from_fn_with_state(
                Arc::clone(&shell),
                checks::require_session,
            ));
        }
        let own = Router::new()
            .route(EVENTS_PATH, get(events::stream))
            .fallback(assets::fallback)
            .with_state(Arc::clone(&shell));
        let app = own
            .merge(api)
            .method_not_allowed_fallback(method_not_allowed)
            .layer(from_fn_with_state(Arc::clone(&shell), checks::guard))
            .layer(from_fn(checks::headers));
        Ok(Bound {
            listener,
            shell,
            app,
        })
    }
}

/// A server listening on its port, with its session token.
pub struct Bound {
    listener: TcpListener,
    shell: Arc<Shell>,
    app: Router,
}

impl Bound {
    /// The port it listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.shell.port
    }

    /// The URL to open: `http://127.0.0.1:<port>/?token=<token>`. Opening it
    /// signs the browser in. Treat it as a password.
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/?token={}",
            self.shell.port, self.shell.token
        )
    }

    /// The whole app, checks included, for driving without a socket in tests.
    pub fn app(&self) -> Router {
        self.app.clone()
    }

    /// Serve until `shutdown` completes. Event streams end when it does, so
    /// open pages don't hold the shutdown up.
    ///
    /// # Errors
    ///
    /// [`ServerError::Serve`] if the listener fails.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), ServerError> {
        let listener =
            tokio::net::TcpListener::from_std(self.listener).map_err(ServerError::Serve)?;
        let shell = self.shell;
        axum::serve(listener, self.app)
            .with_graceful_shutdown(async move {
                shutdown.await;
                shell.stop.send_replace(true);
            })
            .await
            .map_err(ServerError::Serve)
    }
}

/// What the checks and the shell's own routes need.
struct Shell {
    port: u16,
    /// In the sign-in URL, and then sent by the page with each API call. 64
    /// lowercase hex digits.
    token: String,
    /// The session cookie's value, a separate secret from `token`.
    cookie_value: String,
    assets: Option<Arc<dyn Assets>>,
    events: Events,
    /// Becomes `true` when serving stops, ending event streams.
    stop: watch::Sender<bool>,
}

impl Shell {
    /// The session cookie's name. Cookies don't keep ports apart, so the port
    /// is in the name: two runs on different ports keep separate sessions.
    fn cookie(&self) -> String {
        format!("totalfloat-{}", self.port)
    }
}

/// A fresh session token: 32 random bytes as hex.
fn token() -> Result<String, ServerError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| ServerError::Token(error.to_string()))?;
    Ok(bytes
        .iter()
        .flat_map(|byte| [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]])
        .map(char::from)
        .collect())
}

/// A `totalfloat.error/1` response, for the shell's refusals and the caller's
/// routes alike.
pub fn error(status: StatusCode, code: &str, message: &str, hint: &str) -> Response {
    let body = serde_json::json!({
        "schema": "totalfloat.error/1",
        "error": { "code": code, "message": message, "hint": hint },
    });
    (
        status,
        [(CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// A known path asked for with a method it doesn't take.
async fn method_not_allowed(request: Request) -> Response {
    let path = request.uri().path();
    if is_api(path) {
        error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method-not-allowed",
            &format!("{path} doesn't take {}.", request.method()),
            "Check the method for this API route.",
        )
    } else {
        StatusCode::METHOD_NOT_ALLOWED.into_response()
    }
}

/// Whether `path` is under `/api/`.
fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}
