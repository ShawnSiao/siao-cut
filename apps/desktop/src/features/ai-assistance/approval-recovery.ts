export function requiresNewApproval(cause: unknown) {
  const code = (cause as { code?: string } | null)?.code;
  return code === "ai_approval_stale" || code === "ai_dispatch_uncertain"
    || /^(?:Error: )?(?:ai_approval_stale|ai_dispatch_uncertain):/.test(String(cause));
}

export const approvalRecoveryMessage = "原授权已失效，或上次调用结果尚未确认。请重新核对接收方和实际发送内容；再次执行可能消耗额度。原运行记录会保留。";
