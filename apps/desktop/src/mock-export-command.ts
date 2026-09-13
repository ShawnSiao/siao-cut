import { mockRun } from "./core.mock";
import type { ExportCommand } from "./generated/core-contract";
/** Browser simulation preserves existing export fixtures; Core verifies transaction/replay behavior. */
export function mockExportCommand(request: ExportCommand) {
  const o = request.operation;
  if (o.kind === "structured") return mockRun(["transcription","export",request.projectId,"--format",o.format,"--output",o.output,...(o.includeSpeakerLabels?["--include-speaker-labels"]:[]),...(o.confirmWarnings?["--confirm-warnings"]:[])]);
  const common = ["--output",o.output,"--subtitle-mode",o.subtitleMode,...(o.language?["--lang",o.language]:[]),...(o.allowStaleTranslation?["--confirm-stale-translation"]:[])];
  return mockRun(o.kind === "video" ? ["video","export",request.projectId,...common,"--subtitle-delivery",o.subtitleDelivery] : ["transcript","export",request.projectId,...common,"--format",o.format]);
}
