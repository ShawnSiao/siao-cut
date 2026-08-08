# 快速字幕时间安全模式

[简体中文](quick-transcription-timing.md) | [English](quick-transcription-timing.en.md)

快速转写使用标准化的 16 kHz 单声道 WAV。只有当当前 whisper.cpp 运行时通过固定的原始媒体时间轴验收时，Core 才会启用内部 VAD；元数据缺失、身份不符、证据损坏或尚未验收时，会自动使用无 VAD 安全路径。两条路径都必须在写入前通过字幕段与词级时间校验，避免静音被压缩后字幕逐渐提前。

## 写入规则

Core 在修改项目之前校验整份结果：

- 字幕段与词级时间必须为有限非负数，结束时间必须晚于开始时间；
- 时间必须保持顺序，并且不得超出标准化音频时长；
- 词级时间必须位于所属字幕段前后 `0.5 秒` 范围内；
- 非空字幕段必须包含可信词级时间，不能使用按字符平均分配的时间；
- 空白、特殊标记和零时长标点不单独保存为词，标点会并入相邻词；
- 任一检查失败时返回 `transcription_timing_invalid`，当前字幕、译文、剪辑和历史版本保持不变。

通过能力门控并实际启用 VAD 时，成功响应包含：

```json
{
  "timingValidation": {
    "status": "verified",
    "timeDomain": "original_media",
    "mode": "whisper_verified_vad",
    "vadUsed": true,
    "segmentCount": 12,
    "wordCount": 87
  }
}
```

安全回退时，`mode` 为 `whisper_no_vad`，`vadUsed` 为 `false`。两种模式的 `timeDomain` 都必须为 `original_media`。

## 重新生成已有字幕

旧项目不会自动迁移或自动调整时间。VAD 压缩后的原始词级时间不包含可靠的反向映射，直接重排可能继续产生错误。

在已关联媒体并已有字幕的项目中，打开「更多命令」，选择「重新生成快速字幕」。桌面端会先读取替换预检结果：

```powershell
siaocut-core --json transcript replacement-preflight <projectId>
```

只有剪辑、Agent 建议和任务基线都未引用当前字幕时才能继续。确认后，桌面端会绑定预检返回的版本 ID，并显式请求替换：

```powershell
siaocut-core --json transcribe <projectId> `
  --model <modelPath> `
  --language auto `
  --expected-version <currentVersionId> `
  --confirm-replace
```

项目在确认后发生变化时，Core 会拒绝旧请求。成功替换会创建可撤销版本；原片和既有导出文件不会修改。

## 运行时能力门控

Core 不信任单独的「VAD 模型已安装」状态。启用 VAD 前会重新检查：

- 运行时来自 [`release/whisper-runtime-source.json`](../release/whisper-runtime-source.json) 固定的源码提交和补丁；
- `whisper-cli.exe` 的实际 SHA-256 与运行时元数据、文件清单和验收证据一致；
- 验收证据来自固定的 `speech-silence-speech` 样例，后端与当前选择一致；
- 证据状态为 `passed`，词级时间属于 `original_media`，且没有词落在所属字幕段之外。

发布准备会分别构建并验收 CPU 与可选 Vulkan 运行时。CUDA 使用相同的源码构建和验收入口，但只有安装 CUDA Toolkit、完成真实 CUDA 执行并生成绑定证据后才能被选择；没有证据时继续无 VAD 安全回退。

`health` 中的 `engines.vad` 会区分：

- `verified`：VAD 模型存在，当前运行时的时间轴能力已验证；
- `safe_fallback`：VAD 模型存在，但当前运行时未通过能力门控；
- `not_configured`：没有可用的 VAD 模型。

桌面端对应显示「VAD 时间轴已验证」或「无 VAD 安全回退」，不会把模型存在误报为运行时已验证。

## 能力边界

- 当前安全模式不启用 Forced Alignment。
- MOSS 多人长音频转写使用独立的候选结果与应用流程，不受此模式影响。
- 已验证运行时只解决 whisper.cpp VAD 的原始媒体时间域映射，不替代人工校对、复杂噪声测试或 Forced Alignment。
- 手工指定、旧版或来源不明的 whisper.cpp 仍可走无 VAD 路径，但不能声明 VAD 时间轴已验证。
