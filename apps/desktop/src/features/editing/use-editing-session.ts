import { useEffect, useRef, useState } from "react";
import { editingClient } from "../../domains/editing-client";
import type { Project } from "../../types";
import type { EditReceipt } from "../../generated/core-contract";
import { EditingSession } from "./editing-session";

export function useEditingSession(project: Project | null, onSaved: (receipt: EditReceipt) => Promise<Project | void>) {
  const savedRef = useRef(onSaved); savedRef.current = onSaved;
  const [session] = useState(() => new EditingSession(editingClient, (receipt) => savedRef.current(receipt)));
  const [closeError, setCloseError] = useState<string | null>(null);
  useEffect(() => { if (project) session.observe(project); }, [project, session]);
  useEffect(() => {
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (session.entries().some(([, state]) => state.status !== "saved")) { event.preventDefault(); event.returnValue = ""; }
    };
    window.addEventListener("beforeunload", beforeUnload);
    let disposed = false, unlisten: (() => void) | undefined;
    if ("__TAURI_INTERNALS__" in window) void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      if (disposed) return;
      const win = getCurrentWindow();
      const stop = await win.onCloseRequested(async (event) => {
        event.preventDefault();
        try { await session.flush(); await win.destroy(); }
        catch (error) { setCloseError(error instanceof Error ? error.message : String(error)); }
      });
      if (disposed) stop(); else unlisten = stop;
    }).catch((error) => setCloseError(String(error)));
    return () => { disposed = true; unlisten?.(); window.removeEventListener("beforeunload", beforeUnload); session.disposeTimers(); };
  }, [session]);
  const closeWithDrafts = async () => {
    try { await session.journalAll(); const { getCurrentWindow } = await import("@tauri-apps/api/window"); await getCurrentWindow().destroy(); }
    catch (error) { setCloseError(String(error)); }
  };
  return { session, closeError, clearCloseError: () => setCloseError(null), closeWithDrafts };
}
