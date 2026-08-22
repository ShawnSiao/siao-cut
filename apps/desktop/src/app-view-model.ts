import { tr } from "./i18n";
import type { AudioRisk, ModelStatus, Project, SubtitleQualityIssue, TranscriptionLanguage } from "./types";
type HumanState = string;
export type SegmentSelectionMode = "replace" | "toggle" | "range";
export type StructureEditMode = "split" | "merge" | "timing" | "offset";
export const structureEditLabel = (mode: StructureEditMode) => ({ split: tr("app.s0005"), merge: tr("app.s0006"), timing: tr("app.s0007"), offset: tr("app.s0008") })[mode];
export type ProjectCapabilities = {
    hasProject: boolean;
    hasBoundMedia: boolean;
    hasAuthorizedPreview: boolean;
    hasTranscript: boolean;
    hasWordTiming: boolean;
    hasModel: boolean;
    hasTranslationTarget: boolean;
    canRelinkMedia: boolean;
    canTranscribe: boolean;
    canAnalyzeAudio: boolean;
    canPreparePreview: boolean;
    canExportVideo: boolean;
    canCreateAgentTask: boolean;
    canDetectSuggestions: boolean;
};
export const getProjectCapabilities = (project: Project | null, options: {
    mediaUrl?: string | null;
    modelPath?: string | null;
    modelAvailable?: boolean;
    translationTarget?: string | null;
    agentWorkflowKind?: "polish" | "proofread" | "edit" | "translate" | "punctuate" | "speaker_names";
} = {}): ProjectCapabilities => {
    const hasProject = Boolean(project);
    const hasBoundMedia = Boolean(project?.media.sourcePath.trim());
    const hasAuthorizedPreview = Boolean(options.mediaUrl);
    const hasTranscript = Boolean(project?.transcript.segments.length);
    const hasWordTiming = Boolean(project?.transcript.words.length);
    const hasModel = options.modelAvailable ?? Boolean(options.modelPath?.trim());
    const hasTranslationTarget = Boolean(options.translationTarget?.trim());
    const translationReady = options.agentWorkflowKind !== "translate" || hasTranslationTarget;
    return {
        hasProject,
        hasBoundMedia,
        hasAuthorizedPreview,
        hasTranscript,
        hasWordTiming,
        hasModel,
        hasTranslationTarget,
        canRelinkMedia: hasProject,
        canTranscribe: hasBoundMedia && hasModel,
        canAnalyzeAudio: hasBoundMedia,
        canPreparePreview: hasBoundMedia,
        canExportVideo: hasBoundMedia,
        canCreateAgentTask: hasBoundMedia && hasTranscript && translationReady,
        canDetectSuggestions: hasWordTiming,
    };
};
export const isHttpsSourceUrl = (value: string) => {
    try {
        const url = new URL(value.trim());
        return url.protocol === "https:" && Boolean(url.hostname);
    }
    catch {
        return false;
    }
};
export const hasMeaningfulSubtitleText = (value: string) => /[\p{L}\p{N}\p{S}]/u.test(value);
export const segmentCountLabel = (count: number) => tr(count === 1 ? "app.count.segment.one" : "app.count.segment.other", { count });
export const wordCountLabel = (count: number) => tr(count === 1 ? "app.count.word.one" : "app.count.word.other", { count });
export const subtitleCountLabel = (count: number) => tr(count === 1 ? "app.count.subtitle.one" : "app.count.subtitle.other", { count });
export type ExportPreferencesV1 = {
    version: 1;
    subtitleMode: "source" | "translated" | "bilingual";
    subtitleLanguage: string;
    transcriptFormat: "srt" | "vtt" | "ass" | "markdown" | "json";
};
export const DEFAULT_EXPORT_PREFERENCES: ExportPreferencesV1 = {
    version: 1,
    subtitleMode: "source",
    subtitleLanguage: "en",
    transcriptFormat: "srt",
};
export const TRANSCRIPTION_LANGUAGE_STORAGE_KEY = "siaocut.transcriptionLanguage.v1";
export const parseTranscriptionLanguage = (raw: string | null): TranscriptionLanguage => ["auto", "en", "zh"].includes(raw ?? "") ? raw as TranscriptionLanguage : "auto";
export const parseExportPreferences = (raw: string | null): ExportPreferencesV1 => {
    if (!raw)
        return DEFAULT_EXPORT_PREFERENCES;
    try {
        const candidate = JSON.parse(raw) as Partial<ExportPreferencesV1>;
        const subtitleModes = ["source", "translated", "bilingual"];
        const transcriptFormats = ["srt", "vtt", "ass", "markdown", "json"];
        if (candidate.version !== 1 || !subtitleModes.includes(candidate.subtitleMode ?? "") || !transcriptFormats.includes(candidate.transcriptFormat ?? ""))
            return DEFAULT_EXPORT_PREFERENCES;
        return {
            version: 1,
            subtitleMode: candidate.subtitleMode as ExportPreferencesV1["subtitleMode"],
            subtitleLanguage: typeof candidate.subtitleLanguage === "string" ? candidate.subtitleLanguage : "en",
            transcriptFormat: candidate.transcriptFormat as ExportPreferencesV1["transcriptFormat"],
        };
    }
    catch {
        return DEFAULT_EXPORT_PREFERENCES;
    }
};
export const formatTime = (seconds: number) => {
    const minutes = Math.floor(seconds / 60);
    const rest = Math.floor(seconds % 60);
    return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
};
export const formatBytes = (bytes: number | null) => {
    if (bytes == null)
        return tr("app.s0009");
    if (bytes >= 1024 ** 3)
        return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
    return `${Math.max(0.1, bytes / 1024 ** 2).toFixed(1)} MB`;
};
export const taskLabel = (project: Project | null): HumanState => {
    if (!project)
        return tr("app.s0004");
    if (project.patchSets.some((set) => set.items.some((item) => ["pending", "conflict"].includes(item.status))))
        return tr("app.s0003");
    if (project.edits.some((edit) => ["suggested", "proposed"].includes(edit.status)))
        return tr("app.s0003");
    if (project.tasks.some((task) => ["queued", "claimed", "failed", "interrupted"].includes(task.status)))
        return tr("app.s0002");
    if (project.tasks.some((task) => task.status === "running"))
        return tr("app.s0001");
    return tr("app.s0004");
};
export const agentTaskStatusLabel = (task: Project["tasks"][number], now = Date.now()) => {
    if (task.status === "queued")
        return tr("app.agent.status.queued");
    if (task.status === "claimed")
        return tr("app.agent.status.claimed");
    const lastActivityAt = task.lastActivity?.createdAt ? Date.parse(task.lastActivity.createdAt) : Number.NaN;
    const stale = task.status === "running"
        && Number.isFinite(lastActivityAt)
        && now - lastActivityAt > 5 * 60 * 1000;
    return stale ? tr("app.agent.status.stale") : tr("app.agent.status.running");
};
export const cutSuggestionLabel = (type: string | undefined) => ({
    standalone_filler: tr("app.s0010"),
    adjacent_repetition: tr("app.s0011"),
    speech_restart: tr("app.s0012"),
}[type ?? ""] ?? tr("app.s0013"));
export const sourceStatusLabel = (status: string) => ({
    queued: tr("app.s0014"),
    running: tr("app.s0015"),
    finalizing: tr("app.s0016"),
    cancelled: tr("app.s0017"),
    interrupted: tr("app.s0018"),
    failed: tr("app.s0019"),
    completed: tr("app.s0020"),
}[status] ?? status);
export const autoStageLabel = (stage: string) => ({
    import: tr("app.s0021"),
    transcribe: tr("app.s0022"),
    suggestions: tr("app.s0023"),
    translate: tr("app.s0024"),
    review: tr("app.s0025"),
    audit: tr("app.s0026"),
    export: tr("app.s0027"),
    complete: tr("app.s0028"),
}[stage] ?? stage);
export const autoStatusLabel = (status: string) => ({
    queued: tr("app.s0029"),
    running: tr("app.s0001"),
    needs_agent: tr("app.s0002"),
    needs_review: tr("app.s0003"),
    interrupted: tr("app.s0030"),
    failed: tr("app.s0031"),
    cancelled: tr("app.s0017"),
    completed: tr("app.s0032"),
}[status] ?? status);
export const audioRiskLabel = (kind: AudioRisk["kind"]) => ({
    silence: tr("app.s0033"),
    suspected_clipping: tr("app.s0034"),
    loudness_low: tr("app.s0035"),
    loudness_high: tr("app.s0036"),
}[kind]);
export const audioUnitLabel = (unit: string) => unit === "seconds" ? tr("app.s0037") : unit;
export const subtitleIssueLabel = (kind: SubtitleQualityIssue["kind"]) => ({
    empty_text: tr("app.issue.empty_text"),
    invalid_timing: tr("app.issue.invalid_timing"),
    out_of_bounds: tr("app.issue.out_of_bounds"),
    overlap: tr("app.issue.overlap"),
    duration_too_long: tr("app.issue.duration_too_long"),
    line_too_long: tr("app.issue.line_too_long"),
    too_many_lines: tr("app.issue.too_many_lines"),
    reading_speed_high: tr("app.issue.reading_speed_high"),
    gap_too_short: tr("app.issue.gap_too_short"),
})[kind];
export const editReasonLabel = (edit: Project["edits"][number]) => {
    if (edit.suggestion?.suggestionType === "standalone_filler")
        return tr("app.reason.filler", { text: edit.reason.split(/[：:]/).at(-1)?.trim() ?? "" });
    return edit.reason;
};
export const patchReasonLabel = (reason: string) => reason === "删除不影响含义的口语冗余" ? tr("app.reason.removeRedundancy") : reason;
export const versionReasonLabel = (reason: string) => {
    const fixed = ({
        "项目创建": tr("app.reason.projectCreated"),
        "编辑原文": tr("app.reason.transcriptEdited"),
        "重新定位原片": tr("app.version.relinkedMedia"),
        "更新画布设置": tr("app.version.canvasUpdated"),
        "新增字幕段": tr("app.version.segmentAdded"),
        "whisper.cpp 本地转录": tr("app.version.localTranscription"),
        "检测粗剪建议": tr("app.version.cutSuggestions"),
        "创建词范围剪辑": tr("app.version.wordCutCreated"),
        "应用软剪辑": tr("app.version.cutApplied"),
        "保留原片": tr("app.version.cutDismissed"),
        "恢复软剪辑": tr("app.version.cutRestored"),
        "恢复全部软剪辑": tr("app.version.allCutsRestored"),
        "生成说话人轨": tr("app.version.speakerTrackGenerated"),
        "重命名说话人": tr("app.version.speakerRenamed"),
        "合并说话人": tr("app.version.speakersMerged"),
        "重新分配说话人": tr("app.version.speakerAssigned"),
        "拆分字幕段": tr("app.version.segmentSplit"),
        "合并相邻字幕段": tr("app.version.segmentsMerged"),
        "调整字幕时间": tr("app.version.timingAdjusted"),
        "更新字幕样式": tr("app.version.subtitleStyleUpdated"),
        "MOSS 多人长音频转写": tr("app.version.mossTranscription"),
    } as Record<string, string>)[reason];
    if (fixed)
        return fixed;
    const restored = reason.match(/^恢复 (.+)$/u);
    if (restored)
        return tr("app.version.restored", { version: restored[1] });
    const replaced = reason.match(/^批量替换「(.+)」$/u);
    if (replaced)
        return tr("app.version.replaced", { query: replaced[1] });
    const offset = reason.match(/^批量偏移字幕 ([+-]?[0-9.]+) 秒$/u);
    if (offset)
        return tr("app.version.offset", { delta: offset[1] });
    const imported = reason.match(/^导入 (.+) 字幕$/u);
    if (imported)
        return tr("app.version.subtitleImported", { format: imported[1].toUpperCase() });
    const agent = reason.match(/^应用 Agent 建议：(.+)$/u);
    if (agent)
        return tr("app.version.agentApplied", { reason: patchReasonLabel(agent[1]) });
    return reason;
};
export const speakerStageLabel = (stage: string) => {
    const downloaded = stage.match(/^下载组件 ([1-3])\/3$/u);
    if (downloaded)
        return tr("app.speaker.stage.downloading", { current: downloaded[1] });
    return ({
        "等待下载": tr("app.speaker.stage.waitingDownload"),
        "等待分析": tr("app.speaker.stage.waitingAnalysis"),
        "解包并校验": tr("app.speaker.stage.unpacking"),
        "准备 16 kHz 音频": tr("app.speaker.stage.preparing"),
        "本地识别说话人": tr("app.speaker.stage.identifying"),
        "建立字幕关联": tr("app.speaker.stage.associating"),
        "完成": tr("app.speaker.stage.completed"),
    } as Record<string, string>)[stage] ?? tr("app.s0564");
};
export const modelName = (model: ModelStatus) => ({
    tiny: tr("app.model.tiny.name"),
    base: tr("app.model.base.name"),
    small: tr("app.model.small.name"),
}[model.id] ?? model.name);
export const modelDescription = (model: ModelStatus) => ({
    tiny: tr("app.model.tiny.description"),
    base: tr("app.model.base.description"),
    small: tr("app.model.small.description"),
}[model.id] ?? model.description);
export const subtitleQualityStatusLabel = (quality: { status: string; errorCount: number; warningCount: number }) => {
    if (quality.errorCount > 0)
        return tr("app.composite.qualityErrors", { count: quality.errorCount });
    if (quality.warningCount > 0)
        return tr("app.composite.qualityWarnings", { count: quality.warningCount });
    return tr("app.composite.qualityGood");
};
export const shouldCheckForUpdates = (lastCheck: string | null, now: number, enabled: boolean) => {
    if (!enabled)
        return false;
    if (!lastCheck)
        return true;
    const checkedAt = Date.parse(lastCheck);
    return !Number.isFinite(checkedAt) || now - checkedAt >= 24 * 60 * 60 * 1000;
};
export const startSerialPolling = (poll: () => Promise<unknown>, intervalMs: number) => {
    let cancelled = false;
    let timer: number | null = null;
    const schedule = () => {
        if (cancelled)
            return;
        timer = window.setTimeout(() => {
            void poll().catch(() => undefined).finally(schedule);
        }, intervalMs);
    };
    schedule();
    return () => {
        cancelled = true;
        if (timer != null)
            window.clearTimeout(timer);
    };
};
export const clearTransientCoreError = (message: string | null) => message && /core_service_(?:unavailable|no_response)/.test(message) ? null : message;
