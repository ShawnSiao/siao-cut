import { afterEach, describe, expect, it, vi } from "vitest";
import { startBackgroundTaskRegistry } from "./use-background-task-registry";

afterEach(() => {
  vi.useRealTimers();
});

describe("background task registry", () => {
  it("runs task types independently while keeping each task serial", async () => {
    vi.useFakeTimers();
    let releaseFirst: () => void = () => undefined;
    const slow = vi.fn(() => new Promise<void>((resolve) => {
      releaseFirst = resolve;
    }));
    const fast = vi.fn(async () => undefined);
    const stop = startBackgroundTaskRegistry([
      { key: "slow", intervalMs: 10, poll: slow },
      { key: "fast", intervalMs: 20, poll: fast },
    ]);

    await vi.advanceTimersByTimeAsync(60);
    expect(slow).toHaveBeenCalledTimes(1);
    expect(fast).toHaveBeenCalledTimes(3);

    releaseFirst();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(10);
    expect(slow).toHaveBeenCalledTimes(2);

    stop();
    const fastCount = fast.mock.calls.length;
    await vi.advanceTimersByTimeAsync(100);
    expect(fast).toHaveBeenCalledTimes(fastCount);
  });
});
