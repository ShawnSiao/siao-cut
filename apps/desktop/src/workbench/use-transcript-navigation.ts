import { useCallback, useEffect, useRef, useState } from "react";
import type { Project } from "../types";

export function cycleWorkbenchFocus(reverse: boolean) {
  const regions = [".editor-grid", ".creator-player", ".creator-drawer"];
  const active = document.activeElement;
  const index = regions.findIndex((selector) => active instanceof Element && active.closest(selector));
  const next = (index + (reverse ? 2 : 1)) % regions.length;
  const targets = [[".segment-row.active textarea", ".segment-list textarea"], ["video", "button"], ['[role="tab"][aria-selected="true"]']];
  const region = document.querySelector<HTMLElement>(regions[next]);
  targets[next].map((selector) => region?.querySelector<HTMLElement>(selector)).find(Boolean)?.focus({ preventScroll: true });
}

export function useTranscriptNavigation(project: Project | null, time: number, playing: boolean) {
  const listRef = useRef<HTMLDivElement>(null);
  const [followPlayback, setFollowPlayback] = useState(false);
  const playbackSegmentId = project?.transcript.segments.find((segment) => time >= segment.start && time < segment.end)?.id ?? null;
  const locate = useCallback((id: string) => {
    const list = listRef.current;
    const row = list && Array.from(list.querySelectorAll<HTMLElement>("[data-segment-id]")).find((node) => node.dataset.segmentId === id);
    if (!list || !row) return;
    const viewport = list.getBoundingClientRect(), bounds = row.getBoundingClientRect();
    if (bounds.top < viewport.top) list.scrollTop += bounds.top - viewport.top;
    else if (bounds.bottom > viewport.bottom) list.scrollTop += bounds.bottom - viewport.bottom;
  }, []);
  useEffect(() => { if (listRef.current) listRef.current.scrollTop = 0; }, [project?.id]);
  useEffect(() => {
    if (!followPlayback || !playing || !playbackSegmentId) return;
    const focused = document.activeElement;
    // Native composition and ordinary typing both keep ownership of the editing viewport.
    if (focused instanceof HTMLTextAreaElement && listRef.current?.contains(focused)) return;
    locate(playbackSegmentId);
  }, [followPlayback, locate, playbackSegmentId, playing]);
  return { listRef, followPlayback, setFollowPlayback, playbackSegmentId, locate };
}
