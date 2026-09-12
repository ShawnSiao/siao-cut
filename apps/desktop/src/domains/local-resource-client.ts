import type { LocalCapabilityId,LocalTranscriptionProfile } from "../types";
import { desktopControl } from "./desktop-control-client";
import { desktopQuery } from "./desktop-query-client";

export const localResourceClient = {
  status: () => desktopQuery({action:"resource_status"}),
  checkUpdates: (capability?: LocalCapabilityId) => desktopControl({ action: "resource_check_updates", capability: capability ?? null }),
  plan: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => desktopQuery({action:"resource_plan",capability,profile:profile??null}),
  configure: (root: string) => desktopControl({ action: "resource_configure", root: root }),
  migrate: (root: string) => desktopControl({ action: "resource_migrate", root: root }),
  install: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => desktopControl({ action: "resource_install", capability: capability, profile: profile ?? null }),
  update: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => desktopControl({ action: "resource_update", capability: capability, profile: profile ?? null }),
  getJob: (jobId: string) => desktopQuery({action:"resource_job",jobId}),
  listJobs: () => desktopQuery({action:"resource_jobs"}),
  cancel: (jobId: string) => desktopControl({ action: "resource_cancel", jobId: jobId }),
  resume: (jobId: string) => desktopControl({ action: "resource_resume", jobId: jobId }),
  repair: (capability: LocalCapabilityId) => desktopControl({ action: "resource_repair", capability: capability }),
  rollback: (capability: LocalCapabilityId) => desktopControl({ action: "resource_rollback", capability: capability }),
  remove: (capability: LocalCapabilityId) => desktopControl({ action: "resource_remove", capability: capability }),
  cleanup: () => desktopControl({ action: "resource_cleanup" }),
};
