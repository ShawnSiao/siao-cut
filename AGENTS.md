# SiaoCut 仓库协作规则

本文件适用于整个仓库。更具体的目录规则可由下级 `AGENTS.md` 覆盖。

## 变更范围

- 一次提交只处理一个可说明、可验证的目标。
- 不提交密钥、本机配置、真实用户媒体、模型、安装包、日志、数据库或构建输出。
- 内部产品规划保存在仓库外；`docs/goal/` 只用于本地实施状态，不进入公开仓库。
- 不读取或修改与当前任务无关的本地文件。

## 分支与提交

- 分支使用 `feat/`、`fix/`、`docs/`、`chore/`、`refactor/`、`test/`、`security/` 或 `design/` 前缀。
- 提交遵循 Conventional Commits，例如 `feat(core): add subtitle validation`、`fix(ui): preserve timeline selection`。
- 从最新 `main` 创建短期分支，通过 PR 合并；禁止在功能分支中混入无关格式化或生成文件。

## 实现要求

- 新功能必须说明使用目的、能力边界、测试方法和文档影响。
- Bug 修复必须记录复现条件，并增加可稳定复现问题的回归测试。
- 依赖更新必须说明原因，提交对应锁文件，并检查许可证、已知漏洞和构建影响。
- Agent 只能接收文本、时间戳和结构约束，不得获得媒体路径或用户私有内容。
- 会修改项目的操作必须保持显式确认、版本检查和可恢复性。

## 验证

按变更范围运行最小充分测试；影响共享逻辑或发布流程时运行完整检查：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
node --test tests/core.test.mjs tests/cli.test.mjs
npm ci --prefix apps/desktop
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
npm --prefix apps/desktop run test:e2e
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1
```

提交物边界见 [`docs/repository-artifact-policy.md`](docs/repository-artifact-policy.md)，贡献流程见 [`CONTRIBUTING.md`](CONTRIBUTING.md)。
