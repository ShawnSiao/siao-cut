# 一键工作流预设

SiaoCut 的一键工作流由 Rust Core 选择固定路径。桌面端和 Skill 只选择预设与合法参数，不自行拼接阶段。所有预设都要求输出路径并生成 MP4；项目数据库、版本历史和本地媒体仍由 Core 管理。

## 预设契约

| 预设 | 路径 | 阶段起始进度 | 人工关口 |
| --- | --- | --- | --- |
| `draft` | 导入 → 转写 → 审计 → 导出 | 0%、15%、70%、75% | 不运行建议审阅；审计记录「未经建议审阅」。 |
| `balanced` | 导入 → 转写 → 建议 → 可选翻译及审阅 → 审计 → 导出 | 0%、15%、45%、50%、75%、80% | 存在粗剪或 Agent 建议时暂停。 |
| `delivery` | 导入 → 转写 → 音频分析 → 建议 → 可选翻译及审阅 → 审计 → 导出 | 0%、10%、40%、55%、60%、80%、85% | 无论是否发现建议，都在审计前等待一次确认。 |

阶段内只有子任务提供真实进度时才映射百分比。没有连续进度的阶段保持在起始值，不进行模拟增长。

## CLI

```powershell
# 省略 --profile 时保持旧行为，使用 balanced。
siaocut --json auto start --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --output "C:\Exports\balanced.mp4"

siaocut --json auto start --profile draft --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --output "C:\Exports\draft.mp4" --subtitle-mode source

siaocut --json auto start --profile delivery --media "C:\Videos\talk.mp4" --model "C:\Models\ggml-base.bin" --output "C:\Exports\delivery.mp4"
```

`draft` 禁止 `--translate`、AI 执行参数和非 `source` 字幕模式，遇到这些参数会返回 `auto_workflow_profile_invalid`，不会静默忽略。

状态与控制命令：

```powershell
siaocut --json auto status <workflowId>
siaocut --json auto events <workflowId> --after 0
siaocut --json auto cancel <workflowId>
siaocut --json auto continue <workflowId>
```

`needs_agent` 表示翻译执行仍需完成，`needs_review` 表示必须处理建议或确认完成审阅。Agent 提交只生成候选结果；审阅前不会修改字幕或译文。

## 恢复与审计

- 旧数据库中的自动工作流迁移为 `balanced`，事件和项目关联保持不变。
- `delivery` 记录 `audioAnalysisJobId`。取消父工作流时取消活动子任务；继续时恢复或复用同一子任务。
- 导出任务同样按持久化 ID 恢复，不能因继续操作重复创建。
- 远端失败不会静默切换执行目标；当前版本也不提供局域网远程算力。
- 导出前继续检查媒体哈希、字幕质量和译文过期状态。

自动化真实流程脚本为 [`../skills/siaocut/tests/auto-workflow-e2e.ps1`](../skills/siaocut/tests/auto-workflow-e2e.ps1)。默认至少运行三次本地工作流，覆盖旧命令兼容、`draft`、`delivery`、非法参数、人工审阅、恢复和源媒体不变性。
