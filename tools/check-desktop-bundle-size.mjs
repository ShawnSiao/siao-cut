import { readFile, stat } from "node:fs/promises";
import path from "node:path";

const MAX_ENTRY_BYTES = 550 * 1024;
const distDirectory = path.resolve(process.argv[2] ?? "apps/desktop/dist");
const indexPath = path.join(distDirectory, "index.html");
const indexHtml = await readFile(indexPath, "utf8");
const entryMatch = indexHtml.match(/<script[^>]+src="([^"]+\.js)"/);

if (!entryMatch) {
  throw new Error(`Could not find the desktop entry script in ${indexPath}.`);
}

const entryRelativePath = entryMatch[1].replace(/^\//, "");
const entryPath = path.join(distDirectory, entryRelativePath);
const entry = await stat(entryPath);
const kibibytes = (value) => (value / 1024).toFixed(2);

console.log(
  `Desktop entry bundle: ${kibibytes(entry.size)} KiB / ${kibibytes(MAX_ENTRY_BYTES)} KiB (${entryRelativePath})`,
);

if (entry.size > MAX_ENTRY_BYTES) {
  throw new Error(
    `Desktop entry bundle exceeds the ${kibibytes(MAX_ENTRY_BYTES)} KiB budget by ${kibibytes(entry.size - MAX_ENTRY_BYTES)} KiB.`,
  );
}
