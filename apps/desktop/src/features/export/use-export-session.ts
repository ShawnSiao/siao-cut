import { useEffect,useState } from "react";
import { clearTransientCoreError,getProjectCapabilities,parseExportPreferences,type ExportPreferencesV1 } from "../../app-view-model";
import { pickTranscriptPath,pickVideoPath } from "../../domains/desktop-platform-client";
import { exportRuntimeClient } from "../../domains/export-runtime-client";
import { useBackgroundTaskRegistry } from "../../hooks/use-background-task-registry";
import { tr } from "../../i18n";
import type { ExportJob,Project,SpeakerTrack,TranscriptionReviewItem } from "../../types";

type Inputs = { getVersion: (id: string) => string | null; project: Project | null; speakerTrack: SpeakerTrack | null; transcriptionReviews: TranscriptionReviewItem[]; mediaUrl: string | null; setNotice: (message: string | null) => void; setError: import("react").Dispatch<import("react").SetStateAction<string | null>>; activeProjectIdRef: import("react").RefObject<string | null>; withBusy: (label: string, action: () => Promise<void>) => Promise<void>; flush: () => Promise<void> };
export function useExportSession({ getVersion, project, speakerTrack, transcriptionReviews, mediaUrl, setNotice, setError, activeProjectIdRef, withBusy, flush }: Inputs) {
  const [activeExport, setActiveExport] = useState<ExportJob | null>(null);
  const [exportFormat, setExportFormat] = useState<"srt" | "vtt" | "ass" | "markdown" | "json">(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).transcriptFormat);
  const [includeSpeakerLabels, setIncludeSpeakerLabels] = useState(true);
  const [confirmTranscriptionWarnings, setConfirmTranscriptionWarnings] = useState(false);
  const [confirmStaleTranslation, setConfirmStaleTranslation] = useState(false);
  const [confirmUncutExport, setConfirmUncutExport] = useState(false);
  const [subtitleDelivery, setSubtitleDelivery] = useState<ExportPreferencesV1["subtitleDelivery"]>(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).subtitleDelivery);
  const [subtitleMode, setSubtitleMode] = useState<"source" | "translated" | "bilingual">(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).subtitleMode);
  const [subtitleLanguage, setSubtitleLanguage] = useState(() => parseExportPreferences(localStorage.getItem("siaocut.exportPreferences.v1")).subtitleLanguage);
  const translationLanguages = project ? Object.keys(project.translations) : [];
  const pendingTranslationLanguages = project?.tasks
    .filter((task) => task.kind === "translate" && task.language && !["done", "completed", "cancelled", "canceled"].includes(task.status))
    .map((task) => task.language!) ?? [];
  const translationLanguageOptions = Array.from(new Set([...translationLanguages, ...pendingTranslationLanguages]));
  const selectedSubtitleLanguage = translationLanguageOptions.includes(subtitleLanguage) ? subtitleLanguage : translationLanguageOptions[0] ?? "";
  const selectedTranslation = selectedSubtitleLanguage ? project?.translations[selectedSubtitleLanguage] : undefined;
  const translation = selectedTranslation ? [selectedSubtitleLanguage, selectedTranslation] as const : undefined;
  const translatedIds = new Set(selectedTranslation?.segments.map((segment) => segment.segmentId));
  const selectedTranslationIncomplete = Boolean(selectedTranslation && project?.transcript.segments.some((source) => !translatedIds.has(source.id)));
  const selectedTranslationPending = Boolean(subtitleMode !== "source" && selectedSubtitleLanguage && (!selectedTranslation || selectedTranslationIncomplete));
  const selectedTranslationStale = Boolean(subtitleMode !== "source" && !selectedTranslationIncomplete && selectedTranslation && (
    selectedTranslation.status !== "current"
    || selectedTranslation.segments.some((segment) => segment.status !== "current")
  ));
  const transcriptionExportErrors = transcriptionReviews.filter((item) => item.status === "open" && item.severity === "error");
  const transcriptionExportWarnings = transcriptionReviews.filter((item) => item.status === "open" && item.severity === "warning");
  const structuredExport = exportFormat === "json" || (exportFormat === "markdown" && speakerTrack?.status === "ready");
  const transcriptionExportBlocked = structuredExport && (transcriptionExportErrors.length > 0 || (transcriptionExportWarnings.length > 0 && !confirmTranscriptionWarnings));
  useEffect(() => {
    localStorage.setItem("siaocut.exportPreferences.v1", JSON.stringify({
      version: 1,
      subtitleMode,
      subtitleDelivery,
      subtitleLanguage,
      transcriptFormat: exportFormat,
    } satisfies ExportPreferencesV1));
  }, [exportFormat, subtitleDelivery, subtitleLanguage, subtitleMode]);
  useEffect(() => {
    if (!project || subtitleMode === "source")
      return;
    if (translationLanguageOptions.length && !translationLanguageOptions.includes(subtitleLanguage))
      setSubtitleLanguage(translationLanguageOptions[0]);
  }, [project, subtitleLanguage, subtitleMode, translationLanguageOptions]);
  const exportTranscript = () => project && withBusy(tr("app.s0172"), async () => {
    await flush();
    const versionId = getVersion(project.id);
    if (!versionId) throw new Error("editing_version_conflict: 项目已切换，请重新确认导出");
    const output = await pickTranscriptPath(project.title, exportFormat);
    if (!output)
      return;
    if (structuredExport) {
      await exportRuntimeClient.exportStructuredTranscript(project.id, exportFormat, output, includeSpeakerLabels, confirmTranscriptionWarnings, versionId);
    }
    else {
      const subtitle = subtitleExportOptions();
      await exportRuntimeClient.exportTranscript(project.id, exportFormat, output, subtitle.mode, subtitle.language, subtitle.confirmStaleTranslation, versionId);
    }
    setNotice(tr("app.s0173", { "0": exportFormat === "markdown" ? tr("app.s0174") : exportFormat === "json" ? tr("app.moss.export.json") : tr("app.s0175"), "1": output }));
  });
  const subtitleExportOptions = () => {
    if (subtitleMode === "source")
      return { mode: "source" as const, language: undefined, confirmStaleTranslation: false };
    if (!selectedSubtitleLanguage)
      throw new Error(tr("app.s0176"));
    if (selectedTranslationPending)
      throw new Error(tr("app.s0177", { "0": selectedSubtitleLanguage.toUpperCase() }));
    return { mode: subtitleMode, language: selectedSubtitleLanguage, confirmStaleTranslation };
  };
  const exportVideo = () => project && withBusy(tr("app.s0186"), async () => {
    if (!getProjectCapabilities(project, { mediaUrl }).hasBoundMedia)
      throw new Error(tr("app.capability.mediaRequired"));
    await flush();
    const versionId = getVersion(project.id);
    if (!versionId) throw new Error("editing_version_conflict: 项目已切换，请重新确认导出");
    const output = await pickVideoPath(project.title, subtitleDelivery);
    if (!output)
      return;
    const subtitle = subtitleExportOptions();
    const envelope = await exportRuntimeClient.exportVideo(project.id, output, subtitleDelivery, subtitle.mode, subtitle.language, subtitle.confirmStaleTranslation, versionId);
    if (!envelope.job)
      throw new Error(tr("app.s0187"));
    setActiveExport(envelope.job);
    setNotice(envelope.job.status === "completed" ? tr("app.s0051", { "0": envelope.job.outputPath }) : tr("app.s0188"));
  });
  const cancelExport = () => activeExport && withBusy(tr("app.s0189"), async () => {
    const envelope = await exportRuntimeClient.cancelVideoExport(activeExport.id);
    if (envelope.job)
      setActiveExport(envelope.job);
    setNotice(tr("app.s0190"));
  });
  const retryExport = () => activeExport && withBusy(tr("app.s0191"), async () => {
    const envelope = await exportRuntimeClient.retryVideoExport(activeExport.id);
    if (!envelope.job)
      throw new Error(tr("app.s0187"));
    setActiveExport(envelope.job);
    setNotice(tr("app.s0192"));
  });
  useBackgroundTaskRegistry([
    activeExport && ["queued", "running"].includes(activeExport.status) ? {
      key: `video-export:${activeExport.id}`,
      intervalMs: 1000,
      poll: () => exportRuntimeClient.getVideoExport(activeExport.id).then((envelope) => {
        setError(clearTransientCoreError);
        if (!envelope.job)
          return;
        if (activeProjectIdRef.current === envelope.job.projectId)
          setActiveExport(envelope.job);
        if (envelope.job.status === "completed")
          setNotice(tr("app.s0051", { "0": envelope.job.outputPath }));
        if (envelope.job.status === "failed")
          setError(envelope.job.errorMessage ?? tr("app.s0052"));
        if (envelope.job.status === "cancelled")
          setNotice(tr("app.s0053"));
      }).catch((cause) => setError(cause instanceof Error ? cause.message : String(cause))),
    } : null,
  ]);
  return { activeExport, setActiveExport, exportFormat, setExportFormat, includeSpeakerLabels, setIncludeSpeakerLabels, confirmTranscriptionWarnings, setConfirmTranscriptionWarnings, confirmStaleTranslation, setConfirmStaleTranslation, confirmUncutExport, setConfirmUncutExport, subtitleDelivery, setSubtitleDelivery, subtitleMode, setSubtitleMode, subtitleLanguage, setSubtitleLanguage, translationLanguages, translationLanguageOptions, selectedSubtitleLanguage, selectedTranslation, translation, selectedTranslationPending, selectedTranslationStale, transcriptionExportErrors, transcriptionExportWarnings, structuredExport, transcriptionExportBlocked, exportTranscript, exportVideo, cancelExport, retryExport };
}
