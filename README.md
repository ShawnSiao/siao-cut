# SiaoCut

[简体中文](README.md) | [English](README.en.md)

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![Platform: Windows 10/11](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D4)
![Status: Development](https://img.shields.io/badge/status-development-orange)

SiaoCut 是面向 AI 口播创作者的 Windows 本地优先剪辑工作台。它以文稿和字幕为主要编辑入口，在本机完成媒体导入、语音转写、字幕审阅、软剪辑和视频导出。

> **项目状态：开发中。** 当前仓库尚未发布安装包，也没有经过受信任 Windows 代码签名的公开版本。源码可以构建和运行，但不应视为正式发布版本。

## 工作流程

1. 导入本地媒体，或在确认拥有处理权限后导入公开单视频 URL。
2. 选择本地 Whisper 模型，使用 FFmpeg 和 whisper.cpp 生成带时间信息的文稿。
3. 编辑字幕，审阅 Agent 建议、语音证据和软剪辑，再决定是否应用修改。
4. 导出字幕或 MP4；视频导出与字幕重排共用同一套时间映射。

## 当前能力

| 功能 | 当前实现 |
| --- | --- |
| 本地转写 | 使用 FFmpeg 规范化音频，通过 whisper.cpp 在 CPU 或兼容的 Vulkan GPU 上转写；模型由使用者明确选择。 |
| 文稿剪辑 | 支持字幕定位与编辑、翻译审阅、软剪辑、撤销、重做和版本恢复。原片不会被覆盖。 |
| 语音证据 | 标记语速、停顿、口头语、低置信度、响度、静音和疑似削波；可选说话人模型用于生成待审阅说话人轨。 |
| Agent 审阅 | Agent 只接收文本、时间戳和结构约束。结果以三方差异形式待审，不会直接改写项目。 |
| 导出 | 支持 SRT、VTT、ASS、Markdown 和 MP4，可烧录字幕并导出原始比例或 9:16 画布。 |
| 项目完整性 | Rust Core 是唯一写入者；SQLite 保存项目版本，媒体 SHA-256 审计会在原片缺失或变化时阻止导出。 |

## 设计边界

- 当前只支持 Windows 10 和 Windows 11。
- 媒体处理在本机完成。模型、运行时和 URL 媒体只在明确操作后从标示来源下载。
- 桌面应用、CLI 和 Skill 都通过 Rust Core 修改项目，不直接写入 SQLite。
- 语音分析和 Agent 结果只提供证据或建议；应用文本修改和剪辑前需要人工确认。
- 真实方言、重叠语音、复杂噪声和更多硬件组合仍需要扩充验证。

## 从源码开始

### 环境要求

- Windows 10 或 Windows 11
- Git
- Rust stable 与 Visual Studio 2022 C++ Build Tools
- Node.js 22 或更高版本
- Microsoft Edge WebView2 Runtime

### 启动桌面应用

```powershell
git clone https://github.com/ShawnSiao/siao-cut.git
cd siao-cut
npm ci --prefix apps/desktop
cargo build --release
npm run desktop:dev
```

开发模式可以在本机运行界面。转写与导出前，先检查 FFmpeg、whisper.cpp 和模型状态：

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json health
```

默认数据目录为 `%LOCALAPPDATA%\SiaoCut`。开发和测试可以使用 `SIAOCUT_HOME` 覆盖；`SIAOCUT_FFMPEG`、`SIAOCUT_FFPROBE` 和 `SIAOCUT_WHISPER_CLI` 可指向经过核验的本机二进制。

完整 CLI 工作流见 [`skills/siaocut/SKILL.md`](skills/siaocut/SKILL.md)。

受邀英文创作者应按 [English Creator Source Beta 指南](docs/english-creator-beta.md)运行，并遵守其中的 Codex Worker、故障恢复、隐私和反馈要求。

## 开发与验证

```powershell
# Rust Core 与 Node.js 合同测试
npm test

# Desktop 构建、组件测试与浏览器端到端测试
npm --prefix apps/desktop run build
npm run test:ui
npm run test:e2e

# 仓库提交物检查
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1
```

完整环境、分支、提交和 Pull Request 规则见 [`CONTRIBUTING.md`](CONTRIBUTING.md)。

## 仓库结构

```text
src/                  Rust Core、SQLite、CLI 与本机媒体适配器
apps/desktop/         Tauri 2 + React 桌面应用
skills/siaocut/       Agent Skill、PowerShell 入口与端到端测试
docs/                 专题文档与仓库规范
release/              固定运行时来源、哈希与第三方许可证
tools/                构建、发布和仓库检查工具
```

## 文档

- [系统架构](ARCHITECTURE.md)
- [0.3 语音智能](docs/voice-intelligence-0.3.md)
- [英文创作者源码 Beta](docs/english-creator-beta.md)
- [发布与更新](docs/release-updates.md)
- [仓库提交物规范](docs/repository-artifact-policy.md)
- [贡献指南](CONTRIBUTING.md)
- [第三方软件说明](THIRD_PARTY_NOTICES.md)

问题与功能建议可通过 [GitHub Issues](https://github.com/ShawnSiao/siao-cut/issues) 提交。提交日志、截图或示例前，需要移除媒体内容、本机路径和个人信息。

## 许可证

SiaoCut 使用 [Apache License 2.0](LICENSE)。随发布构建使用的第三方组件适用各自许可证，详见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。
