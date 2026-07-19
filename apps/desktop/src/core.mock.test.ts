import { describe, expect, it } from "vitest";
import { mockRun } from "./core.mock";

describe("browser Mock Core contract", () => {
  it("returns a structured error for unknown commands", async () => {
    const result = await mockRun(["unsupported", "command"]);

    expect(result).toMatchObject({
      status: "error",
      error: {
        code: "unsupported_command",
      },
    });
    expect(result.error?.message).toContain("unsupported command");
  });
});
