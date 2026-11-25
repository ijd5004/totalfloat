// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { FrameClock } from "./frames";

describe("frame timing", () => {
  it("counts every gap while measuring, stalls included, and nothing outside", () => {
    const clock = new FrameClock(true);
    const stats = window.__gridFrames;
    clock.tick(0);
    stats?.reset();
    for (const now of [100, 116, 716, 732]) clock.tick(now);
    stats?.stop();
    clock.tick(5000);
    expect(stats?.frames).toBe(3);
    expect(stats?.worstMs).toBe(600);
    expect(stats?.averageMs).toBeCloseTo(632 / 3);
  });
});
