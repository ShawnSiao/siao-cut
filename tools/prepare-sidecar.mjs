import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
let profile = "release";
for (let index = 0; index < args.length; index += 1) {
  if (args[index] !== "--profile" || !args[index + 1]) {
    throw new Error(`未知参数：${args[index] ?? ""}`);
  }
  profile = args[index + 1];
  index += 1;
}
if (!new Set(["debug", "release"]).has(profile)) {
  throw new Error(`不支持的 Core 构建配置：${profile}`);
}

const source = resolve(root, "target", profile, "siaocut-core.exe");
const target = resolve(
  root,
  "apps",
  "desktop",
  "src-tauri",
  "binaries",
  "siaocut-core-x86_64-pc-windows-msvc.exe",
);

if (!existsSync(source)) {
  throw new Error(`${profile} Core 不存在：${source}`);
}
mkdirSync(dirname(target), { recursive: true });
copyFileSync(source, target);
process.stdout.write(`Prepared Tauri sidecar: ${target}\n`);
