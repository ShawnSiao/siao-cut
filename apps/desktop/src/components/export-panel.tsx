import { forwardRef, useEffect, useState } from "react";
import { CircleAlert, Download, Film, ShieldCheck, X } from "lucide-react";
import { tr } from "../i18n";
import type { CanvasSettings, Project } from "../types";
import { Button, IconButton } from "./ui";

type Props = {
  embedded?: boolean;
  project: Project;
  busy: boolean;
  subtitleDelivery: "burned" | "embedded-mp4" | "embedded-mkv" | "sidecar-srt" | "sidecar-vtt";
  subtitleMode: "source" | "translated" | "bilingual";
  translationLanguageOptions: string[];
  translationLanguages: string[];
  selectedSubtitleLanguage: string;
  selectedTranslationPending: boolean;
  selectedTranslationStale: boolean;
  confirmStaleTranslation: boolean;
  confirmUncutExport: boolean;
  exportFormat: "srt" | "vtt" | "ass" | "markdown" | "json";
  structuredExport: boolean;
  includeSpeakerLabels: boolean;
  transcriptionExportErrorCount: number;
  transcriptionExportWarningCount: number;
  confirmTranscriptionWarnings: boolean;
  showSubtitleSafeArea: boolean;
  transcriptionExportBlocked: boolean;
  canExportVideo: boolean;
  activeExportRunning: boolean;
  mediaCapabilityTitle?: string;
  onClose: () => void;
  onChangeCanvas: (settings: CanvasSettings) => void;
  onSubtitleModeChange: (mode: Props["subtitleMode"]) => void;
  onSubtitleDeliveryChange: (delivery: Props["subtitleDelivery"]) => void;
  onSubtitleLanguageChange: (language: string) => void;
  onExportFormatChange: (format: Props["exportFormat"]) => void;
  onIncludeSpeakerLabelsChange: (include: boolean) => void;
  onConfirmWarningsChange: (confirmed: boolean) => void;
  onConfirmStaleTranslationChange: (confirmed: boolean) => void;
  onConfirmUncutExportChange: (confirmed: boolean) => void;
  onSubtitleStyleChange: (preset: Project["subtitleStyle"]["preset"], position: Project["subtitleStyle"]["position"], sourceFontSize?: number, translationFontSize?: number) => void;
  onShowSafeAreaChange: (show: boolean) => void;
  onExportTranscript: () => void;
  onExportVideo: () => void;
};

function FontSizeInput({ label, value, disabled, onCommit }: { label: string; value: number; disabled: boolean; onCommit: (value: number) => void }) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const next = Number(draft);
    if (!Number.isInteger(next) || next < 24 || next > 120) {
      setDraft(String(value));
      return;
    }
    if (next !== value) onCommit(next);
  };
  return <label className="subtitle-font-size"><span>{label}</span><span><input type="number" min="24" max="120" step="1" value={draft} disabled={disabled} aria-label={label} onChange={(event) => setDraft(event.target.value)} onBlur={commit} onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}/><i>px</i></span></label>;
}

const ExportPanel = forwardRef<HTMLElement, Props>(function ExportPanel(props, ref) {
  const { embedded = false, project, busy, subtitleDelivery, subtitleMode, translationLanguageOptions, translationLanguages, selectedSubtitleLanguage, selectedTranslationPending, selectedTranslationStale, confirmStaleTranslation, confirmUncutExport, exportFormat, structuredExport, includeSpeakerLabels, transcriptionExportErrorCount, transcriptionExportWarningCount, confirmTranscriptionWarnings, showSubtitleSafeArea, transcriptionExportBlocked, canExportVideo, activeExportRunning, mediaCapabilityTitle, onClose, onChangeCanvas, onSubtitleDeliveryChange, onSubtitleModeChange, onSubtitleLanguageChange, onExportFormatChange, onIncludeSpeakerLabelsChange, onConfirmWarningsChange, onConfirmStaleTranslationChange, onConfirmUncutExportChange, onSubtitleStyleChange, onShowSafeAreaChange, onExportTranscript, onExportVideo } = props;
  const selectedTranslationUnavailable = subtitleMode !== "source" && !selectedSubtitleLanguage;
  const uncutVideo = project.timeline.cuts.length === 0;
  return <aside ref={ref} className={`export-panel ${embedded ? "embedded" : ""}`} aria-label={tr("app.s0392")}>
    <header className="export-panel-header"><div><p className="eyebrow">{tr("app.s0393")}</p><h2>{tr("app.s0392")}</h2></div><IconButton label={tr("app.s0394")} onClick={onClose}><X size={17}/></IconButton></header>
    <div className="export-panel-body">
      <section className="export-group" aria-labelledby="export-canvas-heading">
        <div><h3 id="export-canvas-heading">{tr("app.s0395")}</h3><p>{tr("app.s0396")}</p></div>
        <label><span>{tr("app.s0397")}</span><select aria-label={tr("app.s0397")} disabled={busy} value={project.canvasSettings.aspectRatio} onChange={(event) => onChangeCanvas({ ...project.canvasSettings, aspectRatio: event.target.value as CanvasSettings["aspectRatio"] })}><option value="source">{tr("app.s0398")}</option><option value="9:16">{tr("app.s0399")}</option></select></label>
        <label><span>{tr("app.s0400")}</span><select aria-label={tr("app.s0400")} disabled={busy || project.canvasSettings.aspectRatio === "source"} value={project.canvasSettings.framing} onChange={(event) => onChangeCanvas({ ...project.canvasSettings, framing: event.target.value as CanvasSettings["framing"] })}><option value="contain-blur">{tr("app.s0401")}</option><option value="cover-center">{tr("app.s0402")}</option></select></label>
      </section>
      <section className="export-group" aria-labelledby="export-subtitle-heading">
        <div><h3 id="export-subtitle-heading">{tr("app.s0175")}</h3><p>{tr("app.s0403")}</p></div>
        <label><span>{tr("app.creator.export.subtitleDelivery")}</span><select aria-label={tr("app.creator.export.subtitleDelivery")} value={subtitleDelivery} onChange={(event) => onSubtitleDeliveryChange(event.target.value as Props["subtitleDelivery"])}><option value="burned">{tr("app.creator.export.delivery.burned")}</option><option value="embedded-mp4">{tr("app.creator.export.delivery.embeddedMp4")}</option><option value="embedded-mkv">{tr("app.creator.export.delivery.embeddedMkv")}</option><option value="sidecar-srt">{tr("app.creator.export.delivery.sidecarSrt")}</option><option value="sidecar-vtt">{tr("app.creator.export.delivery.sidecarVtt")}</option></select></label>
        <p className="runtime-disclosure">{tr(`app.creator.export.delivery.${subtitleDelivery === "burned" ? "burnedHelp" : subtitleDelivery === "embedded-mp4" ? "embeddedMp4Help" : subtitleDelivery === "embedded-mkv" ? "embeddedMkvHelp" : "sidecarHelp"}`)}</p>
        <label><span>{tr("app.s0404")}</span><select aria-label={tr("app.s0405")} value={subtitleMode} onChange={(event) => onSubtitleModeChange(event.target.value as Props["subtitleMode"])}><option value="source">{tr("app.s0406")}</option><option value="translated">{tr("app.s0407")}</option><option value="bilingual">{tr("app.s0408")}</option></select></label>
        <label><span>{tr("app.s0409")}</span><select aria-label={tr("app.s0409")} disabled={!translationLanguageOptions.length} value={selectedSubtitleLanguage} onChange={(event) => onSubtitleLanguageChange(event.target.value)}>{translationLanguageOptions.length ? translationLanguageOptions.map((language) => <option value={language} key={language}>{language.toUpperCase()}{translationLanguages.includes(language) ? "" : tr("app.s0410")}</option>) : <option value="">{tr("app.s0411")}</option>}</select></label>
        <label><span>{tr("app.s0412")}</span><select aria-label={tr("app.s0413")} value={exportFormat} onChange={(event) => onExportFormatChange(event.target.value as Props["exportFormat"])}><option value="srt">SRT</option><option value="vtt">VTT</option><option value="ass">ASS</option><option value="markdown">Markdown</option><option value="json">JSON</option></select></label>
        {selectedTranslationUnavailable && <p className="export-warning" role="alert"><CircleAlert size={14}/>{tr("app.s0176")}</p>}
        {selectedTranslationPending && <p className="export-warning"><CircleAlert size={14}/>{selectedSubtitleLanguage.toUpperCase()}{tr("app.s0414")}</p>}
        {selectedTranslationStale && !selectedTranslationPending && <label className="subtitle-safe-toggle export-confirm"><input type="checkbox" checked={confirmStaleTranslation} onChange={(event) => onConfirmStaleTranslationChange(event.target.checked)}/><span>{tr("app.creator.export.confirmStale", { language: selectedSubtitleLanguage.toUpperCase() })}</span></label>}
        {structuredExport && <>
          <label className="subtitle-safe-toggle"><input type="checkbox" checked={includeSpeakerLabels} onChange={(event) => onIncludeSpeakerLabelsChange(event.target.checked)}/><span>{tr("app.moss.export.includeSpeakers")}</span></label>
          {transcriptionExportErrorCount > 0 && <p className="export-warning error" role="alert"><CircleAlert size={14}/>{tr("app.moss.export.errorsBlock", { count: transcriptionExportErrorCount })}</p>}
          {transcriptionExportWarningCount > 0 && <label className="subtitle-safe-toggle export-confirm"><input type="checkbox" checked={confirmTranscriptionWarnings} disabled={transcriptionExportErrorCount > 0} onChange={(event) => onConfirmWarningsChange(event.target.checked)}/><span>{tr("app.moss.export.confirmWarnings", { count: transcriptionExportWarningCount })}</span></label>}
          <p className="runtime-disclosure">{tr("app.moss.export.evidence")}</p>
        </>}
      </section>
      {subtitleDelivery === "burned" && <section className="export-group subtitle-style-group" aria-labelledby="export-subtitle-style-heading">
        <div><h3 id="export-subtitle-style-heading">{tr("app.s0415")}</h3><p>{tr("app.s0416")}</p></div>
        <label><span>{tr("app.s0417")}</span><select aria-label={tr("app.s0418")} disabled={busy} value={project.subtitleStyle.preset} onChange={(event) => onSubtitleStyleChange(event.target.value as Project["subtitleStyle"]["preset"], project.subtitleStyle.position)}><option value="compact">{tr("app.s0419")}</option><option value="standard">{tr("app.s0420")}</option><option value="emphasis">{tr("app.s0421")}</option></select></label>
        <label><span>{tr("app.s0422")}</span><select aria-label={tr("app.s0422")} disabled={busy} value={project.subtitleStyle.position} onChange={(event) => onSubtitleStyleChange(project.subtitleStyle.preset, event.target.value as Project["subtitleStyle"]["position"])}><option value="bottom">{tr("app.s0423")}</option><option value="center">{tr("app.s0424")}</option></select></label>
        <div className="subtitle-font-size-controls"><FontSizeInput label={tr("app.creator.export.sourceFontSize")} value={project.subtitleStyle.fontSize} disabled={busy} onCommit={(size) => onSubtitleStyleChange(project.subtitleStyle.preset, project.subtitleStyle.position, size, project.subtitleStyle.secondaryFontSize)}/><FontSizeInput label={tr("app.creator.export.translationFontSize")} value={project.subtitleStyle.secondaryFontSize} disabled={busy} onCommit={(size) => onSubtitleStyleChange(project.subtitleStyle.preset, project.subtitleStyle.position, project.subtitleStyle.fontSize, size)}/></div>
        <p className="runtime-disclosure">{tr("app.creator.export.translationEmphasis")}</p>
        <label className="subtitle-safe-toggle"><input type="checkbox" checked={showSubtitleSafeArea} onChange={(event) => onShowSafeAreaChange(event.target.checked)}/><span>{tr("app.s0425")}</span></label>
        <p className="runtime-disclosure">{tr("app.creator.export.karaoke")}</p>
        <div className="subtitle-style-summary compact"><span><strong>{project.subtitleStyle.outlineWidth} px</strong><small>{tr("app.s0428")}</small></span><span><strong>{project.subtitleStyle.safeMarginPercent}%</strong><small>{tr("app.s0429")}</small></span></div>
      </section>}
      {uncutVideo && <section className="export-group export-uncut-warning" aria-label={tr("app.creator.export.uncutWarning")}>
        <p className="export-warning"><CircleAlert size={14}/>{tr("app.creator.export.uncutWarning")}</p>
        <label className="subtitle-safe-toggle export-confirm"><input type="checkbox" checked={confirmUncutExport} onChange={(event) => onConfirmUncutExportChange(event.target.checked)}/><span>{tr("app.creator.export.confirmUncut")}</span></label>
      </section>}
      <section className="export-safety"><ShieldCheck size={17}/><span><strong>{tr("app.s0430")}</strong><small>{tr("app.s0431")}</small></span></section>
    </div>
    <footer className="export-panel-actions">
      <Button disabled={busy || selectedTranslationUnavailable || selectedTranslationPending || (selectedTranslationStale && !confirmStaleTranslation) || transcriptionExportBlocked} onClick={onExportTranscript}><Download size={15}/>{tr("app.s0432")}</Button>
      <Button variant="primary" disabled={!canExportVideo || busy || selectedTranslationUnavailable || selectedTranslationPending || (selectedTranslationStale && !confirmStaleTranslation) || (uncutVideo && !confirmUncutExport) || activeExportRunning} title={mediaCapabilityTitle} onClick={onExportVideo}><Film size={15}/>{tr("app.s0259")}</Button>
    </footer>
  </aside>;
});

export default ExportPanel;
