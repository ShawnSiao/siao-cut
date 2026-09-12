import { useState, useSyncExternalStore } from "react";
import { getUiLocale } from "../../i18n";
import { Dialog } from "../../components/ui";
import type { EditingSession, SaveStatus } from "./editing-session";
import { saveErrorMessage } from "./save-error-message";
import "./editing.css";

export function EditingStatus({ session, projectId, closeError, onCancelClose, onCloseWithDrafts }: { session: EditingSession; projectId?: string; closeError: string | null; onCancelClose(): void; onCloseWithDrafts(): Promise<void> }) {
  useSyncExternalStore(session.subscribe, session.snapshot);
  const zh = getUiLocale() === "zh-CN";
  const [actionError, setActionError] = useState<string | null>(null);
  const entries = session.entries(projectId), dirty = entries.filter(([, state]) => state.status !== "saved");
  const problems = entries.filter(([, state]) => state.status === "failed" || state.status === "conflict");
  const action = (work: Promise<void>) => { setActionError(null); void work.catch((error) => setActionError(String(error))); };
  return <section className="editing-status" aria-label={zh ? "编辑保存状态" : "Editing save status"}>
    <span role="status">{dirty.length ? `${dirty.length} ${zh ? "项尚未保存" : "unsaved edits"}` : zh ? "已保存" : "Saved"}</span>
    {dirty.length > 0 && <button onClick={() => action(session.flush(projectId))}>{zh ? "立即保存" : "Save now"}</button>}
    {(problems.length > 0 || session.restoreError || actionError) && <div className="editing-problems">
    {(session.restoreError || actionError) && <p role="alert">{session.restoreError ?? actionError}</p>}
    {problems.map(([key, state]) => <details key={key} open className="editing-conflict">
      <summary>{state.status === "conflict" ? (zh ? "草稿需要核对" : "Review draft") : (zh ? "保存失败" : "Save failed")} · {state.draft.field}</summary>
      <p>{saveErrorMessage(state, zh)}</p>
      <label>{zh ? "当前内容" : "Current content"}<pre>{state.currentText}</pre></label>
      <label>{zh ? "本地草稿（可编辑后合并）" : "Local draft (edit to merge)"}<textarea value={state.draft.text} onChange={(event) => session.change(key, event.target.value)}/></label>
      {state.status === "conflict" ? <button onClick={() => action(session.keepDraft(key))}>{zh ? "以此草稿保存" : "Save this draft"}</button> : <button onClick={() => action(session.save(key))}>{zh ? "重试保存" : "Retry save"}</button>}
      <button onClick={() => action(session.discard(key))}>{zh ? "使用当前内容" : "Use current content"}</button>
    </details>)}
    </div>}
    {closeError && <Dialog label={zh ? "关闭前保存失败" : "Unable to save before closing"} className="editing-close-error" onClose={onCancelClose}><p role="alert">{closeError}</p><button onClick={() => action(session.flush().then(onCloseWithDrafts))}>{zh ? "重试并退出" : "Retry and exit"}</button><button onClick={() => action(onCloseWithDrafts())}>{zh ? "保留草稿后退出" : "Keep drafts and exit"}</button><button data-dialog-initial-focus onClick={onCancelClose}>{zh ? "继续编辑" : "Keep editing"}</button></Dialog>}
  </section>;
}

export function FieldSaveStatus({ status, journaled }: { status: SaveStatus; journaled: boolean }) {
  const zh = getUiLocale() === "zh-CN";
  const labels = zh ? { saved: "已保存", dirty: "未保存", saving: "保存中", failed: "保存失败", conflict: "存在冲突" } : { saved: "Saved", dirty: "Unsaved", saving: "Saving", failed: "Save failed", conflict: "Conflict" };
  return <small className={`field-save-status ${status}`} role="status">{labels[status]}{status !== "saved" && (journaled ? (zh ? " · 草稿已落盘" : " · Draft stored") : (zh ? " · 草稿待落盘" : " · Draft not stored"))}</small>;
}
