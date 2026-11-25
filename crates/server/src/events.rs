//! Server-sent events to open pages.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{ServerError, Shell};

/// Where pages listen for events.
pub const EVENTS_PATH: &str = "/api/1/events";

/// The event a page gets when it fell too far behind and missed some: it
/// should fetch what it shows again. Its data is `{"missed":<count>}`.
pub const RESYNC: &str = "resync";

/// How often an idle stream sends a comment, so proxies and browsers keep it.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// One event: a kind, which becomes the SSE event name, and its data.
#[derive(Clone, Debug)]
struct Message {
    kind: String,
    data: String,
}

/// A broadcast channel to every open page. Clones send on the same channel.
#[derive(Clone, Debug)]
pub struct Events {
    sender: broadcast::Sender<Message>,
}

impl Default for Events {
    /// A channel that holds 256 events for a slow page.
    fn default() -> Self {
        Events::new(256)
    }
}

impl Events {
    /// A channel that holds up to `capacity` events (at least 1) for a page
    /// that is slow to read them. A page further behind gets [`RESYNC`].
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Events {
            sender: broadcast::channel(capacity.max(1)).0,
        }
    }

    /// Send an event of `kind` with `data`, usually JSON, to every open page.
    /// Line breaks in `data` arrive as `\n`, whether written `\n`, `\r\n`
    /// or `\r`. Returns how many pages it went to; none is fine.
    ///
    /// # Errors
    ///
    /// [`ServerError::EventKind`] if `kind` is empty or has anything but ASCII
    /// letters, digits, `-`, `.` and `_`, and [`ServerError::EventData`] if
    /// `data` is empty.
    pub fn send(&self, kind: &str, data: impl Into<String>) -> Result<usize, ServerError> {
        let good = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_');
        if kind.is_empty() || !kind.chars().all(good) {
            return Err(ServerError::EventKind(kind.to_owned()));
        }
        let mut data = data.into();
        if data.is_empty() {
            return Err(ServerError::EventData(kind.to_owned()));
        }
        if data.contains('\r') {
            data = data.replace("\r\n", "\n").replace('\r', "\n");
        }
        let message = Message {
            kind: kind.to_owned(),
            data,
        };
        Ok(self.sender.send(message).unwrap_or(0))
    }
}

/// `GET /api/1/events`: the channel as server-sent events, until the page goes
/// or the server stops.
pub(crate) async fn stream(
    State(shell): State<Arc<Shell>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = shell.events.sender.subscribe();
    let stop = shell.stop.subscribe();
    let events =
        futures_util::stream::unfold((receiver, stop), |(mut receiver, mut stop)| async move {
            if *stop.borrow_and_update() {
                return None;
            }
            let event = tokio::select! {
                next = receiver.recv() => match next {
                    Ok(message) => Event::default().event(message.kind).data(message.data),
                    Err(RecvError::Lagged(missed)) => {
                        Event::default().event(RESYNC).data(format!("{{\"missed\":{missed}}}"))
                    }
                    Err(RecvError::Closed) => return None,
                },
                _ = stop.changed() => return None,
            };
            Some((Ok(event), (receiver, stop)))
        });
    Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}
