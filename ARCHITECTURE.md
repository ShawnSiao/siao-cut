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
                  Component Store → FFmpeg → 16 kHz PCM WAV → whisper.cpp
```

## Storage and recovery

`rusqlite` uses its bundled SQLite build, so Core has no external SQLite DLL dependency. The database uses foreign keys and WAL mode. The Core service is the only SQLite writer; CLI processes exchange line-delimited JSON over a per-data-directory Windows named pipe. Every content mutation appends an operation record and stores a snapshot (last 40 versions per project). The original media remains outside the database and is identified by absolute path plus SHA-256; it is never overwritten.

`audit` checks subtitle timing, stale translations, missing media, and changed media hashes. Export is blocked for hard audit failures; stale translations remain a visible warning rather than a silent rewrite.

## Runtime adapters

- FFmpeg、FFprobe、`yt-dlp`、Whisper CPU/Vulkan、VAD 和模型统一由共享 `Component Store` 解析；产品只保存 `ComponentKey`，不保存下载 URL、大小、哈希或 Store 内部路径。
- 默认共享根目录为 `%LOCALAPPDATA%\Siao\component-store`。安装、校验、external 登记、租约和迁移均通过 `component-store` 命令完成；旧 `SIAOCUT_*` 路径只作为一次性迁移输入，不作为正式执行来源。
- Whisper 正式身份为版本 `1.9.1-siao.1`，运行时 ID 为 `siao-whisper-cpu` 和 `siao-whisper-vulkan`；旧 SiaoCut 身份不能满足 common v2 requirements。
- 模型通过 `transcribe <projectId> --model component:tiny|component:base|component:small --expected-version <currentVersionId>` 选择。缺少或未验证组件时返回结构化状态，不临时回退到环境变量或祖先目录。
- 每个实际执行子进程持有 `LeasedComponent`，默认租约 TTL 为 30 秒，每 10 秒发送 heartbeat；成功、失败、取消、窗口退出和异常清理均释放租约。
- `transcribe` 使用 FFmpeg 生成标准化的 16 kHz 单声道 WAV，调用已解析的 Whisper 运行时，并在原媒体时间轴上校验完整的字幕段和词级时间后原子写入。替换已有转写仍需只读预检和 `--confirm-replace`，已有译文变为 `stale`。

## Stable CLI contract

Every `--json` result is enveloped as `{ apiVersion, status, ... }`. Rust Core implements `health`, `import`, `project`, `transcript`, `task`, `cut`, `audit`, and `transcribe`. Agent claim payloads contain only text, IDs and timestamps; media paths are never included.

Task leases support heartbeat, progress events, failure, retry, cancellation and request-boundary recovery. Agent submission must return the claimed `baseVersionId`; if the project changed during processing, Core returns `project_version_conflict` instead of overwriting human edits.

## Local Codex Agent Runner

Core 提供 `agent health/start/status/list/cancel/resume`，用于把已明确创建的文本任务交给本机 Codex CLI。Runner 固定使用只读沙箱、结构化输出 Schema 和独立临时工作目录；标准输入只包含任务文本、字幕段 ID、时间戳和结构约束。调用参数与子进程环境不传递媒体路径、数据库路径、仓库路径、API Key 或 `CODEX_HOME` 等私密配置。

`agent_runs` 与 `agent_run_batches` 只保存进度、Codex 版本、认证方式摘要、线程 ID、脱敏错误和已校验的结构化结果。JSONL 事件在管道中解析后立即丢弃，不写入数据库或日志。每个结果必须确认完整的批次字幕段 ID，并拒绝缺段、重复段、越权段和版本变化。

Codex 子进程由带 `KILL_ON_JOB_CLOSE` 的 Windows Job Object 管理。取消、超时或 Worker 退出会终止整棵子进程树；异常退出标记为 `interrupted`，只能通过显式 `resume` 重新排队。`completed` 只表示建议已提交到 `pending_review`，不会自动修改文稿。Codex 缺失或未登录时，原有手工 Agent 交接和不依赖 Agent 的基础流程保持可用。

## Desktop boundary

React 仅调用已注册的 Tauri 命令。Tauri Rust 层以参数数组调用 `siaocut-core --json`，拒绝内部服务命令和未知顶级命令；Core CLI 再通过 Windows 命名管道连接单实例服务。GUI 不读取数据库，也不把媒体路径交给 Agent。

`apps/desktop/src/App.tsx` 只负责装配工作台。项目会话、后台任务、文稿编辑、Agent 审阅、导出与运行环境分别通过 `apps/desktop/src/domains/` 下的具名客户端调用 Core；组件、工作台控制器和普通 Hook 不得直接调用 `runCore`。`apps/desktop/src/architecture.test.ts` 检查该边界并限制 `App.tsx` 的规模。

后台任务统一注册到 `useBackgroundTaskRegistry`。不同任务可以独立轮询，同一任务必须等待上一次请求结束后再调度下一次请求；任务结束或组件卸载时停止对应计时器，避免状态查询重入。

本地媒体使用 Tauri asset 协议播放。项目读取完成后，Rust 层从 Core 响应中取得媒体路径并只授权该文件。静态配置中的 asset scope 保持为空，不配置全磁盘通配符。

## Windows release boundary

SiaoCut 安装包保持 `app-only`，只包含桌面程序、Rust Core、前端资源和 notices，不携带 FFmpeg、Whisper、VAD、模型、`yt-dlp` 或 Store 数据。共享 core/catalog 源码固定依赖 `ShawnSiao/siao-component-store` 的 canonical commit；大型归档、SBOM、构建来源、许可材料和安装后文件清单统一发布到 `ShawnSiao/siao-components`。

common v2 必须同时包含 8 个公共组件，并通过 CPU/Vulkan 固定归档与原媒体时间轴验收后，才允许进入正式 Store 执行路径。产品卸载只释放 `siaocut` consumer，不删除共享 Store、external 来源或未匹配的 legacy 文件。代码签名和二进制发布仍是独立门槛：未签名候选包不能作为公开正式安装包。

代码签名与二进制打包是两个独立门槛：本地可以生成完整但未签名的候选包；只有配置受信任证书并通过 `Get-AuthenticodeSignature` 后，才是可公开分发的正式安装包。
