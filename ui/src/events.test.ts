import { describe, expect, it } from "vitest";

import { EventFeed, type Connection, type FeedHost, backoff, parseEvent } from "./events";

/** An EventSource stand-in the test drives. */
class FakeSource {
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  readonly listeners = new Map<string, ((m: { data: string }) => void)[]>();
  constructor(readonly url: string) {}
  addEventListener(kind: string, listener: (m: { data: string }) => void): void {
    this.listeners.set(kind, [...(this.listeners.get(kind) ?? []), listener]);
  }
  emit(kind: string, data: string): void {
    for (const listener of this.listeners.get(kind) ?? []) listener({ data });
  }
  close(): void {
    this.closed = true;
  }
}

function harness(stillSignedIn?: () => Promise<boolean>) {
  const sources: FakeSource[] = [];
  const timers: { run: () => void; ms: number }[] = [];
  const host: FeedHost = {
    open: (url) => {
      const source = new FakeSource(url);
      sources.push(source);
      return source as unknown as EventSource;
    },
    setTimeout: (run, ms) => timers.push({ run, ms }),
    clearTimeout: () => {},
  };
  const states: Connection[] = [];
  const events: unknown[] = [];
  let resyncs = 0;
  const feed = new EventFeed(
    "tok",
    {
      onEvent: (e) => events.push(e),
      onResync: () => resyncs++,
      onConnection: (s) => states.push(s),
      ...(stillSignedIn ? { stillSignedIn } : {}),
    },
    host,
  );
  return { feed, sources, timers, states, events, resyncs: () => resyncs };
}

describe("the event feed", () => {
  it("connects with the session and passes events on", () => {
    const h = harness();
    h.feed.start();
    const source = h.sources[0];
    expect(source?.url).toBe("/api/1/events?session=tok");
    source?.onopen?.();
    expect(h.states).toEqual(["connecting", "live"]);
    source?.emit("view", JSON.stringify({ schema: "totalfloat.event/1", event: "view", label: "x", branch: null, head: null }));
    source?.emit("view", "not json");
    expect(h.events).toHaveLength(1);
  });

  it("reconnects with backoff after a drop, and resyncs once back", () => {
    const h = harness();
    h.feed.start();
    h.sources[0]?.onopen?.();
    h.sources[0]?.onerror?.();
    expect(h.sources[0]?.closed).toBe(true);
    expect(h.states.at(-1)).toBe("disconnected");
    expect(h.timers[0]?.ms).toBe(500);
    h.timers[0]?.run();
    h.sources[1]?.onerror?.();
    expect(h.timers[1]?.ms).toBe(1000);
    h.timers[1]?.run();
    h.sources[2]?.onopen?.();
    expect(h.states.at(-1)).toBe("live");
    expect(h.resyncs()).toBe(1);
  });

  it("stops, signed out, when the server refuses the session", async () => {
    const h = harness(() => Promise.resolve(false));
    h.feed.start();
    h.sources[0]?.onerror?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(h.states.at(-1)).toBe("signed-out");
    expect(h.timers).toHaveLength(0);
  });

  it("retries when still signed in, as when the server stopped", async () => {
    const h = harness(() => Promise.resolve(true));
    h.feed.start();
    h.sources[0]?.onerror?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(h.states.at(-1)).toBe("disconnected");
    expect(h.timers).toHaveLength(1);
  });

  it("stops for good when closed", () => {
    const h = harness();
    h.feed.start();
    h.feed.close();
    h.sources[0]?.onerror?.();
    expect(h.timers).toHaveLength(0);
  });
});

describe("backoff", () => {
  it("doubles from half a second up to thirty", () => {
    expect([0, 1, 2, 5, 20].map(backoff)).toEqual([500, 1000, 2000, 16000, 30000]);
  });
});

describe("parseEvent", () => {
  it("refuses documents that aren't events", () => {
    expect(parseEvent(JSON.stringify({ schema: "totalfloat.error/1" }))).toBeNull();
    expect(parseEvent(JSON.stringify({ schema: "totalfloat.event/1", event: "nope" }))).toBeNull();
  });
});
