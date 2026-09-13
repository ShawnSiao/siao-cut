# Changelog / 变更日志

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) for user-visible changes. No formal version has been released.

本项目按 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 记录用户可见变更。目前没有正式发布版本。

## [Unreleased]

## [local-preview-0.2.0-20260913-r2] - 2026-09-13

Second unsigned preview build; application version remains 0.2.0 and automatic updates remain disabled. / 第二次未签名预览构建，应用版本仍为 0.2.0，自动更新保持关闭。

### Fixed / 修复

- Share verified runtime selection and VAD gating between CLI and desktop background Whisper transcription. Previously, background jobs omitted VAD and always recorded it as unused. Candidates now retain the actual timing mode and counts; version checks and replacement review remain required. / CLI 与桌面后台 Whisper 转写共用运行时校验和 VAD 门控；修复后台任务未启用 VAD 且始终记录为未使用的问题。候选结果记录实际时间模式和数量，保留版本检查及替换审核。

### Documentation / 文档

- Align release status, first-run guides, AI execution choices, network boundaries, and dated acceptance records. / 统一发布状态、首次使用指南、AI 执行方式、联网边界和历史验收说明。
- Keep product prototypes and development design documents local; remove their public entry points. / 产品原型和开发设计说明仅留本地，移除公开入口。
- Document single-send AI authorization without automatic generation retries, background candidate review, and workflow authorization states. Require Node.js 22.13+ (22.x) or 24+ to match locked tooling; dependency versions remain unchanged. / 对齐 AI 单次发送授权、禁止自动重试生成请求、后台候选审核和工作流授权状态；Node.js 要求调整为 22.13+（22.x）或 24+，匹配已锁定工具，依赖版本不变。

### Repository checks / 仓库检查

- Ignore local campaign material, unreviewed screenshots, machine-specific tools, and Python caches. Reject these paths even when force-added, and inspect Git index contents independently of later working-tree edits. / 忽略活动素材、未审核截图、本机工具和 Python 缓存；即使强制暂存也会拒绝，并独立检查暂存内容，防止后续工作区修改掩盖误提交。

## [local-preview-0.2.0-20260913] - 2026-09-13

Unsigned Windows x64 preview based on `3d3e433`; app version remains `0.2.0`. This GitHub Pre-release is not a stable release and has automatic updates disabled.

基于 `3d3e433` 的 Windows x64 未签名预览包，应用版本仍为 `0.2.0`。此 GitHub Pre-release 不属于稳定版，自动更新关闭。

### Added

- Recoverable MOSS multispeaker transcription with loopback-only service configuration, background jobs, candidate review, speaker review, and structured export.
- English Creator Source Beta guidance and external Agent handoff instructions.
- Windows unsigned-candidate acceptance records.
- Configurable LLM API services alongside local Codex and manual handoff, with per-send approval of the actual text payload. / 支持配置 LLM API、本机 Codex 和手工交接，每次发送前确认实际文本载荷。
- Subtitle box sizing, embedded MP4/MKV subtitle tracks, and external SRT/VTT delivery. / 支持字幕框尺寸调整、MP4/MKV 内嵌字幕轨和外置 SRT/VTT。
- Subtitle autosave, recoverable local drafts, version-conflict protection, and manual component update checks. / 支持字幕自动保存、本地草稿恢复、版本冲突保护和手动检查组件更新。

### Changed

- Pull request CI now validates stacked branches as well as branches that target `main` directly.
- External Agent states distinguish waiting for claim, active processing, submitted results, and human review.
- CPU, Vulkan, and CUDA whisper.cpp build entries now share one pinned source commit and timeline patch. Runtime metadata binds the executable, backend, source, patch, and acceptance evidence.
- Background transcription reports actual stages; old failed execution records can be archived and restored. / 后台转写显示实际阶段，旧失败执行记录支持归档和恢复。
- Desktop database requests use shorter bounded lock waits and distinguish retryable storage errors from draft persistence. / 桌面数据库请求缩短锁等待，区分可重试存储错误与草稿落盘状态。

### Fixed

- Quick transcription enables VAD only for runtimes with independently verified original-media token timing. Unknown or unverified runtimes automatically use the no-VAD safe path, and the desktop UI reports the actual mode.
- Quick transcription rejects untrusted timing before any project write and exposes an explicit undoable regeneration flow for existing subtitles.
- Transcription result application now preserves later project edits and requires explicit replacement confirmation.
- Prepared transcription results can recover after an interrupted finalization step.
- Subtitle merge tests now wait for the asynchronous transcript refresh.
- Fix AI batch configuration checks, manual handoff path validation, cancellation feedback, and subtitle timeline overlap. / 修复 AI 分批配置校验、手工交接路径校验、取消状态提示和字幕时间轴遮挡。

### Release status

- The public preview is unsigned, provides no automatic update manifest, and is not marked as stable Latest. No formal stable release is available. / 公开预览包未签名，不提供自动更新清单，未设为稳定版 Latest；尚无正式稳定版。
- The preview still needs full native regression after installation, system scaling, historical formal-installer upgrades, real media and AI workflows, signing, and provenance acceptance. Historical test results apply only to their recorded builds. / 预览包仍需完成安装后完整原生回归、系统缩放、历史正式安装包升级、真实媒体与 AI 流程、签名和来源证明验收；历史结果只适用于记录中的构建。
- Windows 11 and external Creator Beta acceptance remain incomplete. / Windows 11 和外部 Creator Beta 验收仍未完成。

[Unreleased]: https://github.com/ShawnSiao/siao-cut/compare/local-preview-0.2.0-20260913-r2...main
[local-preview-0.2.0-20260913]: https://github.com/ShawnSiao/siao-cut/releases/tag/local-preview-0.2.0-20260913

[local-preview-0.2.0-20260913-r2]: https://github.com/ShawnSiao/siao-cut/releases/tag/local-preview-0.2.0-20260913-r2
