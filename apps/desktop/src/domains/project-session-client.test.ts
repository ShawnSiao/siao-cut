import { afterEach,expect,it,vi } from "vitest";
import { projectSessionClient } from "./project-session-client";
const transport = vi.hoisted(() => ({run: vi.fn()}));
vi.mock("../core", () => ({runCoreStructured: transport.run, listProjects: vi.fn(), loadProject: vi.fn()}));
afterEach(() => {sessionStorage.clear(); vi.clearAllMocks();});
it("reuses the import operation ID after a lost response and starts a new intent after success", async () => {
  transport.run.mockRejectedValueOnce(new Error("response lost")).mockResolvedValue({projectId: "new"});
  await expect(projectSessionClient.importMedia("D:/媒体.wav")).rejects.toThrow("response lost");
  const first = transport.run.mock.calls[0][0].request.mutationId;
  await projectSessionClient.importMedia("D:/媒体.wav");
  expect(transport.run.mock.calls[1][0].request.mutationId).toBe(first);
  await projectSessionClient.importMedia("D:/媒体.wav");
  expect(transport.run.mock.calls[2][0].request.mutationId).not.toBe(first);
});
it("deleting the same confirmed version reuses its command identity", async () => {
  transport.run.mockResolvedValue({deleted: true});
  await projectSessionClient.deleteProject("p", "v");
  await projectSessionClient.deleteProject("p", "v");
  expect(transport.run.mock.calls[0][0]).toEqual(transport.run.mock.calls[1][0]);
});
