import type { Draft, EditReceipt, ProjectMutation, ProjectOperation, SaveEdit } from "../../generated/core-contract";
import type { CoreEnvelope, Project } from "../../types";

export type SaveStatus = "saved" | "dirty" | "saving" | "failed" | "conflict";
export type DraftState = { draft: Draft; status: SaveStatus; journaled: boolean; error: string | null; currentText: string; composing: boolean; groupId: string; request?: SaveEdit };
export type EditingTransport = {
  journal(draft: Draft): Promise<void>;
  list(projectId: string): Promise<Draft[]>;
  discard(draft: Draft): Promise<void>;
  save(edit: SaveEdit): Promise<EditReceipt>;
  load?(projectId: string): Promise<Project>;
  mutate?(mutation: ProjectMutation): Promise<CoreEnvelope>;
};
const uid = () => crypto.randomUUID();
export const fieldKey = (projectId: string, segmentId: string, field: string) => JSON.stringify([projectId, segmentId, field]);

/** One queue per project; journaling is independent from committing project versions. */
export class EditingSession {
  private readonly sessionId = uid();
  private fields = new Map<string, DraftState>();
  private versions = new Map<string, string | null>();
  private listeners = new Set<() => void>();
  private saves = new Map<string, ReturnType<typeof setTimeout>>();
  private journals = new Map<string, ReturnType<typeof setTimeout>>();
  private chains = new Map<string, Promise<void>>();
  private journalChains = new Map<string, Promise<void>>();
  private loading = new Map<string, Promise<void>>();
  private revision = 0;
  private batching = false;
  private pendingEmission = false;
  private pendingOperations = new Map<string, ProjectMutation>();
  constructor(private transport: EditingTransport, private onSaved: (receipt: EditReceipt) => Promise<Project | void> = async () => {}) {}
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.revision;
  private emit() { if (this.batching) { this.pendingEmission = true; return; } this.revision++; this.listeners.forEach((listener) => listener()); }
  state(key: string) { return this.fields.get(key); }
  entries(projectId?: string) { return [...this.fields.entries()].filter(([, value]) => !projectId || value.draft.projectId === projectId); }
  private set(key: string, patch: Partial<DraftState>) { const value = this.fields.get(key); if (value) { this.fields.set(key, { ...value, ...patch }); this.emit(); } }

  observe(project: Project) {
    this.batching = true;
    this.versions.set(project.id, project.history.currentVersionId);
    const translations = Object.entries(project.translations).map(([language,translation]) => [language,new Map(translation.segments.map((segment) => [segment.segmentId,segment]))] as const);
    for (const segment of project.transcript.segments) {
      this.observeField(project.id, segment.id, "source", segment.text);
      for (const [language, translation] of translations) {
        const item = translation.get(segment.id);
        if (item) this.observeField(project.id, segment.id, `translation:${language}`, item.text);
      }
    }
    const ids = new Set(project.transcript.segments.map((segment) => segment.id));
    for (const [key, state] of this.entries(project.id)) {
      if (!ids.has(state.draft.segmentId) && state.status !== "saved") this.set(key, { status: "conflict", error: "字幕段已被删除 / Segment was removed" });
    }
    this.batching = false;
    if (this.pendingEmission) { this.pendingEmission = false; this.emit(); }
    if (!this.loading.has(project.id)) {
      const loading = this.restore(project.id);
      this.loading.set(project.id, loading);
    }
  }
  private observeField(projectId: string, segmentId: string, field: string, text: string) {
    const key = fieldKey(projectId, segmentId, field), state = this.fields.get(key);
    if (!state) {
      this.fields.set(key, { draft: { projectId, segmentId, field, sessionId: this.sessionId, baseVersionId: this.versions.get(projectId) ?? null, baseText: text, text, revision: 0 }, status: "saved", journaled: true, error: null, currentText: text, composing: false, groupId: uid() });
      this.emit(); return;
    }
    if (state.currentText === text) return;
    if (state.status === "saved") this.set(key, { currentText: text, draft: { ...state.draft, text, baseText: text, baseVersionId: this.versions.get(projectId) ?? null } });
    else if (text !== state.draft.baseText) this.set(key, { currentText: text, status: "conflict", error: "当前内容已变化 / Content changed" });
  }
  private async restore(projectId: string) {
    try {
      for (const draft of await this.transport.list(projectId)) {
        const key = fieldKey(projectId, draft.segmentId, draft.field), existing = this.fields.get(key);
        // Never replace text already typed while the journal read was in flight.
        const recoveryKey = existing && existing.status !== "saved" ? `${key}:${draft.sessionId}` : key;
        const currentText = existing?.currentText ?? "";
        this.fields.set(recoveryKey, { draft, currentText, status: "conflict", journaled: true, composing: false, groupId: uid(), error: "已恢复草稿，请核对后保存 / Recovered draft: review before saving" });
        this.emit();
      }
    } catch (error) {
      // A failed restore is surfaced; it must not be mistaken for an empty journal.
      this.restoreError = error instanceof Error ? error.message : String(error); this.emit();
      this.loading.delete(projectId);
    }
  }
  restoreError: string | null = null;

  change(key: string, text: string) {
    const state = this.fields.get(key); if (!state) return;
    this.set(key, { draft: { ...state.draft, text, revision: state.draft.revision + 1 }, journaled: false, status: state.status === "conflict" ? "conflict" : "dirty", error: state.status === "conflict" ? state.error : null });
    clearTimeout(this.journals.get(key));
    this.journals.set(key, setTimeout(() => { void this.persist(key).catch(() => {}); }, 200));
    clearTimeout(this.saves.get(key));
    if (!state.composing && state.status !== "conflict") this.saves.set(key, setTimeout(() => { void this.save(key).catch(() => {}); }, 800));
  }
  composition(key: string, composing: boolean) {
    this.set(key, { composing }); clearTimeout(this.saves.get(key));
    if (!composing) { const state = this.fields.get(key); if (state && state.status === "dirty") this.saves.set(key, setTimeout(() => { void this.save(key).catch(() => {}); }, 800)); }
  }
  async persist(key: string) {
    clearTimeout(this.journals.get(key)); this.journals.delete(key);
    const state = this.fields.get(key); if (!state || state.journaled) return;
    const draft = { ...state.draft };
    const next = (this.journalChains.get(key) ?? Promise.resolve()).catch(() => {}).then(async () => {
      try { await this.transport.journal(draft); if (this.fields.get(key)?.draft.revision === draft.revision) this.set(key, { journaled: true }); }
      catch (error) { this.set(key, { status: "failed", error: String(error), journaled: false }); throw error; }
    });
    this.journalChains.set(key, next); await next;
  }
  async save(key: string, endGroup = false): Promise<void> {
    clearTimeout(this.saves.get(key)); this.saves.delete(key);
    const initial = this.fields.get(key); if (!initial) return;
    const projectId = initial.draft.projectId;
    const next = (this.chains.get(projectId) ?? Promise.resolve()).catch(() => {}).then(async () => {
      const state = this.fields.get(key);
      if (!state) return;
      if (state.status === "saved") { if (endGroup) this.set(key, { groupId: uid() }); return; }
      if (state.composing || state.status === "conflict") throw new Error(state.error ?? "Finish composing before saving");
      await this.persist(key);
      const edit = state.request ?? { mutationId: uid(), expectedVersionId: this.versions.get(projectId) ?? null, groupId: state.groupId, draft: { ...state.draft } };
      this.set(key, { status: "saving", request: edit });
      try {
        const receipt = await this.transport.save(edit);
        this.versions.set(projectId, receipt.versionId);
        const latest = this.fields.get(key)!;
        const newer = latest.draft.revision !== edit.draft.revision;
        this.set(key, { draft: { ...latest.draft, baseText: receipt.text, baseVersionId: receipt.versionId, revision: latest.draft.revision + (newer ? 1 : 0) }, currentText: receipt.text, status: newer ? "dirty" : "saved", journaled: !newer, request: undefined, error: null, groupId: endGroup ? uid() : latest.groupId });
        if (newer) await this.persist(key);
        // A view refresh failure must not turn an acknowledged write into a failed save.
        await this.onSaved(receipt).then((project) => { if (project) this.observe(project); }).catch((error) => { this.restoreError = String(error); this.emit(); });
      } catch (error) {
        const code = (error as { code?: string }).code ?? "";
        const conflict = code.includes("conflict") || String(error).includes("conflict");
        this.set(key, { status: conflict ? "conflict" : "failed", error: error instanceof Error ? error.message : String(error), ...(conflict ? { request: undefined } : {}) });
        if (conflict && this.transport.load) {
          await this.transport.load(projectId).then((project) => this.observe(project)).catch(() => {});
        }
        throw error;
      }
    });
    this.chains.set(projectId, next); await next;
    if (this.fields.get(key)?.status === "dirty") await this.save(key, endGroup);
  }
  async flush(projectId?: string) {
    // Journal first so one conflicting field cannot prevent recovery of other drafts.
    const keys = this.entries(projectId).filter(([, state]) => state.status !== "saved").map(([key]) => key);
    const journalResults = await Promise.allSettled(keys.map((key) => this.persist(key)));
    const saves = await Promise.allSettled(keys.map((key) => this.save(key, true)));
    const failed = [...journalResults, ...saves].find((result) => result.status === "rejected");
    if (failed?.status === "rejected") throw failed.reason;
  }
  async mutate(projectId: string, operation: ProjectOperation): Promise<CoreEnvelope> {
    await this.flush(projectId);
    let result!: CoreEnvelope;
    const next = (this.chains.get(projectId) ?? Promise.resolve()).catch(() => {}).then(async () => {
      if (!this.transport.mutate) throw new Error("Versioned project mutations are unavailable");
      const pending = this.pendingOperations.get(projectId);
      if (pending && JSON.stringify(pending.operation) !== JSON.stringify(operation)) throw new Error("请先重试上次未确认的操作 / Retry the unacknowledged operation first");
      const mutation = pending ?? { projectId, mutationId: uid(), expectedVersionId: this.versions.get(projectId) ?? null, operation };
      this.pendingOperations.set(projectId, mutation);
      try {
        result = await this.transport.mutate(mutation);
        if (result.versionId !== undefined) this.versions.set(projectId, result.versionId);
        this.pendingOperations.delete(projectId);
      } catch (error) {
        const code = (error as { code?: string }).code;
        // A structured rejection proves there was no commit; transport failures do not.
        if (code) this.pendingOperations.delete(projectId);
        if (code?.includes("conflict") && this.transport.load) await this.transport.load(projectId).then((project) => this.observe(project));
        throw error;
      }
    });
    this.chains.set(projectId, next); await next; return result;
  }
  async discard(key: string) {
    const state = this.fields.get(key); if (!state) return;
    clearTimeout(this.saves.get(key)); clearTimeout(this.journals.get(key));
    await (this.journalChains.get(key) ?? Promise.resolve()).catch(() => {});
    await this.transport.discard(state.draft);
    this.set(key, { draft: { ...state.draft, text: state.currentText, baseText: state.currentText }, status: "saved", journaled: true, error: null, request: undefined, groupId: uid() });
  }
  async keepDraft(key: string) {
    const state = this.fields.get(key); if (!state) return;
    this.set(key, { draft: { ...state.draft, baseText: state.currentText, baseVersionId: this.versions.get(state.draft.projectId) ?? null, revision: state.draft.revision + 1 }, status: "dirty", journaled: false, error: null, request: undefined, groupId: uid() });
    await this.save(key, true);
  }
  async journalAll() { await Promise.all(this.entries().filter(([, s]) => s.status !== "saved").map(([key]) => this.persist(key))); }
  disposeTimers() { this.saves.forEach(clearTimeout); this.journals.forEach(clearTimeout); }
}
