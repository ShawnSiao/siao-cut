import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { aiApprovalClient } from "../../domains/ai-approval-client";
import type { AiSendPreview, AiSendSpec } from "../../generated/core-contract";
import { useAiSendPreview } from "./use-ai-send-preview";

const scope: AiSendSpec = { projectId: "p", expectedVersionId: "v1", kind: "polish", language: null, instructionLocale: "zh-CN", taskId: null, target: { kind: "codex" } };
const preview = (spec: AiSendSpec): AiSendPreview => ({ spec, approvalId: spec.expectedVersionId, configurationRevision: "1", payloadHash: "hash", payloadJson: "actual text", receiver: "unverified", receiverVerified: false, endpoint: null, model: null, segmentCount: 1, characterCount: 11, startTime: 0, endTime: 1 });
afterEach(() => vi.restoreAllMocks());

it("ignores a late preview from a previous project version", async () => {
  let finishOld!: (value: AiSendPreview) => void;
  const preflight = vi.spyOn(aiApprovalClient, "preview").mockImplementationOnce(() => new Promise((resolve) => { finishOld = resolve; })).mockImplementation(async (spec) => preview(spec));
  const { result, rerender } = renderHook((spec) => useAiSendPreview(spec), { initialProps: scope });
  await waitFor(() => expect(preflight).toHaveBeenCalledTimes(1));
  rerender({ ...scope, expectedVersionId: "v2" });
  expect(result.current.preview).toBeNull();
  await waitFor(() => expect(result.current.preview?.approvalId).toBe("v2"));
  await act(async () => finishOld(preview(scope)));
  expect(result.current.preview?.approvalId).toBe("v2");
});

it("clears the previous approval immediately when changing execution targets", async () => {
  vi.spyOn(aiApprovalClient, "preview").mockImplementation(async (spec) => preview(spec));
  const { result, rerender } = renderHook((spec) => useAiSendPreview(spec), { initialProps: scope });
  await waitFor(() => expect(result.current.preview).not.toBeNull());
  rerender({ ...scope, target: { kind: "api", service_config_id: "new", service_revision: 2, network_revision: 1, model_id: "changed" } });
  expect(result.current.preview).toBeNull();
  await waitFor(() => expect(result.current.preview?.spec.target.kind).toBe("api"));
});
