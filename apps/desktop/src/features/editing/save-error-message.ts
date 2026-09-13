import type { DraftState } from "./editing-session";

/** Persistence claims refer to the current draft revision, never just the last write. */
export function saveErrorMessage(state: Pick<DraftState, "errorCode" | "error" | "journaled">, zh: boolean) {
  if (state.errorCode !== "database_busy") return state.error;
  if (zh) return state.journaled
    ? "数据库暂时被占用，本次修改尚未保存到项目。本地草稿已落盘，请稍后重试。"
    : "数据库暂时被占用，本次修改尚未保存到项目。当前草稿尚未确认落盘，请保持应用开启并稍后重试。";
  return state.journaled
    ? "The database is busy. This change has not been saved to the project. The local draft is stored; retry shortly."
    : "The database is busy. This change has not been saved to the project. The current draft is not confirmed on disk; keep the app open and retry shortly.";
}
