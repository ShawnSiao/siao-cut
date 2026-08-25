import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { Bot, ChevronDown, ChevronUp, CircleAlert, Clock3, Eye, Focus, ListChecks, Minus, MoveHorizontal, Pause, Play, Plus, RotateCcw, Users } from "lucide-react";
import { tr } from "../i18n";
import type { AudioRisk, Project, Segment, SpeakerTrack, TranscriptionReviewItem } from "../types";
import { formatTime } from "../app-view-model";

export const TIMELINE_PREFERENCES_STORAGE_KEY = "siaocut.timelinePreferences.v1";
const TIMELINE_PIXELS_PER_SECOND = 4.8;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 3;

export type TimelineExpandedMode = "edit" | "review";

export type TimelinePreferencesV1 = {
  version: 1;
  expanded: boolean;
  mode: TimelineExpandedMode;
  zoom: number;
  followPlayhead: boolean;
};

export const DEFAULT_TIMELINE_PREFERENCES: TimelinePreferencesV1 = {
  version: 1,
  expanded: true,
  mode: "edit",
  zoom: 1.6,
  followPlayhead: true,
};

export type TimelineReviewMarker = {
  id: string;
  source: "quality" | "agent" | "transcription" | "edit" | "audio";
  tone: "error" | "warning" | "agent" | "cut" | "applied";
  segmentId: string | null;
  start: number;
  end: number;
  detail: string;
  detailTarget: "quality" | "review" | "analysis" | "history";
  detailId: string;
};

export function clampTimelineZoom(value: number) {
  if (!Number.isFinite(value))
    return DEFAULT_TIMELINE_PREFERENCES.zoom;
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, value));
}

export function parseTimelinePreferences(value: string | null): TimelinePreferencesV1 {
  if (!value)
    return DEFAULT_TIMELINE_PREFERENCES;
  try {
    const candidate = JSON.parse(value) as Partial<TimelinePreferencesV1>;
    if (candidate.version !== 1)
      return DEFAULT_TIMELINE_PREFERENCES;
    return {
      version: 1,
      expanded: typeof candidate.expanded === "boolean" ? candidate.expanded : DEFAULT_TIMELINE_PREFERENCES.expanded,
      mode: candidate.mode === "review" ? "review" : "edit",
      zoom: clampTimelineZoom(Number(candidate.zoom)),
      followPlayhead: typeof candidate.followPlayhead === "boolean" ? candidate.followPlayhead : DEFAULT_TIMELINE_PREFERENCES.followPlayhead,
    };
  }
  catch {
    return DEFAULT_TIMELINE_PREFERENCES;
  }
}

export function resolveTimelineDuration(project: Project, playbackDuration: number) {
  const candidates = [
    playbackDuration,
    project.media.durationSeconds ?? 0,
    project.timeline.sourceDuration,
    ...project.transcript.segments.map((segment) => segment.end),
  ].filter((value) => Number.isFinite(value) && value > 0);
  return Math.max(0, ...candidates);
}

export function timelinePercent(time: number, duration: number) {
  if (!Number.isFinite(time) || !Number.isFinite(duration) || duration <= 0)
    return 0;
  return Math.max(0, Math.min(100, time / duration * 100));
}

export function timelineTickInterval(duration: number, canvasWidth: number) {
  if (!Number.isFinite(duration) || duration <= 0 || !Number.isFinite(canvasWidth) || canvasWidth <= 0)
    return 10;
  const desiredInterval = duration / Math.max(2, canvasWidth / 90);
  const intervals = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
  return intervals.find((interval) => interval >= desiredInterval) ?? intervals.at(-1)!;
}

function markerRange(segmentById: Map<string, Segment>, segmentId: string | null) {
  if (!segmentId)
    return null;
  const segment = segmentById.get(segmentId);
  return segment ? { segmentId, start: segment.start, end: segment.end } : null;
}

export function deriveTimelineReviewMarkers(
  project: Project,
  transcriptionReviews: TranscriptionReviewItem[],
  audioRisks: AudioRisk[] = [],
): TimelineReviewMarker[] {
  const segmentById = new Map(project.transcript.segments.map((segment) => [segment.id, segment]));
  const markers: TimelineReviewMarker[] = [];

  project.subtitleQuality.issues.forEach((issue) => {
    const range = markerRange(segmentById, issue.segmentId);
    if (!range)
      return;
    markers.push({
      id: `quality:${issue.id}`,
      source: "quality",
      tone: issue.severity,
      ...range,
      detail: issue.message,
      detailTarget: "quality",
      detailId: `quality:${issue.id}`,
    });
  });

  project.patchSets.forEach((set) => set.items
    .filter((item) => ["pending", "conflict"].includes(item.status))
    .forEach((item) => {
      const range = markerRange(segmentById, item.segmentId);
      if (!range)
        return;
      markers.push({
        id: `agent:${item.id}`,
        source: "agent",
        tone: "agent",
        ...range,
        detail: item.reason,
        detailTarget: "review",
        detailId: `agent:${item.id}`,
      });
    }));

  transcriptionReviews
    .filter((item) => item.status === "open")
    .forEach((item) => {
      const range = markerRange(segmentById, item.segmentId);
      if (!range)
        return;
      markers.push({
        id: `transcription:${item.id}`,
        source: "transcription",
        tone: item.severity === "error" ? "error" : item.severity === "warning" ? "warning" : "agent",
        ...range,
        detail: item.message,
        detailTarget: "review",
        detailId: `transcription:${item.id}`,
      });
    });

  project.edits
    .filter((edit) => ["suggested", "proposed"].includes(edit.status))
    .forEach((edit) => {
      const range = markerRange(segmentById, edit.segmentId);
      if (!range)
        return;
      markers.push({
        id: `edit:${edit.id}`,
        source: "edit",
        tone: "cut",
        ...range,
        start: Number.isFinite(edit.start) ? edit.start : range.start,
        end: Number.isFinite(edit.end) ? edit.end : range.end,
        detail: edit.reason,
        detailTarget: "review",
        detailId: `edit:${edit.id}`,
      });
    });

  audioRisks.forEach((risk) => markers.push({
    id: `audio:${risk.kind}:${risk.start}:${risk.end}`,
    source: "audio",
    tone: "warning",
    segmentId: project.transcript.segments.find((segment) => risk.start >= segment.start && risk.start < segment.end)?.id ?? null,
    start: risk.start,
    end: risk.end,
    detail: risk.kind,
    detailTarget: "analysis",
    detailId: `audio:${risk.kind}:${risk.start}:${risk.end}`,
  }));

  const unique = new Map<string, TimelineReviewMarker>();
  markers
    .sort((left, right) => left.start - right.start || left.id.localeCompare(right.id))
    .forEach((marker) => unique.set(marker.id, marker));
  return [...unique.values()];
}

export function deriveTimelineSpeakerTurns(project: Project, speakerTrack: SpeakerTrack | null) {
  if (speakerTrack?.turns.length)
    return speakerTrack.turns;
  if (!speakerTrack?.associations.length)
    return [];
  const associationBySegment = new Map(speakerTrack.associations.map((association) => [association.segmentId, association]));
  const turns: Array<{ id: string; speakerId: string; start: number; end: number }> = [];
  project.transcript.segments.forEach((segment) => {
    const association = associationBySegment.get(segment.id);
    if (!association)
      return;
    const current = turns.at(-1);
    if (current?.speakerId === association.speakerId && segment.start <= current.end + 0.25) {
      current.end = Math.max(current.end, segment.end);
      return;
    }
    turns.push({ id: `derived:${segment.id}`, speakerId: association.speakerId, start: segment.start, end: segment.end });
  });
  return turns;
}

function preciseTime(seconds: number) {
  const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0);
  const minutes = Math.floor(safe / 60);
  const rest = safe - minutes * 60;
  return `${String(minutes).padStart(2, "0")}:${rest.toFixed(1).padStart(4, "0")}`;
}

function markerLabel(marker: TimelineReviewMarker) {
  if (marker.source === "quality")
    return tr("app.timeline.review.quality");
  if (marker.source === "agent")
    return tr("app.timeline.review.agent");
  if (marker.source === "transcription")
    return tr("app.timeline.review.transcription");
  if (marker.source === "audio")
    return tr("app.focusReview.kind.audio");
  return marker.tone === "applied" ? tr("app.timeline.review.appliedCut") : tr("app.timeline.review.cut");
}

export function SubtitleTimelinePanel({
  project,
  speakerTrack,
  transcriptionReviews,
  audioRisks = [],
  waveformUrl,
  playback,
  selectedId,
  selectedSegmentIds,
  busy,
  onSelectSegment,
  onSeek,
  onTogglePlayback,
  onNudgeSelected,
  onOpenTiming,
  onOpenReviewDetail,
  onRestoreCut,
  canEnterFocusReview = false,
  onEnterFocusReview = () => undefined,
}: {
  project: Project;
  speakerTrack: SpeakerTrack | null;
  transcriptionReviews: TranscriptionReviewItem[];
  audioRisks?: AudioRisk[];
  waveformUrl: string | null;
  playback: { playing: boolean; currentTime: number; duration: number };
  selectedId: string | null;
  selectedSegmentIds: string[];
  busy: boolean;
  onSelectSegment: (segment: Segment) => void;
  onSeek: (time: number) => void;
  onTogglePlayback: () => void;
  onNudgeSelected: (segmentId: string, delta: number) => void;
  onOpenTiming: (segment: Segment) => void;
  onOpenReviewDetail: (marker: TimelineReviewMarker) => void;
  onRestoreCut: (editId: string) => void;
  canEnterFocusReview?: boolean;
  onEnterFocusReview?: () => void;
}) {
  const [preferences, setPreferences] = useState(() => parseTimelinePreferences(localStorage.getItem(TIMELINE_PREFERENCES_STORAGE_KEY)));
  const [containerWidth, setContainerWidth] = useState(960);
  const [scrollMetrics, setScrollMetrics] = useState({ left: 0, clientWidth: 1, scrollWidth: 1 });
  const scrollRef = useRef<HTMLDivElement>(null);
  const overviewRef = useRef<HTMLDivElement>(null);
  const seekingRef = useRef(false);

  const duration = resolveTimelineDuration(project, playback.duration);
  const selected = project.transcript.segments.find((segment) => segment.id === selectedId) ?? null;
  const selectedEdit = selected ? project.edits.find((edit) => edit.segmentId === selected.id && ["suggested", "proposed", "applied"].includes(edit.status)) ?? null : null;
  const activeSegment = project.transcript.segments.find((segment) => playback.currentTime >= segment.start && playback.currentTime < segment.end) ?? null;
  const reviewMarkers = useMemo(() => deriveTimelineReviewMarkers(project, transcriptionReviews, audioRisks), [audioRisks, project, transcriptionReviews]);
  const speakerTurns = useMemo(() => deriveTimelineSpeakerTurns(project, speakerTrack), [project, speakerTrack]);
  const speakerById = useMemo(() => new Map(speakerTrack?.speakers.map((speaker) => [speaker.id, speaker]) ?? []), [speakerTrack]);
  const canvasWidth = preferences.expanded
    ? Math.max(containerWidth, duration * TIMELINE_PIXELS_PER_SECOND * preferences.zoom)
    : Math.max(1, containerWidth);
  const tickInterval = timelineTickInterval(duration, canvasWidth);
  const ticks = useMemo(() => {
    const values: number[] = [];
    if (duration <= 0)
      return values;
    for (let time = 0; time <= duration + 0.0001; time += tickInterval)
      values.push(time);
    if (values.at(-1)! < duration)
      values.push(duration);
    return values;
  }, [duration, tickInterval]);

  useEffect(() => {
    localStorage.setItem(TIMELINE_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences));
  }, [preferences]);

  useEffect(() => {
    const scroll = scrollRef.current;
    const update = () => {
      const nextWidth = scroll?.clientWidth || overviewRef.current?.clientWidth || 960;
      setContainerWidth(nextWidth);
      if (scroll)
        setScrollMetrics({ left: scroll.scrollLeft, clientWidth: Math.max(1, scroll.clientWidth), scrollWidth: Math.max(1, scroll.scrollWidth) });
    };
    update();
    scroll?.addEventListener("scroll", update, { passive: true });
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    if (scroll)
      observer?.observe(scroll);
    if (overviewRef.current)
      observer?.observe(overviewRef.current);
    window.addEventListener("resize", update);
    return () => {
      scroll?.removeEventListener("scroll", update);
      observer?.disconnect();
      window.removeEventListener("resize", update);
    };
  }, [preferences.expanded]);

  useEffect(() => {
    const scroll = scrollRef.current;
    if (!scroll || !preferences.expanded || !preferences.followPlayhead || !playback.playing || duration <= 0)
      return;
    const playheadX = playback.currentTime / duration * canvasWidth;
    const edgePadding = Math.min(140, scroll.clientWidth * 0.2);
    if (playheadX < scroll.scrollLeft + edgePadding || playheadX > scroll.scrollLeft + scroll.clientWidth - edgePadding)
      scroll.scrollTo({ left: Math.max(0, playheadX - scroll.clientWidth * 0.35), behavior: "smooth" });
  }, [canvasWidth, duration, playback.currentTime, playback.playing, preferences.expanded, preferences.followPlayhead]);

  const updatePreferences = (patch: Partial<TimelinePreferencesV1>) => {
    setPreferences((current) => ({ ...current, ...patch, version: 1, zoom: patch.zoom == null ? current.zoom : clampTimelineZoom(patch.zoom) }));
  };

  const seekFromPointer = (event: ReactPointerEvent<HTMLElement>, target: HTMLElement) => {
    if (duration <= 0)
      return;
    const rect = target.getBoundingClientRect();
    if (rect.width <= 0)
      return;
    onSeek((event.clientX - rect.left) / rect.width * duration);
  };

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("[data-timeline-action]"))
      return;
    seekingRef.current = true;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    seekFromPointer(event, event.currentTarget);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (seekingRef.current)
      seekFromPointer(event, event.currentTarget);
  };

  const handlePointerEnd = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!seekingRef.current)
      return;
    seekFromPointer(event, event.currentTarget);
    seekingRef.current = false;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
  };

  const fitTimeline = () => {
    if (duration <= 0)
      return;
    updatePreferences({ zoom: containerWidth / (duration * TIMELINE_PIXELS_PER_SECOND) });
  };

  const segmentButtons = (compact: boolean) => project.transcript.segments.map((segment, index) => {
    const edit = project.edits.find((candidate) => candidate.segmentId === segment.id && ["suggested", "proposed", "applied"].includes(candidate.status));
    const association = speakerTrack?.associations.find((candidate) => candidate.segmentId === segment.id);
    const speaker = association ? speakerById.get(association.speakerId) : null;
    const selectedSegment = selectedSegmentIds.includes(segment.id);
    const active = activeSegment?.id === segment.id;
    return <button
      type="button"
      data-timeline-action
      className={`subtitle-timeline-segment${compact ? " compact" : ""}${selectedSegment ? " selected" : ""}${active ? " active" : ""}${edit ? ` ${edit.status}` : ""}`}
      key={segment.id}
      style={{ left: `${timelinePercent(segment.start, duration)}%`, width: `${Math.max(0, timelinePercent(segment.end, duration) - timelinePercent(segment.start, duration))}%` }}
      aria-label={tr("app.timeline.segmentLabel", { index: index + 1, start: preciseTime(segment.start), end: preciseTime(segment.end), text: segment.text })}
      title={`${speaker ? `${speaker.label} · ` : ""}${preciseTime(segment.start)} — ${preciseTime(segment.end)} · ${segment.text}`}
      onClick={() => onSelectSegment(segment)}
    >
      {speaker && <i className={`speaker-color speaker-${speaker.colorIndex % 6}`}/>}
      {!compact && <span className="subtitle-timeline-segment-time">{preciseTime(segment.start)} · {(segment.end - segment.start).toFixed(1)}s</span>}
      <span>{segment.text}</span>
    </button>;
  });

  const playhead = <div
    className="subtitle-timeline-playhead"
    style={{ left: `${timelinePercent(playback.currentTime, duration)}%` }}
    role="slider"
    tabIndex={0}
    aria-label={tr("app.timeline.playhead")}
    aria-valuemin={0}
    aria-valuemax={Math.max(0, duration)}
    aria-valuenow={Math.max(0, playback.currentTime)}
    aria-valuetext={preciseTime(playback.currentTime)}
    onKeyDown={(event) => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
        return;
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Home")
        onSeek(0);
      else if (event.key === "End")
        onSeek(duration);
      else
        onSeek(playback.currentTime + (event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 0.1 : 1));
    }}
  ><i/></div>;

  return <section className={`timeline-panel subtitle-timeline-panel ${preferences.expanded ? `expanded ${preferences.mode}` : "collapsed overview"}`} aria-label={tr("app.timeline.region")}>
    <header className="subtitle-timeline-header">
      <div className="subtitle-timeline-heading">
        <span><p className="eyebrow">{tr("app.s0387")}</p><h2>{tr("app.s0388")}</h2></span>
        <span className="subtitle-timeline-selection">
          <strong>{selected ? tr("app.timeline.selected", { index: project.transcript.segments.indexOf(selected) + 1 }) : tr("app.timeline.noneSelected")}</strong>
          <small>{selected ? `${preciseTime(selected.start)} — ${preciseTime(selected.end)}` : tr("app.timeline.selectHelp")}</small>
        </span>
      </div>
      <div className="subtitle-timeline-controls">
        {preferences.expanded && <>
          <button type="button" aria-label={playback.playing ? tr("app.timeline.pause") : tr("app.timeline.play")} onClick={onTogglePlayback}>{playback.playing ? <Pause size={13}/> : <Play size={13}/>}<span>{preciseTime(playback.currentTime)}</span></button>
          <div className="subtitle-timeline-modes" role="group" aria-label={tr("app.timeline.modeLabel")}>
            <button type="button" className={preferences.mode === "edit" ? "active" : ""} aria-pressed={preferences.mode === "edit"} onClick={() => updatePreferences({ mode: "edit" })}><Clock3 size={13}/>{tr("app.timeline.mode.edit")}</button>
            <button type="button" className={preferences.mode === "review" ? "active" : ""} aria-pressed={preferences.mode === "review"} onClick={() => updatePreferences({ mode: "review" })}><ListChecks size={13}/>{tr("app.timeline.mode.review")}{reviewMarkers.length > 0 && <i>{reviewMarkers.length > 99 ? "99+" : reviewMarkers.length}</i>}</button>
          </div>
          <div className="subtitle-timeline-zoom" role="group" aria-label={tr("app.timeline.zoomGroup")}>
            <button type="button" aria-label={tr("app.timeline.zoomOut")} disabled={preferences.zoom <= MIN_ZOOM} onClick={() => updatePreferences({ zoom: preferences.zoom - 0.1 })}><Minus size={13}/></button>
            <input aria-label={tr("app.timeline.zoom")} type="range" min={MIN_ZOOM * 100} max={MAX_ZOOM * 100} step={5} value={Math.round(preferences.zoom * 100)} onChange={(event) => updatePreferences({ zoom: Number(event.target.value) / 100 })}/>
            <span>{Math.round(preferences.zoom * 100)}%</span>
            <button type="button" aria-label={tr("app.timeline.zoomIn")} disabled={preferences.zoom >= MAX_ZOOM} onClick={() => updatePreferences({ zoom: preferences.zoom + 0.1 })}><Plus size={13}/></button>
            <button type="button" onClick={fitTimeline}><Focus size={13}/>{tr("app.timeline.fit")}</button>
          </div>
          <label className="subtitle-timeline-follow"><input type="checkbox" checked={preferences.followPlayhead} onChange={(event) => updatePreferences({ followPlayhead: event.target.checked })}/><Eye size={13}/>{tr("app.timeline.follow")}</label>
          {preferences.mode === "review" && <button type="button" className="timeline-focus-review" disabled={!canEnterFocusReview} title={canEnterFocusReview ? undefined : tr("app.focusReview.mediaMissing")} onClick={onEnterFocusReview}><Focus size={13}/>{tr("app.focusReview.enter")}</button>}
        </>}
        <button type="button" className="timeline-toggle" aria-expanded={preferences.expanded} onClick={() => updatePreferences({ expanded: !preferences.expanded })}>{preferences.expanded ? <ChevronDown size={14}/> : <ChevronUp size={14}/>}{preferences.expanded ? tr("app.creator.timeline.collapse") : tr("app.creator.timeline.expand")}</button>
      </div>
    </header>

    {!preferences.expanded ? <div className="subtitle-timeline-overview" ref={overviewRef} onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerEnd} onPointerCancel={handlePointerEnd}>
      {waveformUrl ? <img src={waveformUrl} alt={tr("app.s0390")}/> : <span className="subtitle-timeline-empty-waveform">{tr("app.timeline.waveformUnavailable")}</span>}
      <div className="subtitle-timeline-segments compact">{segmentButtons(true)}</div>
      {playhead}
    </div> : <>
      <div className="subtitle-timeline-actions" aria-label={tr("app.timeline.selectionActions")}>
        <span><MoveHorizontal size={13}/>{selected ? tr("app.timeline.nudgeHelp") : tr("app.timeline.selectHelp")}</span>
        <button type="button" disabled={!selected || busy || selected.start < 0.1} onClick={() => selected && onNudgeSelected(selected.id, -0.1)}>{tr("app.timeline.nudgeEarlier")}</button>
        <button type="button" disabled={!selected || busy || (duration > 0 && selected.end + 0.1 > duration)} onClick={() => selected && onNudgeSelected(selected.id, 0.1)}>{tr("app.timeline.nudgeLater")}</button>
        <button type="button" disabled={!selected || busy} onClick={() => selected && onOpenTiming(selected)}><Clock3 size={13}/>{tr("app.timeline.exactTiming")}</button>
        {selectedEdit?.status === "applied" && <button type="button" disabled={busy} onClick={() => onRestoreCut(selectedEdit.id)}><RotateCcw size={13}/>{tr("app.timeline.restoreCut")}</button>}
      </div>
      <div className="subtitle-timeline-scroll" ref={scrollRef}>
        <div className="subtitle-timeline-canvas" style={{ width: `${canvasWidth}px` }} onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerEnd} onPointerCancel={handlePointerEnd}>
          <div className="subtitle-timeline-ruler" aria-hidden="true">
            {ticks.map((time) => <i key={time} className={time % 60 === 0 ? "major" : ""} style={{ left: `${timelinePercent(time, duration)}%` }}><span>{formatTime(time)}</span></i>)}
          </div>
          <div className="subtitle-timeline-waveform">
            {waveformUrl ? <img src={waveformUrl} alt={tr("app.s0390")}/> : <span>{tr("app.timeline.waveformUnavailable")}</span>}
          </div>
          <div className="subtitle-timeline-lane-label">{tr("app.timeline.subtitlesLane")}</div>
          <div className="subtitle-timeline-segments">{segmentButtons(false)}</div>
          {preferences.mode === "review" && <>
            <div className="subtitle-timeline-lane-label speakers"><Users size={12}/>{tr("app.timeline.speakersLane")}</div>
            <div className="subtitle-timeline-speakers">
              {speakerTurns.length > 0 ? speakerTurns.map((turn) => {
                const speaker = speakerById.get(turn.speakerId);
                return <span key={turn.id} className={`speaker-${(speaker?.colorIndex ?? 0) % 6}`} style={{ left: `${timelinePercent(turn.start, duration)}%`, width: `${Math.max(0, timelinePercent(turn.end, duration) - timelinePercent(turn.start, duration))}%` }} title={`${speaker?.label ?? turn.speakerId} · ${preciseTime(turn.start)} — ${preciseTime(turn.end)}`}><i/>{speaker?.label ?? tr("app.timeline.unknownSpeaker")}</span>;
              }) : <small>{tr("app.timeline.speakersUnavailable")}</small>}
            </div>
            <div className="subtitle-timeline-lane-label review"><CircleAlert size={12}/>{tr("app.timeline.reviewLane")}</div>
            <div className="subtitle-timeline-review-markers">
              {reviewMarkers.length > 0 ? reviewMarkers.map((marker, index) => <button
                type="button"
                data-timeline-action
                key={marker.id}
                className={marker.tone}
                style={{ left: `${timelinePercent(marker.start, duration)}%`, top: `${5 + index % 2 * 22}px` }}
                title={`${markerLabel(marker)} · ${marker.detail}`}
                aria-label={`${markerLabel(marker)} · ${preciseTime(marker.start)} · ${marker.detail}`}
                onClick={() => onOpenReviewDetail(marker)}
              ><Bot size={11}/><span>{markerLabel(marker)}</span></button>) : <small>{tr("app.timeline.reviewUnavailable")}</small>}
            </div>
          </>}
          {playhead}
        </div>
      </div>
      <div className="subtitle-timeline-navigator" onClick={(event) => {
        const scroll = scrollRef.current;
        if (!scroll)
          return;
        const rect = event.currentTarget.getBoundingClientRect();
        const center = (event.clientX - rect.left) / rect.width * scroll.scrollWidth;
        scroll.scrollTo({ left: Math.max(0, center - scroll.clientWidth / 2), behavior: "smooth" });
      }}>
        <div>{project.transcript.segments.map((segment) => <i key={segment.id} className={selectedSegmentIds.includes(segment.id) ? "selected" : ""} style={{ left: `${timelinePercent(segment.start, duration)}%`, width: `${Math.max(0, timelinePercent(segment.end, duration) - timelinePercent(segment.start, duration))}%` }}/>)}</div>
        <span style={{ left: `${scrollMetrics.left / scrollMetrics.scrollWidth * 100}%`, width: `${Math.min(100, scrollMetrics.clientWidth / scrollMetrics.scrollWidth * 100)}%` }}/>
      </div>
    </>}
  </section>;
}
