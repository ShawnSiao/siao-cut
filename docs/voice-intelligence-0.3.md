# SiaoCut 0.3 语音智能

SiaoCut 0.3 将语音节奏、音频质量和说话人信息整理为可定位证据。分析在本机运行，不上传媒体，也不会自动删除内容、改写字幕或应用剪辑。

## 能力范围

| 能力 | 输入 | 结果 | 是否需要新增模型 |
| --- | --- | --- | --- |
| 语音节奏 | 当前字幕和词级时间 | 词条/分钟、停顿、口头语、低置信度 | 否 |
| 音频质量 | 项目关联的本地媒体 | 综合响度、真峰值、静音区间、疑似削波 | 否，使用固定 FFmpeg |
| 说话人轨 | 项目关联的本地媒体 | 说话区间、说话人和字幕关联 | 是，可选本地模型包 |

时间轴、字幕正文和软剪辑仍由人工操作决定。语音分析失败不会阻止其他编辑流程。

## 推荐操作顺序

1. 完成本地转录并检查词级时间。
2. 在「当前段落」区域查看「语音节奏」。
3. 启动「音频质量」分析，按时间范围试听响度、静音或削波风险。
4. 需要多人区分时，打开「运行环境」，核对说话人模型包的来源、体积、许可证和 SHA-256。
5. 点击「明确安装」，等待校验完成后启动说话人分析。
6. 在「说话人轨」中改名、合并或重新分配字幕说话人。
7. 使用项目历史撤销或恢复人工调整。

## 命令行操作

### 语音节奏

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json speech analyze <projectId>
```

缺少词级时间时返回 `insufficient_evidence`，不会伪造语速或停顿指标。

### 音频质量

```powershell
$start = .\skills\siaocut\bin\siaocut.ps1 --json speech audio-start <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json speech audio-status <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json speech audio-latest <projectId>
```

后台任务支持取消和显式继续：

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json speech audio-cancel <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json speech audio-resume <jobId>
```

首版阈值随结果一并返回：静音低于 `-40 dB` 且持续至少 `0.8 s`、疑似削波真峰值不低于 `-0.1 dBFS`、偏低响度低于 `-24 LUFS`、偏高响度高于 `-14 LUFS`。

### 说话人模型包

```powershell
# 安装前检查固定来源、许可证和体积
.\skills\siaocut\bin\siaocut.ps1 --json speaker package

# 只在明确需要时安装
.\skills\siaocut\bin\siaocut.ps1 --json speaker install
.\skills\siaocut\bin\siaocut.ps1 --json speaker job-status <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json speaker package --verify
```

模型包使用 sherpa-onnx 1.13.2、pyannote segmentation 3.0 int8 和 3D-Speaker ERes2Net Base 16 kHz。下载体积为 64,389,270 字节，安装后的 5 个文件合计 56,862,907 字节。模型包不会随应用默认安装。

### 说话人分析与人工调整

```powershell
.\skills\siaocut\bin\siaocut.ps1 --json speaker analyze <projectId>
.\skills\siaocut\bin\siaocut.ps1 --json speaker job-status <jobId>
.\skills\siaocut\bin\siaocut.ps1 --json speaker track <projectId>

.\skills\siaocut\bin\siaocut.ps1 --json speaker rename <projectId> <speakerId> --name "主持人"
.\skills\siaocut\bin\siaocut.ps1 --json speaker merge <projectId> --from <speakerId> --into <speakerId>
.\skills\siaocut\bin\siaocut.ps1 --json speaker assign <projectId> <segmentId> <speakerId>
```

改名、合并和重新分配会创建项目历史。可通过 `project undo`、`project redo` 或 `project restore` 恢复。

## 状态解释

- `ready`：存在可审阅证据。
- `insufficient_evidence`：缺少词级时间，不计算语音节奏。
- `no_speech`：说话人运行时未检测到说话区间。
- `speaker_package_missing`：未安装可选模型包；其他项目操作仍可继续。
- `interrupted`：后台任务异常中断，需要显式继续。
- `failed`：分析失败；字幕、剪辑和原片保持原状。

## 能力边界

- 音频质量分析测量响度、真峰值、静音和疑似削波，不分类任意噪声或音乐。
- 说话人数和区间属于模型结果，需要试听和人工修正。
- 语音节奏中的「词条」来自当前 ASR 时间证据，不等同于语言学分词。
- 纯静音不会触发无 VAD 重试；低于 `-55 dBFS` 平均音量的输入保持无字幕状态，避免静音幻觉。
- 合成系统语音基准用于可重复回归，不能替代授权创作者、方言、重叠语音和真实环境噪声测试。

## 验收命令

```powershell
powershell -ExecutionPolicy Bypass -File skills\siaocut\tests\voice-intelligence-e2e.ps1 -InstallSpeakerPackage
```

脚本在系统临时目录生成单人、双人、中英混合、纯静音、粉红噪声叠加和双音调音乐干扰 6 类素材。素材不会提交到仓库。脚本会校验：

- 缺少说话人模型时单人项目仍可继续使用；
- 纯静音不生成字幕或语音节奏；
- 每次分析前后字幕、剪辑和时间映射保持一致；
- 所有原片 SHA-256 保持一致；
- 说话人模型包经显式安装并逐文件校验；
- 说话人数误差按素材记录，不隐藏为综合分数。

脚本兼容 Windows PowerShell 5.1 和 PowerShell 7。中文验收句以 UTF-8 Base64 常量保存，避免 Windows PowerShell 5.1 按系统代码页误读无 BOM 脚本。系统没有中文男声时，脚本会从已启用的中文语音生成确定性降调样本，并在报告中将 `pitchShiftedZhVoiceFallback` 标记为 `true`。
