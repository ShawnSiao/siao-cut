import { runCoreStructured } from "../core";
import type { DesktopControl } from "../generated/core-contract";
export const desktopControl = (request: DesktopControl) => runCoreStructured({kind: "desktop_control", request});
