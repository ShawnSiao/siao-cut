import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, X } from "lucide-react";
import { Button, Dialog } from "../../components/ui";
import { getUiLocale } from "../../i18n";
import type { EditingSession } from "./editing-session";
import "./close-recovery.css";

export function CloseRecoveryDialog({ session, error, onCancel, onCloseWithDrafts }: {
  session: EditingSession; error: string; onCancel(): void; onCloseWithDrafts(): Promise<void>;
}) {
  const zh = getUiLocale() === "zh-CN";
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const running = useRef(false);
  const unsaved = session.entries().filter(([, state]) => state.status !== "saved");
  const stored = unsaved.every(([, state]) => state.journaled);
  const title = unsaved.length ? (zh ? "还有修改未保存" : "Some edits are not saved") : (zh ? "暂时无法退出" : "Unable to close the app");
  const cancel = () => { if (!running.current) onCancel(); };
  const exit = async (save: boolean) => {
    if (running.current) return;
    running.current = true; setPending(true); setFailure(null);
    try { if (save) await session.flush(); await onCloseWithDrafts(); }
    catch (cause) { setFailure(cause instanceof Error ? cause.message : String(cause)); }
    finally { running.current = false; setPending(false); }
  };
  return createPortal(<Dialog label={title} className="close-recovery-dialog" onClose={cancel}>
    <header><span className="close-recovery-icon"><AlertTriangle size={22}/></span><Button variant="ghost" aria-label={zh ? "关闭提示，继续编辑" : "Dismiss and keep editing"} disabled={pending} onClick={cancel}><X size={18}/></Button></header>
    <h2>{title}</h2>
    <p className="close-recovery-copy">{unsaved.length
      ? (zh ? `有 ${unsaved.length} 项修改尚未保存到项目。可以重试保存，或先保存本地草稿再退出。` : `${unsaved.length} edits have not been saved to the project. Retry saving, or store local drafts before exiting.`)
      : (zh ? "项目修改已保存，但窗口未能关闭。可以重试退出，或继续使用。" : "Project edits are saved, but the window could not close. Retry exiting or keep working.")}</p>
    {unsaved.length > 0 && <p className="close-recovery-state">{stored
      ? (zh ? "本地草稿已落盘，下次打开可恢复。" : "Local drafts are stored and can be recovered next time.")
      : (zh ? "部分草稿尚未确认落盘。草稿保存成功后才会退出。" : "Some drafts are not confirmed on disk. The app will only exit after storing them.")}</p>}
    {failure && <p className="close-recovery-failure" role="alert">{zh ? "操作未完成，请继续编辑或稍后重试。" : "The operation did not complete. Keep editing or retry shortly."}</p>}
    <details className="close-recovery-details"><summary>{zh ? "查看错误详情" : "Error details"}</summary><pre>{failure ?? error}</pre></details>
    <footer aria-busy={pending}>
      <Button data-dialog-initial-focus disabled={pending} onClick={cancel}>{zh ? "继续编辑" : "Keep editing"}</Button>
      {unsaved.length > 0 && <Button disabled={pending} onClick={() => void exit(false)}>{zh ? "保存草稿并退出" : "Store drafts and exit"}</Button>}
      <Button variant="primary" disabled={pending} onClick={() => void exit(true)}>{pending ? (zh ? "正在处理…" : "Working…") : unsaved.length ? (zh ? "重试保存并退出" : "Retry save and exit") : (zh ? "重试退出" : "Retry exit")}</Button>
    </footer>
  </Dialog>, document.body);
}
