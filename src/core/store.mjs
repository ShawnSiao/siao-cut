import { createHash, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { access, mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";

export const API_VERSION = "0.1";
const FORMAT_VERSION = 1;

export function homeDir() {
  return process.env.SIAOCUT_HOME || path.join(process.env.LOCALAPPDATA || process.cwd(), "SiaoCut");
}

export function projectDir(id) {
  return path.join(homeDir(), "projects", id);
}

function projectFile(id) {
  return path.join(projectDir(id), "project.json");
}

export async function ensureHome() {
  await mkdir(path.join(homeDir(), "projects"), { recursive: true });
}

export async function hashFile(file) {
  const hash = createHash("sha256");
  await new Promise((resolve, reject) => {
    const stream = createReadStream(file);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", resolve);
  });
  return hash.digest("hex");
}

function now() {
  return new Date().toISOString();
}

function snapshot(project, reason) {
  const record = {
    id: `v-${randomUUID().slice(0, 8)}`,
    reason,
    createdAt: now(),
    transcript: structuredClone(project.transcript),
    translations: structuredClone(project.translations),
    edits: structuredClone(project.edits)
  };
  project.versions.push(record);
  project.versions = project.versions.slice(-40);
  project.updatedAt = record.createdAt;
  return record;
}

async function save(project) {
  const file = projectFile(project.id);
  await mkdir(path.dirname(file), { recursive: true });
  const temp = `${file}.tmp`;
  await writeFile(temp, `${JSON.stringify(project, null, 2)}\n`, "utf8");
  await rename(temp, file);
}

export async function createProject(sourcePath, title) {
  await access(sourcePath);
  await ensureHome();
  const id = `p-${randomUUID().slice(0, 12)}`;
  const createdAt = now();
  const project = {
    formatVersion: FORMAT_VERSION,
    id,
    title: title || path.basename(sourcePath, path.extname(sourcePath)),
    createdAt,
    updatedAt: createdAt,
    media: {
      sourcePath: path.resolve(sourcePath),
      sha256: await hashFile(sourcePath),
      extension: path.extname(sourcePath).toLowerCase()
    },
    transcript: { sourceLanguage: "auto", segments: [] },
    translations: {},
    edits: [],
    tasks: [],
    versions: []
  };
  snapshot(project, "项目创建");
  await save(project);
  return project;
}

export async function loadProject(id) {
  try {
    return JSON.parse(await readFile(projectFile(id), "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") throw new Error(`项目不存在：${id}`);
    throw error;
  }
}

export async function listProjects() {
  await ensureHome();
  const root = path.join(homeDir(), "projects");
  const { readdir } = await import("node:fs/promises");
  const entries = await readdir(root, { withFileTypes: true });
  const projects = await Promise.all(entries.filter((entry) => entry.isDirectory()).map((entry) => loadProject(entry.name)));
  return projects.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
}

export async function mutateProject(id, reason, mutation) {
  const project = await loadProject(id);
  const result = await mutation(project);
  snapshot(project, reason);
  await save(project);
  return { project, result };
}

export async function restoreVersion(id, versionId) {
  return mutateProject(id, `恢复 ${versionId}`, (project) => {
    const version = project.versions.find((item) => item.id === versionId);
    if (!version) throw new Error(`版本不存在：${versionId}`);
    project.transcript = structuredClone(version.transcript);
    project.translations = structuredClone(version.translations);
    project.edits = structuredClone(version.edits);
    return version;
  });
}

export function newId(prefix) {
  return `${prefix}-${randomUUID().slice(0, 8)}`;
}
