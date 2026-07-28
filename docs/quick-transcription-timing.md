# 快速字幕时间安全模式

[简体中文](quick-transcription-timing.md) | [English](quick-transcription-timing.en.md)

快速转写使用标准化的 16 kHz 单声道 WAV，并在 whisper.cpp 内部 VAD 关闭的情况下生成词级时间。这样可保证字幕段与词级时间都属于原始媒体时间轴，避免静音被压缩后字幕逐渐提前。

## 写入规则

Core 在修改项目之前校验整份结果：

- 字幕段与词级时间必须为有限非负数，结束时间必须晚于开始时间；
- 时间必须保持顺序，并且不得超出标准化音频时长；
- 词级时间必须位于所属字幕段前后 `0.5 秒` 范围内；
- 非空字幕段必须包含可信词级时间，不能使用按字符平均分配的时间；
- 空白、特殊标记和零时长标点不单独保存为词，标点会并入相邻词；
- 任一检查失败时返回 `transcription_timing_invalid`，当前字幕、译文、剪辑和历史版本保持不变。

成功响应包含：

```json
{
  "timingValidation": {
    "status": "verified",
    "timeDomain": "original_media",
    "mode": "whisper_no_vad",
    "vadUsed": false,
    "segmentCount": 12,
    "wordCount": 87
  }
}
```

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

## 能力边界

- 当前安全模式不启用 Forced Alignment。
- MOSS 多人长音频转写使用独立的候选结果与应用流程，不受此模式影响。
- 已安装的 VAD 模型仍可用于运行时完整性检查，但快速转写不会使用它。
- 重新启用 VAD 需要运行时明确声明并验证「词级 JSON 时间属于原始媒体时间轴」能力。
