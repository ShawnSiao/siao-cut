import { activeCuts } from "./project.mjs";

function timestamp(seconds, separator = ",") {
  const ms = Math.round(seconds * 1000);
  const h = Math.floor(ms / 3600000);
  const m = Math.floor((ms % 3600000) / 60000);
  const s = Math.floor((ms % 60000) / 1000);
  const milli = ms % 1000;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}${separator}${String(milli).padStart(3, "0")}`;
}

function renderedSegments(project, { language, bilingual, includeCuts = false }) {
  const translation = language ? project.translations[language] : null;
  const translated = new Map(translation?.segments?.map((item) => [item.segmentId, item.text]) || []);
  const cuts = includeCuts ? [] : activeCuts(project).sort((a, b) => a.start - b.start);
  let removed = 0;
  return project.transcript.segments.flatMap((segment) => {
    const cut = cuts.find((item) => item.segmentId === segment.id);
    if (cut) { removed += cut.end - cut.start; return []; }
    const target = translated.get(segment.id);
    const text = bilingual && target ? `${segment.text}\n${target}` : target || segment.text;
    return [{ ...segment, start: segment.start - removed, end: segment.end - removed, text }];
  });
}

export function auditProject(project) {
  const issues = [];
  const segments = project.transcript.segments;
  segments.forEach((segment, index) => {
    if (!segment.text.trim()) issues.push({ code: "empty-caption", segmentId: segment.id });
    if (segment.end <= segment.start) issues.push({ code: "invalid-time-range", segmentId: segment.id });
    if (index > 0 && segment.start < segments[index - 1].end) issues.push({ code: "overlapping-caption", segmentId: segment.id });
  });
  for (const [language, translation] of Object.entries(project.translations)) {
    if (translation.status === "stale") issues.push({ code: "stale-translation", language });
  }
  return { ready: !issues.some((issue) => issue.code !== "stale-translation"), issues };
}

export function renderExport(project, { format = "srt", language, bilingual = false, includeCuts = false } = {}) {
  const segments = renderedSegments(project, { language, bilingual, includeCuts });
  if (format === "markdown") {
    return [`# ${project.title}`, "", ...segments.map((segment) => `- **${timestamp(segment.start, ".").slice(0, -4)}** ${segment.text}`), ""].join("\n");
  }
  if (format === "vtt") {
    return ["WEBVTT", "", ...segments.flatMap((segment) => [`${timestamp(segment.start, ".")} --> ${timestamp(segment.end, ".")}`, segment.text, ""])].join("\n");
  }
  if (format === "ass") {
    const header = "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,Alignment\nStyle: Default,Microsoft YaHei,42,&H00FFFFFF,2\n\n[Events]\nFormat: Layer,Start,End,Style,Text";
    const rows = segments.map((segment) => `Dialogue: 0,${timestamp(segment.start, ".").slice(1, -1)},${timestamp(segment.end, ".").slice(1, -1)},Default,${segment.text.replaceAll("\n", "\\N")}`);
    return `${header}\n${rows.join("\n")}\n`;
  }
  return segments.flatMap((segment, index) => [`${index + 1}`, `${timestamp(segment.start)} --> ${timestamp(segment.end)}`, segment.text, ""]).join("\n");
}
