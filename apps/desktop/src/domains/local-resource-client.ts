import { runCore } from "../core";
import type { LocalCapabilityId, LocalTranscriptionProfile } from "../types";

export const localResourceClient = {
  status: () => runCore(["resources", "status"]),
  plan: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => runCore(["resources", "plan", capability, ...(profile ? ["--profile", profile] : [])]),
  configure: (root: string) => runCore(["resources", "configure", "--root", root]),
  install: (capability: LocalCapabilityId, profile?: LocalTranscriptionProfile) => runCore(["resources", "install", capability, ...(profile ? ["--profile", profile] : [])]),
  getJob: (jobId: string) => runCore(["resources", "job", jobId]),
  listJobs: () => runCore(["resources", "jobs"]),
  cancel: (jobId: string) => runCore(["resources", "cancel", jobId]),
  resume: (jobId: string) => runCore(["resources", "resume", jobId]),
  repair: (capability: LocalCapabilityId) => runCore(["resources", "repair", capability]),
  remove: (capability: LocalCapabilityId) => runCore(["resources", "remove", capability]),
};
