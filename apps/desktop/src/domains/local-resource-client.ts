import { desktopQuery } from "./desktop-query-client";
import { runCore } from "../core";
import type { LocalCapabilityId, LocalTranscriptionProfile } from "../types";

export const localResourceClient = {
  status: () => desktopQuery({action:"resource_status"}),
  checkUpdates: (capability?: LocalCapabilityId) => runCore(["resources", "check-updates", ...(capability ? [capability] : [])]),
  plan: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => desktopQuery({action:"resource_plan",capability,profile:profile??null}),
  configure: (root: string) => runCore(["resources", "configure", "--root", root]),
  migrate: (root: string) => runCore(["resources", "migrate", "--root", root]),
  install: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => runCore(["resources", "install", capability, ...(profile ? ["--profile", profile] : [])]),
  update: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => runCore(["resources", "update", capability, ...(profile ? ["--profile", profile] : [])]),
  getJob: (jobId: string) => desktopQuery({action:"resource_job",jobId}),
  listJobs: () => desktopQuery({action:"resource_jobs"}),
  cancel: (jobId: string) => runCore(["resources", "cancel", jobId]),
  resume: (jobId: string) => runCore(["resources", "resume", jobId]),
  repair: (capability: LocalCapabilityId) => runCore(["resources", "repair", capability]),
  rollback: (capability: LocalCapabilityId) => runCore(["resources", "rollback", capability]),
  remove: (capability: LocalCapabilityId) => runCore(["resources", "remove", capability]),
  cleanup: () => runCore(["resources", "cleanup"]),
};
