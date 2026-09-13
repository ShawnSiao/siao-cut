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

桌面后台任务与 CLI 共用运行时选择、VAD 门控和时间校验。通过门控并实际启用 VAD 时，CLI 转写响应及后台任务保存的候选结果包含以下时间校验信息；后台启动响应只表示任务已登记：

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

在已关联媒体并已有字幕的项目中，打开「更多命令」，选择「重新生成快速字幕」。桌面端会先检查当前版本和替换影响；只有剪辑、Agent 建议和任务基线都未引用当前字幕时才能启动。

确认后登记后台转写任务，可从顶部「任务」查看状态。计算完成后，已有字幕或项目版本变化都会使结果保留为待审核候选。先查看实际字幕和替换段数，再确认应用；应用时再次检查项目版本和替换条件。成功替换会创建可撤销版本，原片和既有导出文件不会修改。

CLI 的同步替换入口仍可使用。先读取预检结果：

```powershell
siaocut-core --json transcript replacement-preflight <projectId>
```

确认替换范围后，使用预检返回的版本 ID 显式请求替换：

```powershell
siaocut-core --json transcribe <projectId> `
  --model <modelPath> `
  --language auto `
  --expected-version <currentVersionId> `
  --confirm-replace
```

同步 CLI 会在计算完成后直接尝试写入已确认的替换；项目版本或原始媒体发生变化时拒绝写入。它与桌面共用转写执行规则，但不提供桌面的候选审核界面。

## 运行时能力门控

Core 不信任单独的「VAD 模型已安装」状态。启用 VAD 前会重新检查：

已经选择的运行时文件缺失或 SHA-256 变化时，转写会拒绝执行，要求重新核验；不会静默换用其他可执行文件。无 VAD 回退适用于可用运行时缺少有效 VAD 能力证据或模型的情况。

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

- 桌面与 CLI 共用转写执行入口的修复目前位于 [Unreleased](../CHANGELOG.md#unreleased)，既有预览安装包不包含此修复。
- 当前安全模式不启用 Forced Alignment。
- MOSS 多人长音频转写使用独立的候选结果与应用流程，不受此模式影响。
- 已验证运行时只解决 whisper.cpp VAD 的原始媒体时间域映射，不替代人工校对、复杂噪声测试或 Forced Alignment。
- 手工指定、旧版或来源不明的 whisper.cpp 仍可走无 VAD 路径，但不能声明 VAD 时间轴已验证。

## 回归验证

复现旧问题：为同一运行时配置有效的 VAD 验收证据及模型，分别运行 CLI 与桌面后台转写；旧后台路径未启用 VAD，且候选结果固定记录 `vadUsed: false`。共用执行入口后，两条路径应采用相同门控，记录实际模式，并保持原始媒体时间。

本机已准备运行时与模型时，可执行以下集成检查。脚本不下载组件、不使用现有项目；在被忽略的 `.tmp-whisper-background-*` 目录中生成短音频、独立项目和报告。

```powershell
node tools/test-whisper-background.mjs `
  --core <Core可执行文件> `
  --whisper <已验收的whisper-cli.exe> `
  --model <本地Whisper模型> `
  --vad-model <本地VAD模型> `
  --sample <英文语音WAV样例> `
  --backend cpu
```

脚本检查 VAD 与无 VAD 的 CLI／后台结果一致性、静音后的时间位置、已有字幕候选审核和运行时哈希异常拒绝。Vulkan 或 CUDA 必须使用对应的已验收运行时，并分别设置 `--backend vulkan` 或 `--backend cuda`；CPU 通过不代表其他后端通过。
