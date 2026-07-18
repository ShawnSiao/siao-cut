import type { Project } from "./types";

export const sampleProject: Project = {
  id: "p_demo",
  title: "发布口播 · 草稿",
  createdAt: "2026-07-16T10:00:00Z",
  updatedAt: "2026-07-16T10:08:00Z",
  canvasSettings: { aspectRatio: "source", framing: "contain-blur" },
  subtitleStyle: {
    preset: "standard",
    position: "bottom",
    fontFamily: "Microsoft YaHei UI",
    bold: true,
    fontSize: 52,
    secondaryFontSize: 40,
    primaryColor: "#F2F4F5",
    secondaryColor: "#B5BEC6",
    outlineColor: "#080A0D",
    outlineWidth: 3,
    shadowDepth: 1,
    safeMarginPercent: 8,
  },
  media: { sourcePath: "", extension: ".mp4", durationSeconds: 278 },
  mediaArtifacts: null,
  timeline: {
    sourceDuration: 278,
    outputDuration: 278,
    keptRanges: [{ sourceStart: 0, sourceEnd: 278, outputStart: 0, outputEnd: 278 }],
    cuts: [],
  },
  transcript: {
    sourceLanguage: "zh",
    segments: [
      { id: "s1", start: 12.4, end: 13.2, text: "嗯，", confidence: 0.52 },
      { id: "s2", start: 13.2, end: 18.6, text: "今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。", confidence: 0.96 },
      { id: "s3", start: 18.6, end: 24.4, text: "它不是替你决定内容，而是把每一次决定留在可以确认的时间线上。", confidence: 0.94 },
      { id: "s4", start: 24.4, end: 27.2, text: "你可以，你可以先看建议，再决定是否删除。", confidence: 0.78 },
    ],
    words: [
      { id: "w1", segmentId: "s1", start: 12.4, end: 13.0, text: "嗯", confidence: 0.52 },
      { id: "w2", segmentId: "s2", start: 13.2, end: 13.6, text: "今天", confidence: 0.96 },
      { id: "w3", segmentId: "s4", start: 24.4, end: 24.9, text: "你可以", confidence: 0.89 },
      { id: "w4", segmentId: "s4", start: 25.0, end: 25.5, text: "你可以", confidence: 0.91 },
      { id: "w5", segmentId: "s4", start: 25.6, end: 27.2, text: "先看建议，再决定是否删除。", confidence: 0.92 },
    ],
  },
  subtitleQuality: {
    status: "warning",
    statusLabel: "3 项质量提醒",
    issueCount: 3,
    errorCount: 0,
    warningCount: 3,
    thresholds: { maxDurationSeconds: 8, maxLineCharacters: 42, maxCharactersPerSecond: 20, minGapSeconds: 0.12 },
    issues: [
      { id: "quality-gap-s2", kind: "gap_too_short", severity: "warning", segmentId: "s2", relatedSegmentId: "s1", start: 13.2, end: 18.6, message: "与上一条字幕间隔过短", measuredValue: 0, threshold: 0.12 },
      { id: "quality-gap-s3", kind: "gap_too_short", severity: "warning", segmentId: "s3", relatedSegmentId: "s2", start: 18.6, end: 24.4, message: "与上一条字幕间隔过短", measuredValue: 0, threshold: 0.12 },
      { id: "quality-gap-s4", kind: "gap_too_short", severity: "warning", segmentId: "s4", relatedSegmentId: "s3", start: 24.4, end: 27.2, message: "与上一条字幕间隔过短", measuredValue: 0, threshold: 0.12 },
    ],
  },
  speechInsights: {
    status: "ready",
    analyzerVersion: "rhythm-v1",
    thresholds: { pauseSeconds: 0.8, longPauseSeconds: 1.5, lowConfidence: 0.75 },
    spanDurationSeconds: 14.8,
    spokenDurationSeconds: 3.6,
    tokenCount: 5,
    tokensPerMinute: 83.3,
    pauseCount: 1,
    longPauseCount: 1,
    totalPauseDurationSeconds: 10.8,
    fillerCount: 1,
    lowConfidenceCount: 1,
    pauses: [
      { start: 13.6, end: 24.4, duration: 10.8, previousWordId: "w2", nextWordId: "w3", severity: "long_pause" },
    ],
    evidence: [
      { kind: "filler", wordId: "w1", segmentId: "s1", start: 12.4, end: 13, text: "嗯", confidence: 0.52 },
      { kind: "low_confidence", wordId: "w1", segmentId: "s1", start: 12.4, end: 13, text: "嗯", confidence: 0.52 },
    ],
  },
  translations: {
    en: {
      status: "current",
      updatedAt: "2026-07-16T10:07:00Z",
      segments: [
        { segmentId: "s2", text: "Today I want to explain why we are building a local-first editing workbench." },
      ],
    },
  },
  edits: [
    { id: "e1", kind: "word_cut", status: "proposed", segmentId: "s1", start: 12.4, end: 13.1, reason: "句内口头语：嗯", cutRange: { fromWordId: "w1", toWordId: "w1", selectedStart: 12.4, selectedEnd: 13.0, paddingMs: 100, transcriptHash: "demo", stale: false }, suggestion: { suggestionType: "standalone_filler", confidence: 0.99, detectorVersion: "heuristic-v1" } },
  ],
  tasks: [
    { id: "t1", kind: "polish", language: null, status: "queued", progress: 0, errorMessage: null, instructionLocale: "zh-CN" },
    { id: "t2", kind: "proofread", language: null, status: "review", progress: 1, errorMessage: null, instructionLocale: "zh-CN" },
  ],
  patchSets: [{
    id: "patch1", taskId: "t2", kind: "proofread", language: null, status: "pending_review",
    baseVersionId: "v2", createdAt: "2026-07-16T10:09:00Z", items: [{
      id: "pi1", segmentId: "s2", target: "transcript",
      beforeText: "今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。",
      afterText: "今天想聊聊，为什么要做一套本地优先的剪辑工作台。",
      currentText: "今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。",
      reason: "删除不影响含义的口语冗余", confidence: 0.94, status: "pending",
    }],
  }],
  workflows: [],
  versions: [
    { id: "v1", reason: "项目创建", createdAt: "2026-07-16T10:00:00Z" },
    { id: "v2", reason: "编辑原文", createdAt: "2026-07-16T10:08:00Z" },
  ],
  history: { canUndo: true, canRedo: false, currentVersionId: "v2" },
};
