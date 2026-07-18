#!/usr/bin/env node
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { API_VERSION, createProject, homeDir, listProjects, loadProject, restoreVersion } from "./core/store.mjs";
import { addSegment, detectCuts, editSegment, restoreAllCuts, setCutStatus } from "./core/project.mjs";
import { auditProject, renderExport } from "./core/export.mjs";
import { claimTask, createTask, submitTask } from "./core/tasks.mjs";

const supportedExtensions = new Set([".mp4", ".mov", ".mkv", ".mp3", ".m4a", ".wav"]);

function parse(argv) {
  const flags = {};
  const positionals = [];
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "-o") {
      flags.o = argv[++index];
      continue;
    }
    if (value.startsWith("--")) {
      const key = value.slice(2);
      if (["json", "bilingual", "include-cuts", "all"].includes(key)) flags[key] = true;
      else flags[key] = argv[++index];
    } else positionals.push(value);
  }
  return { flags, positionals };
}

function reply(payload, json) {
  const envelope = { apiVersion: API_VERSION, status: "ok", ...payload };
  if (json) console.log(JSON.stringify(envelope, null, 2));
  else console.log(payload.message || JSON.stringify(envelope, null, 2));
}

function usage() {
  return `SiaoCut CLI\n\nCommands:\n  health\n  import <media> [--title <title>]\n  project list|show <projectId>|restore <projectId> <versionId>\n  transcript add <projectId> --start <seconds> --end <seconds> --text <text>\n  transcript edit <projectId> <segmentId> --text <text>\n  transcript export <projectId> --format srt|vtt|ass|markdown [-o file] [--lang en] [--bilingual] [--include-cuts]\n  task create <projectId> --kind polish|translate|summary [--lang en]\n  task claim --worker <name>\n  task submit <taskId> --worker <name> --response <file.json>\n  cut detect <projectId>|apply <projectId> <cutId>|restore <projectId> <cutId>|restore <projectId> --all\n  audit <projectId>\n\nUse --json for stable machine-readable output.`;
}

async function main() {
  const { flags, positionals } = parse(process.argv.slice(2));
  const json = Boolean(flags.json);
  const [command, subcommand, ...rest] = positionals;
  if (!command || command === "help" || command === "--help") return reply({ message: usage() }, json);
  if (command === "health") return reply({ home: homeDir(), engines: { asr: "not_configured", ffmpeg: "not_configured" }, message: "核心可用；真实媒体引擎尚未配置。" }, json);
  if (command === "import") {
    const source = subcommand;
    if (!source) throw new Error("请提供本地媒体路径");
    if (!supportedExtensions.has(path.extname(source).toLowerCase())) throw new Error("仅支持 mp4/mov/mkv/mp3/m4a/wav");
    const project = await createProject(source, flags.title);
    return reply({ projectId: project.id, project, message: `已创建项目：${project.title}` }, json);
  }
  if (command === "project") {
    if (subcommand === "list") {
      const projects = await listProjects();
      return reply({ projects: projects.map(({ id, title, updatedAt, media }) => ({ id, title, updatedAt, media })) }, json);
    }
    if (subcommand === "show") {
      const project = await loadProject(rest[0]);
      return reply({ projectId: project.id, project }, json);
    }
    if (subcommand === "restore") {
      const { project, result } = await restoreVersion(rest[0], rest[1]);
      return reply({ projectId: project.id, version: result, message: "已恢复版本。" }, json);
    }
  }
  if (command === "transcript") {
    const projectId = rest[0];
    if (subcommand === "add") {
      const { project, result } = await addSegment(projectId, flags);
      return reply({ projectId: project.id, segment: result }, json);
    }
    if (subcommand === "edit") {
      const { project, result } = await editSegment(projectId, rest[1], flags.text);
      return reply({ projectId: project.id, segment: result, message: "原文已更新；已有译文已标记为待更新。" }, json);
    }
    if (subcommand === "export") {
      const project = await loadProject(projectId);
      const format = flags.format || "srt";
      const content = renderExport(project, { format, language: flags.lang, bilingual: flags.bilingual, includeCuts: flags["include-cuts"] });
      const extension = format === "markdown" ? "md" : format;
      const output = flags.o || path.join(process.cwd(), `${project.title}.${extension}`);
      await writeFile(output, content, "utf8");
      return reply({ projectId, output: path.resolve(output), format, audit: auditProject(project) }, json);
    }
  }
  if (command === "task") {
    if (subcommand === "create") {
      const { project, result } = await createTask(rest[0], flags.kind, flags.lang);
      return reply({ projectId: project.id, taskId: result.id, task: result, message: "任务已创建，等待 Agent 领取。" }, json);
    }
    if (subcommand === "claim") {
      const result = await claimTask(flags.worker);
      return reply(result ? { projectId: result.project.id, taskId: result.result.task.id, task: result.result.task, payload: result.result.payload } : { task: null, message: "当前没有待领取任务。" }, json);
    }
    if (subcommand === "submit") {
      const response = JSON.parse(await readFile(flags.response, "utf8"));
      const { project, result } = await submitTask(rest[0], flags.worker, response);
      return reply({ projectId: project.id, taskId: result.id, task: result, message: "Agent 结果已应用并创建版本。" }, json);
    }
  }
  if (command === "cut") {
    const projectId = rest[0];
    if (subcommand === "detect") {
      const { project, result } = await detectCuts(projectId);
      return reply({ projectId: project.id, suggestions: result, message: `发现 ${result.length} 处可能的口癖，尚未删除。` }, json);
    }
    if (subcommand === "apply") {
      const { project, result } = await setCutStatus(projectId, rest[1], "applied");
      return reply({ projectId: project.id, cut: result, message: "已应用可恢复软剪辑。" }, json);
    }
    if (subcommand === "restore") {
      const operation = flags.all ? restoreAllCuts(projectId) : setCutStatus(projectId, rest[1], "restored");
      const { project, result } = await operation;
      return reply({ projectId: project.id, restored: result, message: "已恢复原片时间线。" }, json);
    }
  }
  if (command === "audit") {
    const project = await loadProject(subcommand);
    return reply({ projectId: project.id, audit: auditProject(project) }, json);
  }
  throw new Error(`未知命令。\n\n${usage()}`);
}

main().catch((error) => {
  const { flags } = parse(process.argv.slice(2));
  const envelope = { apiVersion: API_VERSION, status: "error", code: "invalid_request", message: error.message };
  if (flags.json) console.error(JSON.stringify(envelope, null, 2));
  else console.error(`SiaoCut: ${error.message}`);
  process.exitCode = 1;
});
