import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { sampleProject } from "../mock";
import { useTranscriptNavigation } from "./use-transcript-navigation";

afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks(); });

it("keeps playback separate from the editor viewport until follow is enabled, and never steals typing focus", () => {
  const project = structuredClone(sampleProject);
  const { result, rerender, unmount } = renderHook(({ time, playing }) => useTranscriptNavigation(project, time, playing), { initialProps: { time: 0, playing: false } });
  const list = document.createElement("div");
  for (const [index, segment] of project.transcript.segments.entries()) {
    const row = document.createElement("div"); row.dataset.segmentId = segment.id;
    row.append(document.createElement("textarea")); list.append(row);
    vi.spyOn(row, "getBoundingClientRect").mockReturnValue(new DOMRect(0, index * 200, 100, 100));
  }
  document.body.append(list); result.current.listRef.current = list;
  vi.spyOn(list, "getBoundingClientRect").mockReturnValue(new DOMRect(0, 0, 100, 100));
  rerender({ time: 14, playing: true });
  expect(list.scrollTop).toBe(0);
  const editor = list.querySelector("textarea")!; editor.focus();
  act(() => result.current.setFollowPlayback(true));
  rerender({ time: 19, playing: true });
  expect(list.scrollTop).toBe(0);
  expect(document.activeElement).toBe(editor);
  editor.blur();
  rerender({ time: 25, playing: true });
  expect(list.scrollTop).toBeGreaterThan(0);
  expect(document.activeElement).not.toBe(editor);
  unmount();
});
