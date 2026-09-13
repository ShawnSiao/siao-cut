import "./transcription-candidate.css";
import { useEffect, useState } from "react";
import { backgroundTaskClient } from "../domains/background-task-client";
import { getUiLocale } from "../i18n";
import type { CoreEnvelope } from "../types";
import { CircleAlert, FileText, LoaderCircle, Trash2, X } from "lucide-react";
import { tr } from "../i18n";
import type { TranscriptionJob } from "../types";
import { Dialog } from "./ui";

type Props = {
  job: TranscriptionJob;
  busy: boolean;
  confirmed: boolean;
  onConfirmedChange: (confirmed: boolean) => void;
  onApply: (version: string) => void;
  onDiscard: () => void;
  onClose: () => void;
};

export default function TranscriptionCandidateDialog({ job, busy, confirmed, onConfirmedChange, onApply, onDiscard, onClose }: Props) {
  const [offset, setOffset] = useState(0);
  const [preview, setPreview] = useState<CoreEnvelope["candidatePreview"]>();
  const [error, setError] = useState<string | null>(null);
  const zh = getUiLocale() === "zh-CN";
  useEffect(() => {
    let current = true; setPreview(undefined); setError(null); onConfirmedChange(false);
    void backgroundTaskClient.previewTranscription(job.id, offset).then((result) => { if (current) setPreview(result.candidatePreview); }).catch((cause) => { if (current) setError(String(cause)); });
    return () => { current = false; };
  }, [job.id, job.candidate?.currentVersionId, offset]);
  const candidate = job.candidate;
  if (!candidate) return null;
  const duration = candidate.durationSeconds == null ? tr("app.moss.candidate.durationUnknown") : tr("app.moss.candidate.duration", { seconds: Math.round(candidate.durationSeconds) });

  return <Dialog label={tr("app.moss.candidate.dialogTitle")} className="confirm-dialog transcription-candidate-dialog" onClose={onClose}>
    <button className="dialog-close" aria-label={tr("app.moss.candidate.close")} onClick={onClose}><X size={18}/></button>
    <div className="confirm-icon warning"><FileText size={20}/></div>
    <p className="eyebrow">{tr("app.moss.candidate.eyebrow")}</p>
    <h2>{tr("app.moss.candidate.title")}</h2>
    <p className="dialog-copy">{tr("app.moss.candidate.explanation")}</p>
    <dl className="candidate-summary">
      <div><dt>{tr("app.moss.candidate.segments")}</dt><dd>{candidate.segmentCount}</dd></div>
      <div><dt>{tr("app.moss.candidate.speakers")}</dt><dd>{candidate.speakerCount}</dd></div>
      <div><dt>{tr("app.moss.candidate.length")}</dt><dd>{duration}</dd></div>
      <div><dt>{tr("app.moss.candidate.warnings")}</dt><dd>{candidate.warningCount}</dd></div>
    </dl>
    <div className="candidate-version-warning" role="note"><CircleAlert size={16}/><span>{tr("app.moss.candidate.versionWarning")}</span></div>
    <section className="candidate-text-preview" aria-label={zh ? "候选字幕文本" : "Candidate transcript text"}>
      {error ? <p role="alert">{error}</p> : preview ? <>
        <p>{zh ? `将替换 ${preview.overwrittenSegments} 段现有字幕；候选共 ${preview.total} 段。` : `Replaces ${preview.overwrittenSegments} existing segments with ${preview.total} candidate segments.`}</p>
        <div className="candidate-text-page">{preview.segments.map((segment, index) => <p key={index}><time>{segment.start.toFixed(2)}–{segment.end.toFixed(2)}</time> {segment.text}</p>)}</div>
        {preview.total > 50 && <div><button disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 50))}>{zh ? "上一页" : "Previous"}</button><button disabled={offset + 50 >= preview.total} onClick={() => setOffset(offset + 50)}>{zh ? "下一页" : "Next"}</button></div>}
      </> : <p>{zh ? "正在加载实际文本…" : "Loading candidate text…"}</p>}
    </section>
    <label className="source-consent"><input type="checkbox" disabled={!preview} checked={confirmed} onChange={(event) => onConfirmedChange(event.target.checked)}/><span>{tr("app.moss.candidate.confirmReplace")}</span></label>
    <div className="confirm-actions candidate-actions">
      <button className="button quiet" disabled={busy} onClick={onDiscard}>{busy ? <LoaderCircle className="spin" size={14}/> : <Trash2 size={14}/>}{tr("app.moss.candidate.discard")}</button>
      <button className="button primary" disabled={busy || !confirmed || !candidate.canApply || !preview} onClick={() => preview && onApply(preview.versionId)}>{busy && <LoaderCircle className="spin" size={14}/>} {tr("app.moss.candidate.apply")}</button>
    </div>
  </Dialog>;
}
