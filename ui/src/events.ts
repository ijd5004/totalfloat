// The live change feed: `/api/1/events` as server-sent events
// (`totalfloat.event/1`), reconnected with backoff when it drops.

import type { ServerEvent } from "./api";

export const EVENT_KINDS = ["changeset", "proposal", "branch", "view"] as const;
/** Sent instead of events a slow page missed: fetch everything again. */
export const RESYNC = "resync";

/** `signed-out`: the server refuses this page's session, so retrying won't help. */
export type Connection = "connecting" | "live" | "disconnected" | "signed-out";

/** Reads one event's data, or `null` if it isn't a `totalfloat.event/1` document. */
export function parseEvent(data: string): ServerEvent | null {
  try {
    const event: unknown = JSON.parse(data);
    if (
      typeof event === "object" &&
      event !== null &&
      (event as { schema?: unknown }).schema === "totalfloat.event/1" &&
      (EVENT_KINDS as readonly unknown[]).includes((event as { event?: unknown }).event)
    ) {
      return event as ServerEvent;
    }
  } catch {
    // Not JSON.
  }
  return null;
}

/** The delay before reconnect attempt `attempt` (from 0): 0.5 s doubling to 30 s. */
export function backoff(attempt: number): number {
  return Math.min(30_000, 500 * 2 ** Math.min(attempt, 10));
}

export interface FeedHandlers {
  onEvent: (event: ServerEvent) => void;
  /** The page missed events, or reconnected after a gap: fetch what it shows again. */
  onResync: () => void;
  onConnection: (state: Connection) => void;
  /**
   * Asked after a drop, since a stream can't tell a stopped server from a
   * refused session: `false` means signed out, and the feed stops.
   */
  stillSignedIn?: () => Promise<boolean>;
}

/** What the feed needs of the browser, so tests can stand in for it. */
export interface FeedHost {
  open: (url: string) => EventSource;
  setTimeout: (run: () => void, ms: number) => number;
  clearTimeout: (id: number) => void;
}

const browser: FeedHost = {
  open: (url) => new EventSource(url),
  setTimeout: (run, ms) => window.setTimeout(run, ms),
  clearTimeout: (id) => window.clearTimeout(id),
};

/**
 * Follows the event stream until `close`. The browser's own reconnect is
 * replaced with backoff, so a stopped server isn't hammered, and each
 * reconnect after a drop asks for a resync, since events sent meanwhile are
 * gone.
 */
export class EventFeed {
  private source: EventSource | null = null;
  private timer: number | null = null;
  private attempt = 0;
  private dropped = false;
  private closed = false;

  constructor(
    private readonly session: string | null,
    private readonly handlers: FeedHandlers,
    private readonly host: FeedHost = browser,
  ) {}

  start(): void {
    this.closed = false;
    this.connect();
  }

  close(): void {
    this.closed = true;
    if (this.timer !== null) this.host.clearTimeout(this.timer);
    this.timer = null;
    this.source?.close();
    this.source = null;
  }

  private connect(): void {
    if (this.closed) return;
    this.handlers.onConnection("connecting");
    const query = this.session ? `?session=${encodeURIComponent(this.session)}` : "";
    const source = this.host.open(`/api/1/events${query}`);
    this.source = source;
    source.onopen = () => {
      this.attempt = 0;
      this.handlers.onConnection("live");
      if (this.dropped) {
        this.dropped = false;
        this.handlers.onResync();
      }
    };
    source.onerror = () => {
      source.close();
      if (this.source !== source || this.closed) return;
      this.source = null;
      this.dropped = true;
      this.handlers.onConnection("disconnected");
      const retry = () => {
        if (this.closed) return;
        this.timer = this.host.setTimeout(() => {
          this.timer = null;
          this.connect();
        }, backoff(this.attempt++));
      };
      const check = this.handlers.stillSignedIn;
      if (!check) {
        retry();
        return;
      }
      void check().then(
        (signedIn) => {
          if (this.closed) return;
          if (signedIn) retry();
          else this.handlers.onConnection("signed-out");
        },
        retry,
      );
    };
    for (const kind of EVENT_KINDS) {
      source.addEventListener(kind, (message) => {
        const event = parseEvent((message as MessageEvent<string>).data);
        if (event) this.handlers.onEvent(event);
      });
    }
    source.addEventListener(RESYNC, () => this.handlers.onResync());
  }
}
