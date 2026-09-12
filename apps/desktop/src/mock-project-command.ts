import { mockRun } from "./core.mock";
import type { ProjectCommand } from "./generated/core-contract";
/** Browser behavior simulation; durable replay is verified by Core integration tests. */
export function mockProjectCommand(request: ProjectCommand) {
  return mockRun(request.action === "import" ? ["import", request.path]
    : ["project", "delete", request.projectId, "--expected-version", request.expectedVersionId]);
}
