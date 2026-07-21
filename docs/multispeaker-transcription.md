# MOSS 多人长音频转写

[简体中文](multispeaker-transcription.md) | [English](multispeaker-transcription.en.md)

MOSS 多人长音频转写是高级实验能力，适用于访谈、播客、会议和课程等多人素材。它在一次推理中生成分段时间、匿名说话人标签和正文，再由 SiaoCut 校验并写入可恢复的项目版本。

该能力不随 SiaoCut 安装 MOSS、Python、CUDA、模型权重或推理框架。需要先在同一台机器上独立启动兼容的本机服务。

## 服务要求

SiaoCut 只接受以下服务地址：

- `http://127.0.0.1:<端口>`
- `http://localhost:<端口>`
- `http://[::1]:<端口>`

地址必须使用 HTTP，不得包含凭据、查询参数、片段或 `/v1` 等 API 路径。远程主机和 HTTPS 地址会被 Core 拒绝。

服务需要提供 OpenAI 兼容接口：

- `GET /v1/models`：健康检查。
- `POST /v1/audio/transcriptions`：接收临时 WAV，返回 `verbose_json`。

当前默认模型为 `OpenMOSS-Team/MOSS-Transcribe-Diarize`。模型团队推荐通过 SGLang Omni 或 vLLM 提供兼容接口；CUDA 版本、推理框架版本和启动参数变化较快，应以 [官方模型卡](https://huggingface.co/OpenMOSS-Team/MOSS-Transcribe-Diarize) 和 [官方代码仓库](https://github.com/OpenMOSS/MOSS-Transcribe-Diarize) 为准。

## 配置与健康检查

服务启动后，在桌面应用的「运行环境」中找到「MOSS 多人长音频服务」，填写服务根地址和模型标识，再点击「保存并检查」。只有状态显示「服务可用」时才能启动多人转写。

也可以使用 CLI：

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription configure `
  --endpoint http://127.0.0.1:8000 `
  --model OpenMOSS-Team/MOSS-Transcribe-Diarize

.\skills\siaocut\bin\siaocut.ps1 --json transcription health
```

`providerHealth.state` 必须为 `healthy`。`status=ok` 只表示 Core 命令执行成功，不代表外部 MOSS 服务可用。

## 启动与观察任务

打开已关联本地媒体的项目，选择「多人长音频」模式并启动转写。Core 会执行以下操作：

1. 重新校验原始媒体 SHA-256。
2. 使用 FFmpeg 生成临时 16 kHz WAV。
3. 把临时 WAV 发送到已确认的本机回环服务。
4. 校验响应中的时间范围、顺序、正文和说话人标签。
5. 原子写入字幕、说话人轨和复核项，或保留为待确认候选结果。

临时 WAV 会在任务结束后删除。原始响应保存在 SiaoCut 数据目录的 `transcription-runs` 中，用于中断恢复和结果审计，不进入公开仓库。

CLI 示例：

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription start <projectId> `
  --language zh `
  --hotword SiaoCut `
  --hotword MOSS

.\skills\siaocut\bin\siaocut.ps1 --json transcription latest <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription status <jobId>
```

Prompt 与热词属于实验输入。模型可能忽略这些内容，不应把它们视为强制词典或内容规则。

## 结果确认与冲突处理

如果转写期间项目没有发生变化，Core 会把字幕和说话人轨作为同一个可撤销版本写入。

如果项目版本或原始媒体发生变化，结果不会覆盖当前内容：

- 原始媒体哈希变化：任务失败，必须重新定位字节一致的原片。
- 项目版本变化：任务进入 `awaiting_apply`，界面显示候选结果。
- 应用候选结果：需要重新检查影响，并明确确认替换当前字幕与说话人轨。
- 丢弃候选结果：删除待应用结果，不修改当前项目。

MOSS 结果当前没有词级时间戳，因此词范围剪辑会停用。分段编辑、说话人复核、字幕导出和结构化导出仍可使用。

## 复核与导出

复核项包括快速说话人切换、极短分段和缺少标点。错误项会阻止结构化导出；警告项需要明确确认后才能继续。

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription review <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription resolve <itemId> --action resolved

.\skills\siaocut\bin\siaocut.ps1 --json transcription export <projectId> `
  --format json `
  --output C:\Temp\multispeaker.json `
  --include-speaker-labels
```

说话人标签是素材内部的匿名相对标签，不代表真实身份。合并、改名和重新分配前需要人工核对声音与上下文。

## 中断与恢复

- 服务不可用：任务失败，不会回退到 Whisper；恢复服务后执行「继续」。
- 应用重启：运行中的任务会标记为中断；重新打开项目后显式继续。
- 用户取消：未完成结果不会修改项目。
- 应用阶段失败：已准备的原始结果会保留，可在校验通过后恢复，不需要重复推理。
- 项目已修改：候选结果保持隔离，直到明确应用或丢弃。

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json transcription resume <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription cancel <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json transcription discard <jobId>
```

## 能力边界

- 只连接本机回环服务，不保存 API 密钥，不支持远程 MOSS 服务。
- SiaoCut 不管理外部服务进程、GPU 驱动、CUDA、Python 环境或模型下载。
- 模型支持的语言、时长和硬件范围不等于 SiaoCut 已完成真实验收的范围。
- 当前仍缺少真实 MOSS 服务、长音频、复杂噪声和多语言素材的完整产品验收。
- 不可靠字幕不得因为任务完成而直接视为可发布内容；仍需逐段复核。
