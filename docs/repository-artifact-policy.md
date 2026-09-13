# 仓库提交物规范

本规范定义公开仓库中可以提交的内容、禁止内容，以及非代码制品的最低质量要求。公开仓库保留源码与可复现工具；个人资料、环境配置和运行产物仅保存在本地。

## 可以提交

- 产品源码、构建脚本、自动化工具和配置文件。
- 可重复运行的单元测试、集成测试和小型合成测试夹具。
- `Cargo.lock`、`package-lock.json` 等用于可重复构建的锁文件。
- README、贡献指南、变更记录、用户手册和稳定的接口使用、构建与验证文档。
- 第三方许可证、来源说明和运行时清单。
- 经审核的 SVG 源文件，以及从固定母版导出的必要 PNG、ICO 图标。

## 不得提交

- 密钥、令牌、密码、证书私钥、签名私钥或真实 `.env` 文件。
- 安装包、动态库、压缩包、模型权重和无法审计的第三方二进制。
- `target/`、`node_modules/`、`dist/`、测试报告、覆盖率、缓存和临时目录。
- 日志、数据库、崩溃转储、本机绝对路径和个人配置。
- 真实视频、音频、字幕、项目数据库或含个人信息的截图。
- 内部产品规划、产品需求说明、产品与交互设计、开发设计说明、设计原型、商业策略、未公开路线图和本地 Agent 实施状态。

本仓库明确保留在本地的目录包括 `designs/`、`docs/hackathon/`、`.local-tools/` 和私有设计 Skill。架构说明、本机工具兼容入口及未审核截图的精确路径记录在 [`tools/repository-local-paths.json`](../tools/repository-local-paths.json)；`.gitignore` 防止普通暂存，提交物检查器另行拒绝强制暂存的这些路径。

Python 的 `__pycache__/`、`.pyc` 和 `.pyo` 属于自动生成缓存。真实 `.env` 及其变体禁止提交；`.env.example` 可以保留无凭据的配置示例，仍需通过文本检查。公开图片、JSON、生成契约、测试夹具和通用 `tools/` 脚本不作整类排除。

禁止项应保存在以下位置之一：

- 本机私有目录。
- GitHub Actions Artifacts，用于短期验证结果。
- GitHub Releases，用于安装包、签名、校验和、SBOM 和来源证明。
- 经许可证确认的外部运行时下载源。

## 非代码制品标准

### 测试夹具

- 优先使用程序生成或明确授权的合成内容。
- 文件保持最小，只包含稳定复现测试所需的数据。
- 在相邻文档或测试代码中说明来源、用途和预期结果。
- 真实媒体只能在仓库外执行验收，不得作为 PR 附件长期公开。

### 截图与演示图

- 只能展示真实存在的界面和能力。
- 移除姓名、账号、路径、媒体缩略图和其他个人信息。
- 使用 PNG 或 WebP；避免无损保留不必要的拍摄元数据。
- 单个文件原则上不超过 1 MiB，超出时在 PR 中说明原因。

### Logo、图标与插图

- SVG 必须保留可编辑矢量结构，不嵌入位图或来源不明的字体。
- PNG、ICO 等派生文件必须能追溯到固定母版和导出规则。
- AI 生成内容只能用于已标明的概念探索，不直接作为正式 Logo 或文字成品。
- 品牌素材遵守单独的版权和商标规则，不自动适用 Apache-2.0。

### 文档与报告

- 公开文档使用 Markdown；内部规划和可编辑办公文档保存在仓库外。
- `designs/`、`ARCHITECTURE.md` 和 `docs/desktop-architecture.md` 仅保留为本地设计资料，不进入公开仓库；原型启动脚本 `tools/serve-prototype.mjs` 同样仅供本地使用。现有文件取消 Git 跟踪后由 `.gitignore` 排除，新建内部规划仍保存在仓库外。
- 从当前版本取消跟踪不会删除已发布的 Git 历史。历史清理与远端更新需要单独处理，不能将本地取消跟踪描述为远端撤回完成。
- 文档不得包含本机绝对路径、身份信息、密钥、私有链接或未公开商业信息。
- 大量原始验证记录由 CI Artifact 保存；仓库只保留稳定结论和必要复现步骤。

### 第三方材料

- 提交前确认再分发权利和许可证兼容性。
- 在 `THIRD_PARTY_NOTICES.md` 或 `release/licenses/` 中记录名称、版本、来源和许可证。
- 不提交仅有下载权、没有再分发权的模型或运行时。

## 自动检查

`tools/check-repository-artifacts.ps1` 默认检查完整暂存快照，以及工作区中的跟踪文件和未被忽略的新文件。暂存文件的内容和大小直接从 Git 对象读取：暂存后在工作区修改、缩小或删除文件，都不能掩盖索引中仍将被提交的内容。仅在取消跟踪或暂存删除后，该文件才退出暂存快照。

检查范围包括：

- 禁止目录和扩展名。
- 本地专用路径和 Python 缓存，不以体积大小作为唯一判断。
- 超过 5 MiB 的文件，分别检查索引和工作区大小。
- 常见密钥格式和私钥标记。
- 指向个人目录或工作区的本机绝对路径。
- 未解决的合并条目，以及当前不支持的符号链接或子模块条目；工作区检查不读取链接目标。

检查通过不代表内容必然适合公开。PR 提交者和评审者仍需核对授权、隐私、真实性及品牌边界。

运行方式：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1
```

仅检查本次将提交的完整快照（包括未变更的跟踪文件），不检查尚未暂存的修改：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check-repository-artifacts.ps1 -Staged
```

默认命令通过提交物检查后继续执行源码规模检查；`-Staged` 只检查提交物边界。两者都只读，不自动暂存、取消跟踪、删除文件或提交。

规则回归使用独立的临时 Git 仓库，覆盖强制暂存、暂存后修改／删除、超大索引对象、中文路径和允许保留的公开资源：

```powershell
node --test tools/test-repository-artifacts.mjs
$env:SIAOCUT_POLICY_TEST_SHELL = 'pwsh'
node --test tools/test-repository-artifacts.mjs
Remove-Item Env:SIAOCUT_POLICY_TEST_SHELL
```

CI 在 Windows PowerShell 5.1 和 PowerShell 7 下运行这些回归；无需安装额外 Node.js 依赖。
