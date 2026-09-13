import { runCoreStructured } from "../core";
import type { DesktopQuery } from "../generated/core-contract";

export const desktopQuery = (request: DesktopQuery) => runCoreStructured({kind:"desktop_query",request});
