import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "target", "release", "siaocut-core.exe");
const target = resolve(
  root,
  "apps",
  "desktop",
  "src-tauri",
  "binaries",
  "siaocut-core-x86_64-pc-windows-msvc.exe",
);

if (!existsSync(source)) {
  throw new Error(`Release Core 不存在：${source}`);
}
mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
process.stdout.write(`Prepared Tauri sidecar: ${target}\n`);
