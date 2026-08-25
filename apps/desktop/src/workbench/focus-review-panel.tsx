import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, CircleAlert, PanelRightClose, PanelRightOpen } from "lucide-react";
import { audioRiskLabel, formatTime } from "../app-view-model";
import { StatusBadge } from "../components/ui";
import { tr } from "../i18n";
import type { AudioRisk, Project, TranscriptionReviewItem } from "../types";
import { deriveReviewQueue, type ReviewQueueItem } from "./review-queue";

export function FocusReviewToolbar({
  remaining,
  subtitleMode,
  translationPending,
  translationStale,
  onSubtitleModeChange,
  onExit,
}: {
  remaining: number;
  subtitleMode: "source" | "translated" | "bilingual";
  translationPending: boolean;
  translationStale: boolean;
  onSubtitleModeChange: (mode: "source" | "translated" | "bilingual") => void;
  onExit: () => void;
}) {
  const exitButtonRef = useRef<HTMLButtonElement>(null);
  useEffect(() => exitButtonRef.current?.focus(), []);
  return <header className="focus-review-toolbar">
    <span><CircleAlert size={17}/><strong>{tr("app.focusReview.header")}</strong><small>{tr("app.focusReview.remaining", { count: remaining })}</small></span>
    <label><span>{tr("app.focusReview.subtitleMode")}</span><select value={subtitleMode} onChange={(event) => onSubtitleModeChange(event.target.value as typeof subtitleMode)}><option value="source">{tr("app.s0406")}</option><option value="translated">{tr("app.s0407")}</option><option value="bilingual">{tr("app.s0408")}</option></select></label>
    {subtitleMode !== "source" && <StatusBadge tone={translationPending || translationStale ? "warning" : "success"}>{translationPending ? tr("app.focusReview.noTranslation") : translationStale ? tr("app.focusReview.translationStale") : tr("app.focusReview.translationCurrent")}</StatusBadge>}
    <button ref={exitButtonRef} type="button" data-focus-review-exit onClick={onExit}>{tr("app.focusReview.exit")}</button>
  </header>;
}

function itemTitle(item: ReviewQueueItem) {
  return tr(({
    quality: "app.focusReview.kind.quality",
    agent: "app.focusReview.kind.agent",
    cut: "app.focusReview.kind.cut",
    transcription: "app.focusReview.kind.transcription",
    audio: "app.focusReview.kind.audio",
  } as const)[item.kind]);
}

export default function FocusReviewPanel({
  project,
  transcriptionReviews,
  audioRisks,
  busy,
  error,
  onLocate,
  onAgentReview,
  onCutReview,
  onTranscriptionReview,
  onOpenEditor,
  onTogglePlayback,
  onSeekDelta,
  onExit,
}: {
  project: Project;
  transcriptionReviews: TranscriptionReviewItem[];
  audioRisks: AudioRisk[];
  busy: boolean;
  error: string | null;
  onLocate: (item: ReviewQueueItem) => void;
  onAgentReview: (item: ReviewQueueItem, action: "apply" | "keep") => void;
  onCutReview: (item: ReviewQueueItem, action: "apply" | "dismiss") => void;
  onTranscriptionReview: (item: ReviewQueueItem, action: "resolved" | "ignored") => void;
  onOpenEditor: (item: ReviewQueueItem) => void;
  onTogglePlayback: () => void;
  onSeekDelta: (delta: number) => void;
  onExit: () => void;
}) {
  const queue = useMemo(() => deriveReviewQueue(project, transcriptionReviews, audioRisks), [audioRisks, project, transcriptionReviews]);
  const [index, setIndex] = useState(0);
  const [collapsed, setCollapsed] = useState(false);
  const previousItemId = useRef<string | null>(null);
  const current = queue[Math.min(index, Math.max(0, queue.length - 1))] ?? null;

  const move = (direction: -1 | 1) => {
    if (!queue.length)
      return;
    const nextIndex = (index + direction + queue.length) % queue.length;
    setIndex(nextIndex);
    onLocate(queue[nextIndex]);
  };

  useEffect(() => {
    if (!queue.length) {
      setIndex(0);
      previousItemId.current = null;
      return;
    }
    const previousId = previousItemId.current;
    if (previousId && !queue.some((item) => item.id === previousId)) {
      const nextIndex = Math.min(index, queue.length - 1);
      setIndex(nextIndex);
      onLocate(queue[nextIndex]);
    }
    previousItemId.current = current?.id ?? null;
  }, [current?.id, index, onLocate, queue]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      const editing = target instanceof HTMLElement && (target.isContentEditable || target.matches("input, textarea, select"));
      if (event.key === "Escape") {
        event.preventDefault();
        onExit();
        return;
      }
      if (editing || event.ctrlKey || event.metaKey || event.altKey)
        return;
      const key = event.key.toLowerCase();
      if (event.code === "Space") {
        event.preventDefault();
        onTogglePlayback();
      }
      else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        onSeekDelta((event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 0.1 : 1));
      }
      else if (key === "p" || key === "n") {
        event.preventDefault();
        move(key === "p" ? -1 : 1);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  });

  if (!current)
    return <section className="focus-review-panel complete" aria-label={tr("app.focusReview.panel")}><CircleAlert size={20}/><strong>{tr("app.focusReview.complete")}</strong><button type="button" onClick={onExit}>{tr("app.focusReview.exit")}</button></section>;

  const risk = current.kind === "audio" ? audioRisks.find((item) => `${item.kind}:${item.start}:${item.end}` === current.sourceId) : null;
  return <section className={`focus-review-panel${collapsed ? " collapsed" : ""}`} aria-label={tr("app.focusReview.panel")}>
    <header><span><small>{tr("app.focusReview.position", { current: index + 1, total: queue.length })}</small><strong>{itemTitle(current)}</strong></span><button type="button" aria-label={collapsed ? tr("app.focusReview.expandPanel") : tr("app.focusReview.collapsePanel")} aria-expanded={!collapsed} onClick={() => setCollapsed((value) => !value)}>{collapsed ? <PanelRightOpen size={16}/> : <PanelRightClose size={16}/>}</button></header>
    {!collapsed && <>
      <div className="focus-review-range"><span>{formatTime(current.start)} — {formatTime(current.end)}</span><button type="button" onClick={() => onLocate(current)}>{tr("app.focusReview.locate")}</button></div>
      <p>{risk ? `${audioRiskLabel(risk.kind)} · ${risk.measuredValue} ${risk.unit}` : current.detail}</p>
      {current.conflict && <div className="focus-review-warning"><CircleAlert size={14}/>{tr("app.focusReview.conflict")}</div>}
      {error && <div className="focus-review-error" role="alert"><CircleAlert size={14}/><span>{error}</span></div>}
      <div className="focus-review-actions">
        {current.kind === "agent" && <><button type="button" disabled={busy || current.conflict} onClick={() => onAgentReview(current, "apply")}>{tr("app.s0584")}</button><button type="button" disabled={busy} onClick={() => onAgentReview(current, "keep")}>{tr("app.s0583")}</button></>}
        {current.kind === "cut" && <><button type="button" disabled={busy} onClick={() => onCutReview(current, "apply")}>{tr("app.s0305")}</button><button type="button" disabled={busy} onClick={() => onCutReview(current, "dismiss")}>{tr("app.cut.dismiss")}</button></>}
        {current.kind === "transcription" && <><button type="button" disabled={busy} onClick={() => onTranscriptionReview(current, "resolved")}>{tr("app.focusReview.markHandled")}</button><button type="button" disabled={busy} onClick={() => onTranscriptionReview(current, "ignored")}>{tr("app.focusReview.ignore")}</button></>}
        {(current.kind === "quality" || current.kind === "audio") && <button type="button" disabled={busy} onClick={() => onOpenEditor(current)}>{tr("app.focusReview.openEditor")}</button>}
      </div>
      <footer><button type="button" aria-label={tr("app.focusReview.previous")} onClick={() => move(-1)}><ChevronLeft size={15}/>{tr("app.focusReview.previous")}</button><button type="button" aria-label={tr("app.focusReview.next")} onClick={() => move(1)}>{tr("app.focusReview.next")}<ChevronRight size={15}/></button></footer>
    </>}
  </section>;
}
