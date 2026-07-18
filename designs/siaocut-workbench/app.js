const state = {
  playing: false,
  selected: "s2",
  zoom: 100,
  cuts: new Set(),
  suggestions: [
    { id: "cut-1", segmentId: "s1", label: "疑似口癖", text: "“嗯”是一个独立停顿。", duration: "缩短 0.8 秒" },
    { id: "cut-2", segmentId: "s4", label: "疑似重复", text: "这句与前一句表达相近。", duration: "缩短 2.1 秒" }
  ],
  segments: [
    { id: "s1", time: "00:00:12", start: "12.4", end: "13.2", text: "嗯，", confidence: "低置信度", low: true },
    { id: "s2", time: "00:00:13", start: "13.2", end: "18.6", text: "今天想和大家聊聊，为什么要做一套本地优先的剪辑工作台。", confidence: "已校对" },
    { id: "s3", time: "00:00:18", start: "18.6", end: "24.4", text: "它不是替你决定内容，而是把每一次决定留在可以确认的时间线上。", confidence: "已校对" },
    { id: "s4", time: "00:00:24", start: "24.4", end: "26.5", text: "你可以先看建议，再决定是否删除。", confidence: "建议复核", low: true }
  ]
};

const $ = (selector) => document.querySelector(selector);
const format = (seconds) => `00:00:${String(Math.floor(Number(seconds))).padStart(2, "0")}`;

function toast(message) { const node = $("#toast"); node.textContent = message; node.classList.add("visible"); window.clearTimeout(toast.timer); toast.timer = window.setTimeout(() => node.classList.remove("visible"), 2600); }

function renderTranscript() {
  $("#transcriptList").innerHTML = state.segments.map((segment) => `
    <div class="segment ${state.selected === segment.id ? "selected" : ""}" data-id="${segment.id}">
      <span class="segment-time">${segment.time}</span>
      <div class="segment-text" contenteditable="${state.selected === segment.id}" data-id="${segment.id}">${segment.text}</div>
      <span class="segment-info ${segment.low ? "low" : ""}">${segment.confidence}</span>
    </div>`).join("");
  document.querySelectorAll(".segment").forEach((node) => node.addEventListener("click", () => selectSegment(node.dataset.id)));
  document.querySelectorAll(".segment-text").forEach((node) => node.addEventListener("blur", () => {
    const segment = state.segments.find((item) => item.id === node.dataset.id);
    if (segment && segment.text !== node.textContent.trim()) { segment.text = node.textContent.trim(); $("#translationStatus").textContent = "需要更新"; toast("原文已更新；译文需要更新。"); }
  }));
}

function renderSuggestions() {
  const visible = state.suggestions.filter((item) => !state.cuts.has(item.id));
  $("#suggestionCount").textContent = visible.length;
  $("#suggestionList").innerHTML = visible.length ? visible.map((item) => `
    <article class="suggestion"><span class="suggestion-tag">${item.label}</span><p>${item.text}</p><span class="suggestion-meta">${item.duration} · 尚未删除</span><div class="suggestion-actions"><button data-keep="${item.id}">保留</button><button data-apply="${item.id}">生成草稿</button></div></article>`).join("") : "<p class=\"suggestion\">没有待确认建议。原片仍可随时恢复。</p>";
  document.querySelectorAll("[data-keep]").forEach((button) => button.addEventListener("click", () => { state.suggestions = state.suggestions.filter((item) => item.id !== button.dataset.keep); renderSuggestions(); toast("已保留原句。") }));
  document.querySelectorAll("[data-apply]").forEach((button) => button.addEventListener("click", () => { state.cuts.add(button.dataset.apply); renderSuggestions(); renderTimeline(); $("#versionText").textContent = `已生成 ${state.cuts.size} 处可恢复草稿`; toast("已生成软剪辑草稿；原片未改动。") }));
}

function renderTimeline() {
  $("#captionTrack").innerHTML = state.segments.map((segment, index) => {
    const cut = [...state.cuts].some((cutId) => state.suggestions.find((item) => item.id === cutId)?.segmentId === segment.id);
    return `<button class="caption ${cut ? "cut" : ""}" style="--size:${index === 1 ? 4 : index === 2 ? 4 : 2}" data-caption="${segment.id}">${cut ? "待删除" : segment.text}</button>`;
  }).join("");
  document.querySelectorAll("[data-caption]").forEach((button) => button.addEventListener("click", () => selectSegment(button.dataset.caption)));
  $("#timelineNote").textContent = state.cuts.size ? `已生成 ${state.cuts.size} 处虚线草稿；导出前仍可恢复。` : "虚线标记为建议，尚未删除任何内容。";
}

function selectSegment(id) {
  state.selected = id;
  const segment = state.segments.find((item) => item.id === id);
  $("#contextTitle").textContent = segment.text;
  $("#contextTime").textContent = `${format(segment.start)} — ${format(segment.end)}`;
  $("#contextStatus").textContent = segment.low ? "建议复核" : "已校对";
  $("#currentTime").textContent = `00:00:${segment.start}`;
  $("#playTime").textContent = `00:00:${segment.start}`;
  const percent = Math.min(92, Number(segment.start) / 46 * 100);
  $("#playhead").style.left = `${percent}%`; $("#scrubPosition").style.width = `${percent}%`;
  renderTranscript();
}

function togglePlay() {
  state.playing = !state.playing;
  $("#playButton").textContent = state.playing ? "Ⅱ" : "▶";
  $("#playMini").textContent = state.playing ? "暂停" : "播放";
  $("#statusText").textContent = state.playing ? "正在预览草稿" : "本地处理完成";
}

$("#newProject").addEventListener("click", () => $("#welcomeDialog").showModal());
$("#exportButton").addEventListener("click", () => $("#exportDialog").showModal());
$("#playButton").addEventListener("click", togglePlay); $("#playMini").addEventListener("click", togglePlay); $("#previous").addEventListener("click", () => selectSegment("s1"));
$("#agentButton").addEventListener("click", () => { $("#statusText").textContent = "需要 Agent 继续"; toast("Agent 将只获得待处理的字幕文本。") });
$("#restoreButton").addEventListener("click", () => { state.cuts.clear(); renderSuggestions(); renderTimeline(); $("#versionText").textContent = "已恢复到导入版本"; toast("已恢复原片时间线。") });
$("#zoomIn").addEventListener("click", () => { state.zoom = Math.min(180, state.zoom + 20); $("#zoomLevel").textContent = `${state.zoom}%`; });
$("#zoomOut").addEventListener("click", () => { state.zoom = Math.max(60, state.zoom - 20); $("#zoomLevel").textContent = `${state.zoom}%`; });
$("#modelGrid").addEventListener("click", (event) => { const card = event.target.closest(".model-card"); if (!card) return; document.querySelectorAll(".model-card").forEach((node) => node.classList.toggle("selected", node === card)); });
$("#welcomeDialog").addEventListener("close", () => { if ($("#welcomeDialog").returnValue === "default") toast("项目已创建；可随时取消模型下载。") });
$("#exportDialog").addEventListener("close", () => { if ($("#exportDialog").returnValue === "default") toast("已创建导出任务；原片不会被覆盖。") });

renderTranscript(); renderSuggestions(); renderTimeline(); selectSegment("s2");
