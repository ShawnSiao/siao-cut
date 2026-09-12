import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import type { Segment } from "../../types";

const ESTIMATED_HEIGHT = 148;
const OVERSCAN = 4;
export function visibleRange(offsets: number[], top: number, height: number): [number, number] {
  const find = (position: number) => {
    let low = 0, high = offsets.length - 1;
    while (low < high) { const mid = Math.ceil((low + high) / 2); if (offsets[mid] <= position) low = mid; else high = mid - 1; }
    return low;
  };
  return [Math.max(0, find(top) - OVERSCAN), Math.min(offsets.length - 2, find(top + height) + OVERSCAN)];
}

function MeasuredRow({ id, top, onMeasure, children }: { id: string; top: number; onMeasure: (id: string, height: number) => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = ref.current; if (!node) return;
    const measure = () => onMeasure(id, node.getBoundingClientRect().height);
    measure(); if (typeof ResizeObserver === "undefined") return; const observer = new ResizeObserver(measure); observer.observe(node);
    return () => observer.disconnect();
  }, [id, onMeasure]);
  return <div ref={ref} style={{ position: "absolute", top, left: 0, right: 0, paddingBottom: 8 }}>{children}</div>;
}

/** Variable-height windowing. A focused row remains mounted even outside the viewport. */
export function VirtualTranscript({ segments, listRef, label, empty, renderRow }: {
  segments: Segment[]; listRef: RefObject<HTMLDivElement | null>; label: string; empty: ReactNode; renderRow: (segment: Segment) => ReactNode;
}) {
  const heights = useRef(new Map<string, number>());
  const [measurement, setMeasurement] = useState(0);
  const [viewport, setViewport] = useState({ top: 0, height: 700 });
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const frame = useRef(0);
  const virtual = segments.length > 100;
  const offsets = useMemo(() => {
    const result = [0]; for (const segment of segments) result.push(result.at(-1)! + (heights.current.get(segment.id) ?? ESTIMATED_HEIGHT));
    return result;
  }, [segments, measurement]);
  const measure = useCallback((id: string, height: number) => {
    if (height <= 0 || heights.current.get(id) === height) return;
    heights.current.set(id, height);
    if (!frame.current) frame.current = requestAnimationFrame(() => { frame.current = 0; setMeasurement((value) => value + 1); });
  }, []);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  useLayoutEffect(() => {
    const node = listRef.current; if (!node) return;
    const update = () => setViewport({ top: node.scrollTop, height: node.clientHeight });
    update(); if (typeof ResizeObserver === "undefined") return; const observer = new ResizeObserver(update); observer.observe(node);
    return () => observer.disconnect();
  }, [listRef]);
  useEffect(() => {
    const node = listRef.current; if (!node || !virtual) return;
    const locate = (event: Event) => {
      const id = (event as CustomEvent<string>).detail;
      const index = segments.findIndex((segment) => segment.id === id);
      if (index < 0) return;
      node.scrollTop = offsets[index]; setViewport({ top: node.scrollTop, height: node.clientHeight });
    };
    node.addEventListener("transcript-locate", locate); return () => node.removeEventListener("transcript-locate", locate);
  }, [listRef, offsets, segments, virtual]);
  const [start, end] = visibleRange(offsets, viewport.top, viewport.height);
  const indices = new Set<number>();
  if (virtual) {
    for (let index = start; index <= end; index++) indices.add(index);
    const pinned = segments.findIndex((segment) => segment.id === focusedId); if (pinned >= 0) indices.add(pinned);
  }
  return <div ref={listRef} className="segment-list" aria-label={label}
    onScroll={(event) => { const node = event.currentTarget; setViewport({ top: node.scrollTop, height: node.clientHeight }); }}
    onFocusCapture={(event) => setFocusedId((event.target as HTMLElement).closest<HTMLElement>("[data-segment-id]")?.dataset.segmentId ?? null)}
    onBlurCapture={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocusedId(null); }}>
    {virtual ? <div style={{ position: "relative", height: offsets.at(-1), flexShrink: 0 }} data-virtual-total={segments.length}>
      {[...indices].sort((a, b) => a - b).map((index) => <MeasuredRow key={segments[index].id} id={segments[index].id} top={offsets[index]} onMeasure={measure}>{renderRow(segments[index])}</MeasuredRow>)}
    </div> : segments.map(renderRow)}
    {!segments.length && empty}
  </div>;
}
