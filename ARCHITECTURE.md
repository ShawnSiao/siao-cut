# SiaoCut Core architecture

SiaoCut 的唯一项目写入者是 Rust Core。GUI、CLI 与 Skill 必须通过它操作，不能直接写 SQLite 或项目文件。

```text
Tauri 2 + React GUI ─ Tauri Rust proxy
                 │             │
SiaoCut Skill ─ siaocut.ps1 ─ CLI client
                                  │ Windows named pipe
                         per-user Core service
                                  │
                 SQLite (%LOCALAPPDATA%\SiaoCut\siaocut.db)
                   ├─ immutable versions + operations
                   ├─ projects / media evidence
                   ├─ transcript / translation / edit
                   └─ Agent task leases
                                  │
                  FFmpeg → 16 kHz PCM WAV → whisper.cpp
```

## Storage and recovery

`rusqlite` uses its bundled SQLite build, so Core has no external SQLite DLL dependency. The database uses foreign keys and WAL mode. The Core service is the only SQLite writer; CLI processes exchange line-delimited JSON over a per-data-directory Windows named pipe. Every content mutation appends an operation record and stores a snapshot (last 40 versions per project). The original media remains outside the database and is identified by absolute path plus SHA-256; it is never overwritten.

`audit` checks subtitle timing, stale translations, missing media, and changed media hashes. Export is blocked for hard audit failures; stale translations remain a visible warning rather than a silent rewrite.

## Runtime adapters

- FFmpeg comes from `SIAOCUT_FFMPEG` or `PATH`; FFprobe is used opportunistically during import.
- `whisper-cli.exe` comes from `SIAOCUT_WHISPER_CLI`, otherwise `%LOCALAPPDATA%\SiaoCut\bin\whisper-cli.exe`.
- Models are explicitly selected with `transcribe <projectId> --model <path>`; Core does not silently download a model or send media over the network.
- Curated Tiny / Base / Small downloads are background jobs with fixed size and SHA-256. A cancelled `.part` file is retained for an explicit later resume.
- `transcribe` uses FFmpeg to normalize audio, invokes whisper.cpp with JSON output, validates timestamps, and replaces only the generated source transcript. Existing translations become `stale`.

## Stable CLI contract

Every `--json` result is enveloped as `{ apiVersion, status, ... }`. Rust Core implements `health`, `import`, `project`, `transcript`, `task`, `cut`, `audit`, and `transcribe`. Agent claim payloads contain only text, IDs and timestamps; media paths are never included.

Task leases support heartbeat, progress events, failure, retry, cancellation and request-boundary recovery. Agent submission must return the claimed `baseVersionId`; if the project changed during processing, Core returns `project_version_conflict` instead of overwriting human edits.

## Desktop boundary

React 仅调用已注册的 Tauri 命令。Tauri Rust 层以参数数组调用 `siaocut-core --json`，拒绝内部服务命令和未知顶级命令；Core CLI 再通过 Windows 命名管道连接单实例服务。GUI 不读取数据库，也不把媒体路径交给 Agent。

`apps/desktop/src/App.tsx` 只负责装配工作台。项目会话、后台任务、文稿编辑、Agent 审阅、导出与运行环境分别通过 `apps/desktop/src/domains/` 下的具名客户端调用 Core；组件、工作台控制器和普通 Hook 不得直接调用 `runCore`。`apps/desktop/src/architecture.test.ts` 检查该边界并限制 `App.tsx` 的规模。

后台任务统一注册到 `useBackgroundTaskRegistry`。不同任务可以独立轮询，同一任务必须等待上一次请求结束后再调度下一次请求；任务结束或组件卸载时停止对应计时器，避免状态查询重入。

本地媒体使用 Tauri asset 协议播放。项目读取完成后，Rust 层从 Core 响应中取得媒体路径并只授权该文件。静态配置中的 asset scope 保持为空，不配置全磁盘通配符。

## Windows release boundary

NSIS 将 Release Core 作为 sidecar，并携带固定哈希的 LGPL FFmpeg、whisper.cpp CPU 运行时和从锁定提交构建的 Vulkan 运行时。Core 会在切换 Vulkan 时执行硬件探测，不可用时保留 CPU 基线。模型位于 `%LOCALAPPDATA%\SiaoCut\models`，必须由用户明确选择后下载。安装、升级与卸载不应删除该数据目录。

代码签名与二进制打包是两个独立门槛：本地可以生成完整但未签名的候选包；只有配置受信任证书并通过 `Get-AuthenticodeSignature` 后，才是可公开分发的正式安装包。
