# SiaoCut

SiaoCut 是面向 AI 口播创作者的 Windows 本地优先剪辑工作台。当前仓库提供可运行的 Rust + SQLite Core、JSON CLI、Agent Skill、FFmpeg / whisper.cpp 本机适配器，以及 Tauri 2 + React 桌面审阅工作台。

## 已实现

- 单实例 Core 服务：CLI 通过 Windows 命名管道访问 SQLite，客户端退出不终止正在执行的本机任务。
- SQLite 项目库：媒体 SHA-256 证据、转录、翻译、软剪辑、Agent 短租约、待审补丁和不可变版本快照；导出前审计会阻止缺失或哈希变化的原片。
- JSON CLI：`health`、`import`、`project`、`transcript`、`task`、`workflow`、`cut`、`media`、`video`、`speech`、`speaker`、`audit`、`transcribe`。
- FFmpeg 音频规范化与 `whisper.cpp` JSON 转录；模型由调用者显式选择，本地处理不上传媒体。
- SRT、VTT、ASS、Markdown 导出；已应用的软剪辑和 Agent 语义剪辑使用同一时间映射重排字幕与视频。
- 一次性代理 MP4、波形和关键帧；后台视频导出支持字幕烧录、进度、取消、磁盘检查和 JSON 清单。
- Tauri 2 + React 桌面应用：项目列表、媒体导入、本地转录、字幕定位与编辑、三方差异审阅、软剪辑预览、版本恢复、SRT 和 MP4 导出。
- 三档按需模型管理：显示来源、体积、许可证和 SHA-256；后台下载支持暂停、断点续传、校验和移除。
- Windows 安装包内置 Core、whisper.cpp CPU/Vulkan 运行时和经哈希锁定的 LGPL FFmpeg；Vulkan 仅在检测到兼容显卡后启用，原始项目与模型保存在安装目录之外。
- Windows 正式版不会创建控制台窗口；Core、FFmpeg、ffprobe 和 whisper.cpp 作为隐藏子进程运行。诊断日志仅保存在 `%LOCALAPPDATA%\SiaoCut\logs`，可从「运行环境」打开。
- VAD 完全漏检且平均音量高于 `-55 dBFS` 时，自动使用同一本地模型重试一次；纯静音保持无字幕状态，音乐、合唱或特殊音色仍有一次本地重试机会。
- 0.3 语音智能：根据词级时间计算语速、停顿、口头语和低置信度证据；通过 FFmpeg 检查响度、真峰值、静音和疑似削波；所有结果只用于定位和人工审阅。
- 可选本地说话人轨：显式安装前显示 61.4 MB 下载体积、组件来源和许可证，安装后逐文件校验 SHA-256；支持改名、合并、重新分配和项目历史恢复。
- 桌面应用通过 Tauri Rust 层调用 Core，不直接读取 SQLite。媒体预览按项目动态授权，不开放任意磁盘读取范围。

## 本机依赖（本次已安装）

- Rust stable x64（Rustup）与 Visual Studio 2022 C++ Build Tools。
- CMake 4.4。
- FFmpeg（从现有 `PATH` 发现）。
- `%LOCALAPPDATA%\SiaoCut\bin\whisper-cli.exe` 与同目录 DLL；由 `whisper.cpp` Release/x64 构建而来。
- `%LOCALAPPDATA%\SiaoCut\models\ggml-tiny.en.bin` 仅作英语转录验证模型。它是显式下载的模型，不会在 CLI 运行时自动下载。

`whisper.cpp` 源代码及构建目录位于本机 `third_party/whisper.cpp`，由 `.gitignore` 排除；发布前应由安装器重新下载、验证哈希并生成第三方许可证清单。

## 使用

```powershell
# 验证 Rust Core、FFmpeg 与 whisper.cpp
.\skills\siaocut\bin\siaocut.ps1 --json health

# 创建项目并转录（模型路径必须明确给出）
.\skills\siaocut\bin\siaocut.ps1 --json import "C:\Videos\talk.mp4" --title "产品发布口播"
.\skills\siaocut\bin\siaocut.ps1 --json transcribe <projectId> --model "$env:LOCALAPPDATA\SiaoCut\models\ggml-tiny.en.bin" --language en

# 审阅、导出
.\skills\siaocut\bin\siaocut.ps1 --json cut detect <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json media prepare <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json audit <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json transcript export <projectId> --format srt -o "C:\Exports\talk.srt"
.\skills\siaocut\bin\siaocut.ps1 --json video export <projectId> -o "C:\Exports\talk.mp4" --burn-subtitles
.\skills\siaocut\bin\siaocut.ps1 --json video status <jobId>
```

默认目录为 `%LOCALAPPDATA%\SiaoCut`；开发或测试时可设置 `SIAOCUT_HOME` 覆盖。可通过 `SIAOCUT_FFMPEG`、`SIAOCUT_FFPROBE`、`SIAOCUT_WHISPER_CLI` 指向经审计的替代二进制。

运行验证：

```powershell
npm test
cargo build --release
npm run test:ui
npm run test:e2e
powershell -ExecutionPolicy Bypass -File skills\siaocut\tests\workflow-e2e.ps1
powershell -ExecutionPolicy Bypass -File skills\siaocut\tests\video-export-e2e.ps1
powershell -ExecutionPolicy Bypass -File skills\siaocut\tests\video-duration-matrix.ps1 -Source "C:\Videos\talk.mp4"
powershell -ExecutionPolicy Bypass -File skills\siaocut\tests\voice-intelligence-e2e.ps1 -InstallSpeakerPackage
```

## 0.3 语音智能

语音节奏和音频质量分析不需要新增模型。说话人轨为可选组件，未安装时不会阻止转录、字幕编辑或导出。

```powershell
# 查看节奏和音频质量证据
.\skills\siaocut\bin\siaocut.ps1 --json speech analyze <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json speech audio-start <projectId>

# 查看固定来源、体积与许可证，再显式安装说话人模型包
.\skills\siaocut\bin\siaocut.ps1 --json speaker package
.\skills\siaocut\bin\siaocut.ps1 --json speaker install
.\skills\siaocut\bin\siaocut.ps1 --json speaker package --verify

# 生成待审阅说话人轨
.\skills\siaocut\bin\siaocut.ps1 --json speaker analyze <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json speaker track <projectId>
```

完整操作与限制见 [`docs/voice-intelligence-0.3.md`](docs/voice-intelligence-0.3.md)。

启动桌面开发环境：

```powershell
npm --prefix apps/desktop install
npm run desktop:dev
```

生成 Windows 安装包：

```powershell
npm run desktop:build
```

安装包生成到 `apps/desktop/src-tauri/target/release/bundle/nsis/`。构建过程会把 Release Core 作为 sidecar，从 `release/runtime-manifest.json` 指定的来源取得 FFmpeg 与 whisper.cpp CPU 运行时，并从锁定的 whisper.cpp 提交构建 Vulkan 运行时；归档文件必须通过固定 SHA-256，模型不会随安装包静默附带。包含 Vulkan 的发布构建机需要 Vulkan SDK。

模型管理命令：

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json model list
.\skills\siaocut\bin\siaocut.ps1 --json model install base
.\skills\siaocut\bin\siaocut.ps1 --json model status <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json model cancel <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json model verify base
```

打开静态设计原型：

```powershell
npm run prototype
```

若 4311 已被占用，使用 `$env:PORT=4312; npm run prototype`。

## Agent 工作方式

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json workflow create <projectId> --kind translate --lang en
.\skills\siaocut\bin\siaocut.ps1 --json task claim --worker codex-1
.\skills\siaocut\bin\siaocut.ps1 --json task submit <taskId> --worker codex-1 --response "C:\Temp\siaocut-response.json"
```

Agent 仅取得文本、时间戳和结构约束，不能读取媒体路径。`task submit` 只创建待审补丁；只有明确执行 `task review` 或 `task review-all` 后才会修改项目。软剪辑只有在显式 `apply` 后才生效，且可恢复。

Agent 处理过程中可使用 `task heartbeat` 更新进度和续租；`task fail`、`task retry`、`task cancel` 和 `task events` 用于失败恢复、取消与 App 进度展示。提交文件必须包含领取任务时返回的 `baseVersionId`，项目被人工修改后旧结果不会静默覆盖。

## 边界

Vulkan GPU 运行时已在 GTX 1660 SUPER 上完成真实视频验证；0.3 语音智能已完成本地合成基准，但授权创作者素材、方言、重叠语音和真实噪声仍需扩充。当前发布候选仍未取得受信任的 Windows 代码签名证书，未签名的本机构建不得标记为正式发布版本。
