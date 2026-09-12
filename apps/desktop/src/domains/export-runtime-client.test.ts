import { afterEach,expect,it,vi } from "vitest";
import { exportRuntimeClient } from "./export-runtime-client";
const transport = vi.hoisted(() => ({run:vi.fn()}));
vi.mock("../core", () => ({runCoreStructured:transport.run}));
afterEach(() => {sessionStorage.clear();vi.clearAllMocks();});
it("binds an export retry to the same version, payload and operation ID after response loss", async () => {
  transport.run.mockRejectedValueOnce(new Error("lost")).mockResolvedValue({job:{id:"export"}});
  const send = () => exportRuntimeClient.exportVideo("p","D:/输出.mp4","burned","source",undefined,false,"v1");
  await expect(send()).rejects.toThrow("lost");
  await send();
  expect(transport.run.mock.calls[0][0]).toEqual(transport.run.mock.calls[1][0]);
  expect(transport.run.mock.calls[0][0]).toMatchObject({kind:"export_command",request:{projectId:"p",expectedVersionId:"v1",operation:{kind:"video",output:"D:/输出.mp4",subtitleDelivery:"burned"}}});
});
