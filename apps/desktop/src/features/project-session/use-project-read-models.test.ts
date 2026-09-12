import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { projectSessionClient } from "../../domains/project-session-client";
import { sampleProject } from "../../mock";
import { useProjectReadModels } from "./use-project-read-models";

afterEach(() => vi.restoreAllMocks());

it("loads history again when returning to a project whose workspace projection omits it", async () => {
  const first = { ...structuredClone(sampleProject), versions: [] };
  const second = { ...first, id: "second" };
  const query = vi.spyOn(projectSessionClient, "history").mockResolvedValue({
    apiVersion: "0.1", status: "ok", history: first.history, versions: sampleProject.versions,
  });
  const setProject = vi.fn();
  const { rerender } = renderHook(({ project }) => useProjectReadModels(project, "history", setProject, vi.fn()), { initialProps: { project: first } });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(1));
  rerender({ project: second });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(2));
  rerender({ project: first });
  await waitFor(() => expect(setProject).toHaveBeenCalledTimes(3));
  expect(query).toHaveBeenNthCalledWith(3, first.id);
});
