import { runCore } from "../core";
import type { LocalCapabilityId } from "../types";

export const localResourceClient = {
  status: () => runCore(["resources", "status"]),
  plan: (capability: LocalCapabilityId) => runCore(["resources", "plan", capability]),
  configure: (root: string) => runCore(["resources", "configure", "--root", root]),
  install: (capability: LocalCapabilityId) => runCore(["resources", "install", capability]),
  getJob: (jobId: string) => runCore(["resources", "job", jobId]),
  listJobs: () => runCore(["resources", "jobs"]),
  cancel: (jobId: string) => runCore(["resources", "cancel", jobId]),
  resume: (jobId: string) => runCore(["resources", "resume", jobId]),
  repair: (capability: LocalCapabilityId) => runCore(["resources", "repair", capability]),
  remove: (capability: LocalCapabilityId) => runCore(["resources", "remove", capability]),
};
