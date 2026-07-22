const I18N = {
  zh: {
    importButton: "导入素材",
    projects: "项目",
    projectCurrent: "发布口播",
    projectDraft: "草稿 · 刚刚保存",
    projectSecond: "五月复盘",
    projectReview: "待审阅",
    deviceReady: "本机就绪",
    coreReady: "编辑核心",
    transcribeReady: "本机转写",
    available: "可用",
    optional: "可选",
    advancedSettings: "高级设置",
    language: "界面语言",
    privacy: "素材只保存在本机",
    projectType: "单源口播 · 本地项目",
    saved: "已保存",
    saving: "保存中",
    undo: "撤销",
    more: "更多",
    prototypeState: "原型状态",
    stateSetup: "首次导入",
    stateReview: "文稿审阅",
    stateExport: "导出检查",
    advancedTitle: "需要时再配置",
    advancedCopy: "默认流程只使用 CPU 转写。以下能力保留，但不占用首次使用界面。",
    mossCopy: "多人说话人转写",
    notConfigured: "未配置",
    speakerAnalysis: "说话人分析",
    speakerCopy: "匿名说话人复核",
    onDemand: "按需",
    audioAnalysis: "音频分析",
    audioCopy: "响度、静音和削波",
    manualAgent: "手工 Agent 交接",
    manualAgentCopy: "Codex 不可用时使用",
    done: "完成",
    stepPrepare: "准备素材",
    stepTranscribe: "转写",
    stepAgent: "Agent 处理",
    stepReview: "待审",
    stepExport: "可导出",
    startProcessing: "开始处理",
    reviewSuggestions: "审阅建议",
    checkExport: "检查并导出",
    importEyebrow: "首次使用",
    importTitle: "先完成一条口播，再配置高级能力。",
    importCopy: "默认方案只需要选择素材。转写在本机运行；Codex 可选，并且只处理字幕文本、时间戳、ID 和结构约束。",
    dropTitle: "拖入视频或音频",
    dropAction: "选择本地文件",
    dropHint: "支持 MP4、MOV、M4A、WAV",
    attachedTitle: "发布口播-原片.mp4",
    attachedHint: "04:38 · 1080p · 已在本机校验",
    processingPlan: "处理方案",
    recommended: "推荐",
    modelTitle: "平衡型多语言基础模型",
    modelCopy: "CPU 默认运行，可在处理期间继续编辑其他项目。",
    smallModelTitle: "轻量模型",
    smallModelCopy: "下载更小，适合低性能设备快速试用。",
    localTranscribe: "本机转写",
    localTranscribeCopy: "生成带时间戳的可编辑文稿。",
    codexProofread: "Codex 校对",
    codexProofreadCopy: "生成待审建议，不自动改写文稿。",
    translateOff: "翻译",
    translateOffCopy: "本轮默认关闭，可在转写后单独启用。",
    setupFootnote: "基础转写、编辑和导出不依赖 Codex。模型来源、许可证和哈希会在下载前显示。",
    preparation: "准备状态",
    preparationTitle: "可以开始",
    preparationCopy: "当前方案不会访问云端媒体，也不会覆盖原片。",
    sourceMedia: "源素材",
    sourceMissing: "等待选择",
    sourceReady: "已校验",
    runtime: "转写运行时",
    cpuReady: "CPU 可用",
    agentSetting: "Agent 校对",
    codexOptional: "Codex 可选",
    outputReview: "结果处理",
    humanReview: "全部人工审阅",
    attachFirst: "选择素材后即可开始处理。",
    attachedToast: "素材已在本机校验，可以开始处理。",
    processingToast: "转写与校对已完成，建议仍需人工审阅。",
    playerCollapse: "收起播放器",
    playerExpand: "展开播放器",
    sourceLabel: "发布口播-原片.mp4",
    transcript: "文稿",
    wordCount: "4 段 · 118 字",
    find: "查找",
    showTranslation: "显示译文",
    hideTranslation: "隐藏译文",
    transcriptHint: "直接编辑文字。Enter 拆分，段首 Backspace 合并，Ctrl+S 保存。",
    corrected: "已校对",
    agentSuggestion: "Agent 建议",
    qualityReview: "质量复核",
    staleTranslation: "译文过期",
    timeline: "时间线",
    timelineCollapsed: "默认收起 · 需要调时或精剪时展开",
    timelineExpand: "展开时间线",
    timelineCollapse: "收起时间线",
    drawerReview: "审阅",
    drawerQuality: "质量",
    drawerHistory: "历史",
    drawerExport: "导出",
    pendingReview: "待审建议",
    pendingReviewTitle: "逐条决定，不自动应用",
    pendingReviewCopy: "建议来自当前项目版本。应用前会再次检查版本。",
    fillerWord: "口癖",
    fillerTitle: "删除独立停顿「嗯」",
    fillerCopy: "预计缩短 0.8 秒，原片和导入版本保持不变。",
    concise: "表达精简",
    conciseTitle: "去掉重复表达",
    conciseCopy: "只修改当前段落，译文会标记为需要更新。",
    keep: "保留",
    applyDraft: "生成草稿",
    noSuggestions: "没有待审建议",
    noSuggestionsCopy: "当前版本的 Agent 建议已全部处理。",
    qualityChecks: "质量检查",
    qualityTitle: "问题集中在一个位置",
    qualityCopy: "质量问题与导出阻塞共用同一份检查结果。",
    readingSpeed: "阅读速度",
    readingSpeedCopy: "第 4 段为 22 CPS，建议缩短文字或延长时间。",
    translationState: "翻译状态",
    translationStateCopy: "第 3 段原文已修改，英文译文需要更新。",
    fixSegment: "定位此段",
    retrySegment: "重新处理此段",
    versionHistory: "版本历史",
    versionTitle: "每次修改都可以恢复",
    versionCopy: "历史只记录项目快照，不修改源素材。",
    justNow: "刚刚",
    editSaved: "文稿编辑已保存",
    minutesAgo: "3 分钟前",
    agentSubmitted: "Agent 建议已提交",
    importVersion: "导入版本",
    originalMedia: "原片未改动",
    exportReadiness: "导出就绪度",
    exportBlocked: "还不能导出",
    exportBlockedCopy: "1 个阻塞项需要处理；软剪辑提醒不会阻止导出。",
    exportReady: "可以导出",
    exportReadyCopy: "阻塞项已处理。导出会创建新文件，不覆盖原片。",
    checkMedia: "媒体与哈希",
    checkMediaCopy: "已绑定",
    checkSubtitle: "字幕时间与行数",
    checkSubtitleCopy: "通过",
    checkTranslation: "英文译文",
    checkTranslationBlock: "1 段过期",
    checkTranslationReady: "已更新",
    checkDraft: "可恢复软剪辑",
    checkDraftCopy: "1 处提醒",
    refreshTranslation: "重新处理过期段",
    exportFormat: "导出格式",
    bilingualSrt: "双语 SRT",
    markdownTranscript: "Markdown 文稿",
    captionVideo: "带字幕 MP4",
    createExport: "创建导出任务",
    exportCreated: "已创建导出任务，原片未被覆盖。",
    suggestionKept: "已保留原文。",
    draftCreated: "已生成可恢复草稿，仍需在导出前确认。",
    qualityLocated: "已定位到需要处理的段落。",
    translationUpdated: "过期译文已更新，导出阻塞已解除。",
    editChanged: "原文已更新，关联译文已标记为过期。",
    editSavedToast: "文稿已保存。",
    splitDone: "已在当前光标处拆分；时间仍需确认。",
    mergeDone: "已与上一段合并；关联译文已标记为过期。",
    findPlaceholder: "查找文稿",
    restored: "已恢复上一步原型状态。"
  },
  en: {
    importButton: "Import media", projects: "Projects", projectCurrent: "Launch video", projectDraft: "Draft · saved now", projectSecond: "May recap", projectReview: "Needs review", deviceReady: "Device ready", coreReady: "Editing core", transcribeReady: "Local transcription", available: "Available", optional: "Optional", advancedSettings: "Advanced settings", language: "Language", privacy: "Media stays on this device", projectType: "Single-source talking head · local project", saved: "Saved", saving: "Saving", undo: "Undo", more: "More", prototypeState: "Prototype state", stateSetup: "First import", stateReview: "Transcript review", stateExport: "Export check", advancedTitle: "Configure only when needed", advancedCopy: "The default flow uses CPU transcription. These tools remain available without crowding first use.", mossCopy: "Multi-speaker transcription", notConfigured: "Not set", speakerAnalysis: "Speaker analysis", speakerCopy: "Review anonymous speakers", onDemand: "On demand", audioAnalysis: "Audio analysis", audioCopy: "Loudness, silence, and clipping", manualAgent: "Manual Agent handoff", manualAgentCopy: "Fallback when Codex is unavailable", done: "Done", stepPrepare: "Prepare", stepTranscribe: "Transcribe", stepAgent: "Agent", stepReview: "Review", stepExport: "Export", startProcessing: "Start processing", reviewSuggestions: "Review suggestions", checkExport: "Check and export", importEyebrow: "First use", importTitle: "Finish one talking-head video before configuring advanced tools.", importCopy: "The default plan only asks for media. Transcription runs locally; Codex is optional and only receives subtitle text, timestamps, IDs, and structural constraints.", dropTitle: "Drop video or audio", dropAction: "Choose a local file", dropHint: "MP4, MOV, M4A, or WAV", attachedTitle: "launch-video-source.mp4", attachedHint: "04:38 · 1080p · verified locally", processingPlan: "Processing plan", recommended: "Recommended", modelTitle: "Balanced multilingual base model", modelCopy: "Runs on CPU by default while other projects remain editable.", smallModelTitle: "Compact model", smallModelCopy: "Smaller download for a quick trial on slower devices.", localTranscribe: "Local transcription", localTranscribeCopy: "Creates an editable transcript with timestamps.", codexProofread: "Codex proofreading", codexProofreadCopy: "Creates review suggestions without changing the transcript.", translateOff: "Translation", translateOffCopy: "Off by default; it can be enabled after transcription.", setupFootnote: "Core transcription, editing, and export do not depend on Codex. Source, license, and hash appear before download.", preparation: "Preparation", preparationTitle: "Ready to start", preparationCopy: "This plan does not upload media or overwrite the source.", sourceMedia: "Source media", sourceMissing: "Waiting", sourceReady: "Verified", runtime: "Transcription runtime", cpuReady: "CPU ready", agentSetting: "Agent proofreading", codexOptional: "Codex optional", outputReview: "Result handling", humanReview: "Human review required", attachFirst: "Choose media to start processing.", attachedToast: "Media was verified locally. Processing can start.", processingToast: "Transcription and proofreading finished. Suggestions still require review.", playerCollapse: "Collapse player", playerExpand: "Expand player", sourceLabel: "launch-video-source.mp4", transcript: "Transcript", wordCount: "4 segments · 92 words", find: "Find", showTranslation: "Show translation", hideTranslation: "Hide translation", transcriptHint: "Edit text directly. Enter splits, Backspace at the start merges, and Ctrl+S saves.", corrected: "Proofread", agentSuggestion: "Agent suggestion", qualityReview: "Quality review", staleTranslation: "Stale translation", timeline: "Timeline", timelineCollapsed: "Collapsed by default · open for timing or fine cuts", timelineExpand: "Expand timeline", timelineCollapse: "Collapse timeline", drawerReview: "Review", drawerQuality: "Quality", drawerHistory: "History", drawerExport: "Export", pendingReview: "Pending suggestions", pendingReviewTitle: "Decide item by item", pendingReviewCopy: "Suggestions target the current project version. The version is checked again before apply.", fillerWord: "Filler", fillerTitle: "Remove the isolated “um” pause", fillerCopy: "Shortens about 0.8 seconds without modifying the source or import version.", concise: "Conciseness", conciseTitle: "Remove repeated phrasing", conciseCopy: "Only this paragraph changes; its translation becomes stale.", keep: "Keep", applyDraft: "Create draft", noSuggestions: "No pending suggestions", noSuggestionsCopy: "All Agent suggestions for this version have been reviewed.", qualityChecks: "Quality checks", qualityTitle: "One place for every issue", qualityCopy: "Quality findings and export blockers share the same audit.", readingSpeed: "Reading speed", readingSpeedCopy: "Segment 4 is 22 CPS. Shorten the copy or extend its timing.", translationState: "Translation status", translationStateCopy: "Segment 3 changed, so its English translation needs an update.", fixSegment: "Go to segment", retrySegment: "Reprocess segment", versionHistory: "Version history", versionTitle: "Every edit can be restored", versionCopy: "History stores project snapshots without changing source media.", justNow: "Just now", editSaved: "Transcript edit saved", minutesAgo: "3 minutes ago", agentSubmitted: "Agent suggestions submitted", importVersion: "Import version", originalMedia: "Source media unchanged", exportReadiness: "Export readiness", exportBlocked: "Not ready to export", exportBlockedCopy: "Resolve one blocker. The soft-cut warning does not block export.", exportReady: "Ready to export", exportReadyCopy: "All blockers are resolved. Export creates a new file without overwriting the source.", checkMedia: "Media and hash", checkMediaCopy: "Bound", checkSubtitle: "Subtitle timing and lines", checkSubtitleCopy: "Passed", checkTranslation: "English translation", checkTranslationBlock: "1 stale segment", checkTranslationReady: "Updated", checkDraft: "Recoverable soft cut", checkDraftCopy: "1 warning", refreshTranslation: "Reprocess stale segment", exportFormat: "Export format", bilingualSrt: "Bilingual SRT", markdownTranscript: "Markdown transcript", captionVideo: "Captioned MP4", createExport: "Create export task", exportCreated: "Export task created. The source was not overwritten.", suggestionKept: "Original wording kept.", draftCreated: "Recoverable draft created. It remains reviewable before export.", qualityLocated: "Moved to the segment that needs attention.", translationUpdated: "The stale translation was updated and the export blocker is cleared.", editChanged: "Source text changed; its translation is now stale.", editSavedToast: "Transcript saved.", splitDone: "Split at the cursor. Timing still needs confirmation.", mergeDone: "Merged with the previous segment; related translation is now stale.", findPlaceholder: "Find in transcript", restored: "Restored the previous prototype state."
  }
};

const state = {
  locale: "zh",
  view: "setup",
  activeDrawer: "review",
  mediaAttached: false,
  model: "balanced",
  codexEnabled: true,
  showTranslation: true,
  playerCollapsed: false,
  timelineExpanded: false,
  selectedSegment: "s2",
  findOpen: false,
  translationResolved: false,
  suggestions: [
    { id: "filler", status: "pending" },
    { id: "concise", status: "pending" }
  ],
  segments: [
    {
      id: "s1", time: "00:00:12", statusKey: "corrected",
      zh: "今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。",
      en: "Today I want to explain why we built a local-first editing workbench.",
      translation: "Today I want to explain why we built a local-first editing workbench."
    },
    {
      id: "s2", time: "00:00:18", statusKey: "agentSuggestion",
      zh: "它不是替创作者决定内容，而是把每一次决定留在可以确认的时间线上。",
      en: "It does not make creative decisions for the editor; it keeps every decision on a reviewable timeline.",
      translation: "It does not make creative decisions for the editor; it keeps every decision on a reviewable timeline."
    },
    {
      id: "s3", time: "00:00:26", statusKey: "staleTranslation",
      zh: "建议先进入待审状态，确认后再生成可恢复的剪辑草稿。",
      en: "Suggestions enter review first, then become a recoverable editing draft after confirmation.",
      translation: "Suggestions enter review first, then become a recoverable editing draft after confirmation."
    },
    {
      id: "s4", time: "00:00:34", statusKey: "qualityReview",
      zh: "导出前还会集中检查字幕速度、翻译状态和媒体绑定是否可靠。",
      en: "Before export, SiaoCut checks caption speed, translation freshness, and media binding in one place.",
      translation: "Before export, SiaoCut checks caption speed, translation freshness, and media binding in one place."
    }
  ]
};

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];
const t = (key) => I18N[state.locale][key] || key;
const escapeHtml = (value) => String(value).replace(/[&<>'"]/g, (character) => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;"
})[character]);

function toast(message) {
  const node = $("#toast");
  node.textContent = message;
  node.classList.add("visible");
  window.clearTimeout(toast.timer);
  toast.timer = window.setTimeout(() => node.classList.remove("visible"), 2300);
}

function applyStaticTranslations() {
  document.documentElement.lang = state.locale === "zh" ? "zh-CN" : "en";
  $$('[data-i18n]').forEach((node) => { node.textContent = t(node.dataset.i18n); });
  $$('[data-i18n-title]').forEach((node) => { node.setAttribute("title", t(node.dataset.i18nTitle)); });
}

function renderFlow() {
  const stepKeys = ["stepPrepare", "stepTranscribe", "stepAgent", "stepReview", "stepExport"];
  const activeIndex = state.view === "setup" ? 0 : state.view === "review" ? 3 : 4;
  $("#flowSteps").innerHTML = stepKeys.map((key, index) => {
    const mode = index < activeIndex ? "done" : index === activeIndex ? "active" : "";
    const marker = index < activeIndex ? "✓" : String(index + 1);
    return `<li class="${mode}"><span class="step-dot">${marker}</span><span>${escapeHtml(t(key))}</span></li>`;
  }).join("");
}

function setupMarkup() {
  const attached = state.mediaAttached;
  return `
    <div class="setup-surface">
      <section class="setup-intro">
        <p class="eyebrow">${escapeHtml(t("importEyebrow"))}</p>
        <h2>${escapeHtml(t("importTitle"))}</h2>
        <p>${escapeHtml(t("importCopy"))}</p>
        <button class="drop-zone" id="dropZone" type="button">
          <b aria-hidden="true">${attached ? "✓" : "＋"}</b>
          <strong>${escapeHtml(t(attached ? "attachedTitle" : "dropTitle"))}</strong>
          <small>${escapeHtml(t(attached ? "attachedHint" : "dropAction"))}</small>
          ${attached ? "" : `<small>${escapeHtml(t("dropHint"))}</small>`}
        </button>
      </section>
      <section class="setup-options">
        <header>
          <p class="eyebrow">${escapeHtml(t("processingPlan"))}</p>
          <h2>${escapeHtml(t("recommended"))}</h2>
          <p>${escapeHtml(t("setupFootnote"))}</p>
        </header>
        <div class="option-group" role="radiogroup" aria-label="${escapeHtml(t("processingPlan"))}">
          <button class="model-choice ${state.model === "balanced" ? "selected" : ""}" data-model="balanced" type="button" role="radio" aria-checked="${state.model === "balanced"}">
            <span class="radio-mark"></span>
            <span class="choice-copy"><strong>${escapeHtml(t("modelTitle"))}</strong><small>${escapeHtml(t("modelCopy"))}</small></span>
            <span class="choice-meta">141 MB</span>
          </button>
          <button class="model-choice ${state.model === "compact" ? "selected" : ""}" data-model="compact" type="button" role="radio" aria-checked="${state.model === "compact"}">
            <span class="radio-mark"></span>
            <span class="choice-copy"><strong>${escapeHtml(t("smallModelTitle"))}</strong><small>${escapeHtml(t("smallModelCopy"))}</small></span>
            <span class="choice-meta">75 MB</span>
          </button>
        </div>
        <div class="option-group">
          <label>${escapeHtml(t("processingPlan"))}</label>
          <label class="process-choice">
            <input type="checkbox" checked disabled>
            <span class="choice-copy"><strong>${escapeHtml(t("localTranscribe"))}</strong><small>${escapeHtml(t("localTranscribeCopy"))}</small></span>
            <span class="choice-meta">CPU</span>
          </label>
          <label class="process-choice">
            <input id="codexToggle" type="checkbox" ${state.codexEnabled ? "checked" : ""}>
            <span class="choice-copy"><strong>${escapeHtml(t("codexProofread"))}</strong><small>${escapeHtml(t("codexProofreadCopy"))}</small></span>
            <span class="choice-meta">${escapeHtml(t("optional"))}</span>
          </label>
          <label class="process-choice">
            <input type="checkbox">
            <span class="choice-copy"><strong>${escapeHtml(t("translateOff"))}</strong><small>${escapeHtml(t("translateOffCopy"))}</small></span>
            <span class="choice-meta">OFF</span>
          </label>
        </div>
      </section>
    </div>`;
}

function mediaStripMarkup() {
  return `
    <section class="media-strip ${state.playerCollapsed ? "collapsed" : ""}">
      <div class="preview-mini"><button id="playButton" type="button" aria-label="Play">▶</button></div>
      <div class="media-copy"><strong>${escapeHtml(t("sourceLabel"))}</strong><p><span>00:01:12 / 00:04:38</span><span class="mini-progress"><i></i></span></p></div>
      <button class="collapse-button" id="playerToggle" type="button">${escapeHtml(t(state.playerCollapsed ? "playerExpand" : "playerCollapse"))}</button>
    </section>`;
}

function transcriptMarkup() {
  const paragraphs = state.segments.map((segment) => {
    const selected = state.selectedSegment === segment.id;
    const sourceText = state.locale === "zh" ? segment.zh : segment.en;
    const statusClass = segment.statusKey === "qualityReview" || segment.statusKey === "staleTranslation" ? "warn" : segment.statusKey === "agentSuggestion" ? "agent" : "";
    const diff = segment.id === "s2" && state.suggestions.find((item) => item.id === "concise")?.status === "pending"
      ? `<div class="inline-diff"><p>${state.locale === "zh" ? "建议：<del>每一次</del><ins>所有</ins>决定都保留在可确认的时间线上。" : "Suggestion: Keep <del>every single</del><ins>all</ins> decisions on a reviewable timeline."}</p></div>`
      : "";
    return `
      <article class="paragraph ${selected ? "selected" : ""}" data-segment="${segment.id}">
        <span class="paragraph-time">${segment.time}</span>
        <div class="paragraph-body">
          <div class="paragraph-text" contenteditable="${selected}" spellcheck="false" data-editable="${segment.id}">${escapeHtml(sourceText)}</div>
          ${state.showTranslation ? `<p class="translation-line">${escapeHtml(segment.translation)}</p>` : ""}
          ${diff}
        </div>
        <span class="paragraph-state ${statusClass}">${escapeHtml(t(segment.statusKey))}</span>
      </article>`;
  }).join("");
  return `
    <section class="transcript-shell">
      <header class="transcript-toolbar">
        <div><h2>${escapeHtml(t("transcript"))}</h2><span class="word-count">${escapeHtml(t("wordCount"))}</span></div>
        <div>
          <div class="find-bar ${state.findOpen ? "visible" : ""}"><input id="findInput" type="search" placeholder="${escapeHtml(t("findPlaceholder"))}"></div>
          <button class="toolbar-button ${state.findOpen ? "active" : ""}" id="findButton" type="button">${escapeHtml(t("find"))}</button>
          <button class="toolbar-button" id="translationButton" type="button">${escapeHtml(t(state.showTranslation ? "hideTranslation" : "showTranslation"))}</button>
        </div>
      </header>
      <div class="transcript-document" id="transcriptDocument">
        <p class="document-lede">${escapeHtml(t("transcriptHint"))}</p>
        ${paragraphs}
      </div>
    </section>`;
}

function timelineMarkup() {
  return `
    <section class="timeline-dock ${state.timelineExpanded ? "expanded" : ""}">
      <div class="timeline-summary">
        <div><strong>${escapeHtml(t("timeline"))}</strong><span>${escapeHtml(t("timelineCollapsed"))}</span></div>
        <button class="timeline-expand" id="timelineToggle" type="button" aria-label="${escapeHtml(t(state.timelineExpanded ? "timelineCollapse" : "timelineExpand"))}">${state.timelineExpanded ? "⌄" : "⌃"}</button>
      </div>
      <div class="timeline-detail"><div class="ruler"><span>00:00</span><span>01:00</span><span>02:00</span><span>03:00</span><span>04:00</span></div><div class="waveform"><i></i></div></div>
    </section>`;
}

function editorMarkup() {
  return `${mediaStripMarkup()}${transcriptMarkup()}${timelineMarkup()}`;
}

function setupDrawerMarkup() {
  return `
    <div class="drawer-body">
      <header class="drawer-heading"><div><p class="eyebrow">${escapeHtml(t("preparation"))}</p><h2>${escapeHtml(t("preparationTitle"))}</h2></div><span class="drawer-count">${state.mediaAttached ? "4/4" : "3/4"}</span></header>
      <p class="drawer-copy">${escapeHtml(t("preparationCopy"))}</p>
      <ul class="check-list">
        <li class="${state.mediaAttached ? "" : "block"}"><span class="check-dot ${state.mediaAttached ? "ok" : ""}"></span><span>${escapeHtml(t("sourceMedia"))}</span><small>${escapeHtml(t(state.mediaAttached ? "sourceReady" : "sourceMissing"))}</small></li>
        <li><span class="check-dot ok"></span><span>${escapeHtml(t("runtime"))}</span><small>${escapeHtml(t("cpuReady"))}</small></li>
        <li><span class="check-dot ${state.codexEnabled ? "ok" : "optional"}"></span><span>${escapeHtml(t("agentSetting"))}</span><small>${escapeHtml(t("codexOptional"))}</small></li>
        <li><span class="check-dot ok"></span><span>${escapeHtml(t("outputReview"))}</span><small>${escapeHtml(t("humanReview"))}</small></li>
      </ul>
      <div class="readiness-summary ${state.mediaAttached ? "ready" : ""}">
        <strong>${escapeHtml(t(state.mediaAttached ? "preparationTitle" : "sourceMissing"))}</strong>
        <p>${escapeHtml(t(state.mediaAttached ? "preparationCopy" : "attachFirst"))}</p>
      </div>
    </div>`;
}

function drawerTabsMarkup(keys) {
  const counts = { review: pendingSuggestions().length, quality: state.translationResolved ? 1 : 2, history: 3, export: state.translationResolved ? 0 : 1 };
  return `<div class="drawer-tabs">${keys.map((key) => `<button class="${state.activeDrawer === key ? "active" : ""}" type="button" data-drawer="${key}">${escapeHtml(t(`drawer${key[0].toUpperCase()}${key.slice(1)}`))}<b>${counts[key]}</b></button>`).join("")}</div>`;
}

function pendingSuggestions() {
  return state.suggestions.filter((suggestion) => suggestion.status === "pending");
}

function reviewDrawerBody() {
  const pending = pendingSuggestions();
  const cards = pending.map((suggestion) => {
    const filler = suggestion.id === "filler";
    return `<article class="drawer-card"><span class="card-tag ${filler ? "" : "info"}">${escapeHtml(t(filler ? "fillerWord" : "concise"))}</span><h3>${escapeHtml(t(filler ? "fillerTitle" : "conciseTitle"))}</h3><p>${escapeHtml(t(filler ? "fillerCopy" : "conciseCopy"))}</p><div class="drawer-actions"><button type="button" data-suggestion="${suggestion.id}" data-action="keep">${escapeHtml(t("keep"))}</button><button class="accept" type="button" data-suggestion="${suggestion.id}" data-action="apply">${escapeHtml(t("applyDraft"))}</button></div></article>`;
  }).join("");
  return `<div class="drawer-body"><header class="drawer-heading"><div><p class="eyebrow">${escapeHtml(t("pendingReview"))}</p><h2>${escapeHtml(t(pending.length ? "pendingReviewTitle" : "noSuggestions"))}</h2></div><span class="drawer-count">${pending.length}</span></header><p class="drawer-copy">${escapeHtml(t(pending.length ? "pendingReviewCopy" : "noSuggestionsCopy"))}</p>${cards}</div>`;
}

function qualityDrawerBody() {
  return `<div class="drawer-body"><header class="drawer-heading"><div><p class="eyebrow">${escapeHtml(t("qualityChecks"))}</p><h2>${escapeHtml(t("qualityTitle"))}</h2></div><span class="drawer-count">${state.translationResolved ? 1 : 2}</span></header><p class="drawer-copy">${escapeHtml(t("qualityCopy"))}</p><article class="drawer-card"><span class="card-tag">${escapeHtml(t("readingSpeed"))}</span><h3>${escapeHtml(t("readingSpeed"))}</h3><p>${escapeHtml(t("readingSpeedCopy"))}</p><div class="drawer-actions"><button type="button" data-quality="locate">${escapeHtml(t("fixSegment"))}</button></div></article>${state.translationResolved ? "" : `<article class="drawer-card"><span class="card-tag danger">${escapeHtml(t("translationState"))}</span><h3>${escapeHtml(t("translationState"))}</h3><p>${escapeHtml(t("translationStateCopy"))}</p><div class="drawer-actions"><button class="accept" type="button" data-quality="translate">${escapeHtml(t("retrySegment"))}</button></div></article>`}</div>`;
}

function historyDrawerBody() {
  return `<div class="drawer-body"><header class="drawer-heading"><div><p class="eyebrow">${escapeHtml(t("versionHistory"))}</p><h2>${escapeHtml(t("versionTitle"))}</h2></div></header><p class="drawer-copy">${escapeHtml(t("versionCopy"))}</p><article class="drawer-card"><span class="card-tag success">${escapeHtml(t("justNow"))}</span><h3>${escapeHtml(t("editSaved"))}</h3><p>v12 · 4 segments</p></article><article class="drawer-card"><span class="card-tag info">${escapeHtml(t("minutesAgo"))}</span><h3>${escapeHtml(t("agentSubmitted"))}</h3><p>v11 · 2 suggestions</p></article><article class="drawer-card"><span class="card-tag">v1</span><h3>${escapeHtml(t("importVersion"))}</h3><p>${escapeHtml(t("originalMedia"))}</p></article></div>`;
}

function exportDrawerBody() {
  const ready = state.translationResolved;
  return `<div class="drawer-body"><header class="drawer-heading"><div><p class="eyebrow">${escapeHtml(t("exportReadiness"))}</p><h2>${escapeHtml(t(ready ? "exportReady" : "exportBlocked"))}</h2></div><span class="drawer-count">${ready ? "✓" : "1"}</span></header><p class="drawer-copy">${escapeHtml(t(ready ? "exportReadyCopy" : "exportBlockedCopy"))}</p><div class="readiness-summary ${ready ? "ready" : ""}"><strong>${escapeHtml(t(ready ? "exportReady" : "exportBlocked"))}</strong><p>${escapeHtml(t(ready ? "exportReadyCopy" : "exportBlockedCopy"))}</p></div><ul class="check-list"><li><span class="check-dot ok"></span><span>${escapeHtml(t("checkMedia"))}</span><small>${escapeHtml(t("checkMediaCopy"))}</small></li><li><span class="check-dot ok"></span><span>${escapeHtml(t("checkSubtitle"))}</span><small>${escapeHtml(t("checkSubtitleCopy"))}</small></li><li class="${ready ? "" : "block"}"><span class="check-dot ${ready ? "ok" : ""}"></span><span>${escapeHtml(t("checkTranslation"))}</span><small>${escapeHtml(t(ready ? "checkTranslationReady" : "checkTranslationBlock"))}</small></li><li><span class="check-dot optional"></span><span>${escapeHtml(t("checkDraft"))}</span><small>${escapeHtml(t("checkDraftCopy"))}</small></li></ul>${ready ? "" : `<div class="drawer-actions"><button class="accept" type="button" data-quality="translate">${escapeHtml(t("refreshTranslation"))}</button></div>`}<div class="export-format-list"><label><input type="radio" name="format" checked><span>${escapeHtml(t("bilingualSrt"))}</span><small>SRT</small></label><label><input type="radio" name="format"><span>${escapeHtml(t("markdownTranscript"))}</span><small>MD</small></label><label><input type="radio" name="format"><span>${escapeHtml(t("captionVideo"))}</span><small>MP4</small></label></div><button class="primary-action" id="createExport" type="button" ${ready ? "" : "disabled"}>${escapeHtml(t("createExport"))}</button></div>`;
}

function renderDrawer() {
  const drawer = $("#drawer");
  if (state.view === "setup") {
    drawer.innerHTML = setupDrawerMarkup();
    return;
  }
  const tabs = state.view === "export" ? ["quality", "export", "history"] : ["review", "quality", "history"];
  if (!tabs.includes(state.activeDrawer)) state.activeDrawer = state.view === "export" ? "export" : "review";
  const body = state.activeDrawer === "review" ? reviewDrawerBody() : state.activeDrawer === "quality" ? qualityDrawerBody() : state.activeDrawer === "export" ? exportDrawerBody() : historyDrawerBody();
  drawer.innerHTML = `${drawerTabsMarkup(tabs)}${body}`;
}

function renderMain() {
  const main = $("#mainSurface");
  main.dataset.view = state.view;
  main.innerHTML = state.view === "setup" ? setupMarkup() : editorMarkup();
  renderDrawer();
  wireDynamicEvents();
}

function render() {
  applyStaticTranslations();
  renderFlow();
  renderMain();
  const primary = $("#primaryAction");
  primary.textContent = t(state.view === "setup" ? "startProcessing" : state.view === "review" ? "reviewSuggestions" : "checkExport");
  primary.disabled = state.view === "setup" && !state.mediaAttached;
  $$("[data-view]").forEach((button) => button.classList.toggle("active", button.dataset.view === state.view));
}

function saveFeedback(messageKey = "editSavedToast") {
  const node = $("#saveState");
  node.textContent = t("saving");
  node.classList.add("saving");
  window.setTimeout(() => {
    node.textContent = t("saved");
    node.classList.remove("saving");
    toast(t(messageKey));
  }, 300);
}

function updateSegmentFromNode(node) {
  const segment = state.segments.find((item) => item.id === node.dataset.editable);
  if (!segment) return;
  const next = node.textContent.trim();
  const key = state.locale === "zh" ? "zh" : "en";
  if (next && next !== segment[key]) {
    segment[key] = next;
    segment.statusKey = "staleTranslation";
    state.translationResolved = false;
    saveFeedback("editChanged");
  }
}

function splitSegment(node) {
  const segmentIndex = state.segments.findIndex((item) => item.id === node.dataset.editable);
  const segment = state.segments[segmentIndex];
  if (!segment) return;
  const key = state.locale === "zh" ? "zh" : "en";
  const text = node.textContent.trim();
  const selection = window.getSelection();
  const offset = selection && selection.anchorNode && node.contains(selection.anchorNode) ? selection.anchorOffset : Math.floor(text.length / 2);
  const before = text.slice(0, offset).trim();
  const after = text.slice(offset).trim();
  if (!/[\p{L}\p{N}]/u.test(before) || !/[\p{L}\p{N}]/u.test(after)) return;
  segment[key] = before;
  segment.statusKey = "staleTranslation";
  const next = { ...segment, id: `s${Date.now()}`, time: segment.time, [key]: after, statusKey: "staleTranslation" };
  state.segments.splice(segmentIndex + 1, 0, next);
  state.selectedSegment = next.id;
  state.translationResolved = false;
  renderMain();
  toast(t("splitDone"));
}

function mergeWithPrevious(node) {
  const segmentIndex = state.segments.findIndex((item) => item.id === node.dataset.editable);
  if (segmentIndex <= 0) return false;
  const key = state.locale === "zh" ? "zh" : "en";
  const previous = state.segments[segmentIndex - 1];
  const current = state.segments[segmentIndex];
  previous[key] = `${previous[key]}${state.locale === "zh" ? "" : " "}${node.textContent.trim()}`;
  previous.statusKey = "staleTranslation";
  state.segments.splice(segmentIndex, 1);
  state.selectedSegment = previous.id;
  state.translationResolved = false;
  renderMain();
  toast(t("mergeDone"));
  return true;
}

function wireDynamicEvents() {
  $("#dropZone")?.addEventListener("click", () => {
    state.mediaAttached = true;
    render();
    toast(t("attachedToast"));
  });
  $$('[data-model]').forEach((button) => button.addEventListener("click", () => { state.model = button.dataset.model; renderMain(); }));
  $("#codexToggle")?.addEventListener("change", (event) => { state.codexEnabled = event.target.checked; renderMain(); });
  $("#playerToggle")?.addEventListener("click", () => { state.playerCollapsed = !state.playerCollapsed; renderMain(); });
  $("#playButton")?.addEventListener("click", (event) => { event.currentTarget.textContent = event.currentTarget.textContent === "▶" ? "Ⅱ" : "▶"; });
  $("#timelineToggle")?.addEventListener("click", () => { state.timelineExpanded = !state.timelineExpanded; renderMain(); });
  $("#findButton")?.addEventListener("click", () => { state.findOpen = !state.findOpen; renderMain(); $("#findInput")?.focus(); });
  $("#findInput")?.addEventListener("input", (event) => {
    const query = event.target.value.trim().toLowerCase();
    $$('[data-segment]').forEach((node) => node.classList.toggle("selected", Boolean(query) && node.textContent.toLowerCase().includes(query)));
  });
  $("#translationButton")?.addEventListener("click", () => { state.showTranslation = !state.showTranslation; renderMain(); });
  $$('[data-segment]').forEach((node) => node.addEventListener("click", () => { state.selectedSegment = node.dataset.segment; renderMain(); }));
  $$('[data-editable]').forEach((node) => {
    node.addEventListener("click", (event) => event.stopPropagation());
    node.addEventListener("blur", () => updateSegmentFromNode(node));
    node.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); splitSegment(node); }
      if (event.key === "Backspace") {
        const selection = window.getSelection();
        if (selection && selection.anchorOffset === 0 && selection.isCollapsed && mergeWithPrevious(node)) event.preventDefault();
      }
    });
  });
  $$('[data-drawer]').forEach((button) => button.addEventListener("click", () => { state.activeDrawer = button.dataset.drawer; renderMain(); }));
  $$('[data-suggestion]').forEach((button) => button.addEventListener("click", () => {
    const suggestion = state.suggestions.find((item) => item.id === button.dataset.suggestion);
    if (!suggestion) return;
    suggestion.status = button.dataset.action;
    renderMain();
    toast(t(button.dataset.action === "keep" ? "suggestionKept" : "draftCreated"));
  }));
  $$('[data-quality]').forEach((button) => button.addEventListener("click", () => {
    if (button.dataset.quality === "translate") {
      state.translationResolved = true;
      const segment = state.segments.find((item) => item.id === "s3");
      if (segment) segment.statusKey = "corrected";
      renderMain();
      toast(t("translationUpdated"));
    } else {
      state.selectedSegment = "s4";
      state.view = "review";
      state.activeDrawer = "quality";
      render();
      toast(t("qualityLocated"));
    }
  }));
  $("#createExport")?.addEventListener("click", () => toast(t("exportCreated")));
}

$("#importButton").addEventListener("click", () => {
  state.view = "setup";
  state.mediaAttached = false;
  render();
});

$("#advancedButton").addEventListener("click", () => $("#advancedDialog").showModal());
$("#localeSelect").addEventListener("change", (event) => { state.locale = event.target.value; render(); });
$("#primaryAction").addEventListener("click", () => {
  if (state.view === "setup" && state.mediaAttached) {
    state.view = "review";
    state.activeDrawer = "review";
    render();
    toast(t("processingToast"));
    return;
  }
  if (state.view === "review") {
    state.activeDrawer = pendingSuggestions().length ? "review" : "quality";
    renderMain();
    return;
  }
  state.activeDrawer = "export";
  renderMain();
});
$("#undoButton").addEventListener("click", () => toast(t("restored")));
$$('[data-view]').forEach((button) => button.addEventListener("click", () => {
  state.view = button.dataset.view;
  state.activeDrawer = state.view === "export" ? "export" : "review";
  state.mediaAttached = state.view !== "setup";
  render();
}));

document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f" && state.view !== "setup") {
    event.preventDefault();
    state.findOpen = true;
    renderMain();
    $("#findInput")?.focus();
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveFeedback();
  }
});

render();
