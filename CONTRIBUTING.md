# 为 SiaoCut 做贡献

SiaoCut 接受 Bug 修复、功能改进、测试、文档和设计贡献。所有变更通过独立分支和 Pull Request 提交。

## 开始之前

开发环境需要：

- Windows 10 或 Windows 11
- Git
- Rust stable，包含 `rustfmt` 和 `clippy`
- Node.js 22 或更高版本
- Microsoft Edge WebView2 Runtime

安装 Desktop 依赖：

```powershell
npm ci --prefix apps/desktop
```

## 提交 Issue

- Bug 使用 Bug 模板，提供版本、Windows 版本、复现步骤、预期结果和实际结果。
- 功能建议先说明要解决的问题，再说明建议方案和可接受的替代方案。
- 文档问题提供具体文件、章节或页面地址。
- 日志、截图和示例项目前必须移除姓名、账号、绝对路径、媒体内容和其他敏感信息。
- 安全漏洞不要提交公开 Issue；等待仓库安全策略提供私密报告入口，或先通过维护者公开资料建立私下联系。

## 创建分支

从最新 `main` 创建短期分支：

```powershell
git switch main
git pull --ff-only
git switch -c fix/short-description
```

分支前缀：

- `feat/`：新增功能
- `fix/`：修复 Bug
- `docs/`：文档变更
- `chore/`：仓库维护、依赖或工具调整
- `refactor/`：不改变外部行为的重构
- `test/`：测试改进
- `security/`：安全加固
- `design/`：品牌和设计素材

分支名称使用小写英文和连字符，不包含个人姓名、临时编号或工具标识。

## 编写提交

提交信息遵循 Conventional Commits：

```text
<type>(<scope>): <summary>
```

常用类型包括 `feat`、`fix`、`docs`、`chore`、`refactor`、`test`、`build`、`ci` 和 `security`。摘要使用祈使语气，说明已经完成的单一变更。

示例：

```text
feat(core): validate subtitle timing
fix(desktop): retain selected timeline clip
docs(repo): clarify artifact requirements
```

## 变更要求

### 新功能

- 说明用户问题、使用流程和不在本次实现范围内的内容。
- 保持本地优先和隐私边界，不把媒体路径或原始内容交给 Agent。
- 为核心逻辑增加测试，并更新受影响的用户或开发文档。
- UI 变更提供去除个人信息的真实截图；不得用概念图冒充已实现功能。

### Bug 修复

- 在 Issue 或 PR 中给出最小复现步骤和根因。
- 对可自动化复现的问题增加回归测试。
- 说明修复对项目格式、数据库、导出结果和兼容性的影响。

### 依赖更新

- 说明更新原因和上游变更范围。
- 提交 `Cargo.lock` 或 `package-lock.json` 等对应锁文件。
- 检查许可证、已知漏洞、最小运行环境和构建结果。
- 不引入下载后无法审计的二进制依赖。

## 本地验证

Rust：

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Node.js Core：

```powershell
node --test tests/core.test.mjs tests/cli.test.mjs
```

Desktop：

```powershell
npm ci --prefix apps/desktop
npm --prefix apps/desktop run build
npm --prefix apps/desktop run test:ui
npx --prefix apps/desktop playwright install chromium
npm --prefix apps/desktop run test:e2e
```

仓库提交物：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1
```

## 提交 Pull Request

- PR 标题使用 Conventional Commits 格式。
- 关联对应 Issue；简单文档修正可说明无需 Issue。
- 填写改动、验证、风险、截图和提交物检查结果。
- 保持 PR 范围单一；不提交本机输出或顺手修改的无关文件。
- 所有自动检查通过并解决评审对话后再合并。
- Fork PR 使用 `pull_request` 工作流，无法读取仓库发布 Secret。

使用 AI 辅助产生的代码、文档或素材必须由提交者检查其正确性、许可证和隐私边界。提交者对最终内容负责。

## 提交物边界

允许和禁止提交的文件、非代码制品标准及自动检查规则见 [`docs/repository-artifact-policy.md`](docs/repository-artifact-policy.md)。
