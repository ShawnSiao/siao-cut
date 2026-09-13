import { runCoreStructured } from "../core";
import type { AiSendSpec } from "../generated/core-contract";

export const aiApprovalClient = {
  preview: async (spec: AiSendSpec) => {
    const result = await runCoreStructured({ kind: "ai_approval", request: { action: "preview", spec } });
    if (!result.aiSendPreview) throw new Error("Core 未返回发送预检，请检查版本。");
    return result.aiSendPreview;
  },
  execute: (approvalId: string) => runCoreStructured({ kind: "ai_approval", request: { action: "execute", approvalId } }),
};
