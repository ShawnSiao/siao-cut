import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { startBackgroundTaskRegistry, useBackgroundTaskRegistry } from "./use-background-task-registry";

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

it("adding or removing another task cannot restart an in-flight poll",async()=>{
  vi.useFakeTimers();
  let release!:()=>void;
  const slow=vi.fn(()=>new Promise<void>((resolve)=>{release=resolve;}));
  const fast=vi.fn(async()=>{});
  const {rerender,unmount}=renderHook(({extra})=>useBackgroundTaskRegistry([{key:"slow",intervalMs:10,poll:slow},extra?{key:"fast",intervalMs:10,poll:fast}:null]),{initialProps:{extra:false}});
  await act(async()=>{await vi.advanceTimersByTimeAsync(10);});
  expect(slow).toHaveBeenCalledTimes(1);
  rerender({extra:true});
  await act(async()=>{await vi.advanceTimersByTimeAsync(100);});
  expect(slow).toHaveBeenCalledTimes(1);expect(fast.mock.calls.length).toBeGreaterThan(1);
  rerender({extra:false});
  await act(async()=>{release();await vi.advanceTimersByTimeAsync(10);});
  expect(slow).toHaveBeenCalledTimes(2);unmount();
});
