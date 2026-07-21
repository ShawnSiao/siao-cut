# Windows 候选包验收记录

本文记录 0.2.0 未签名候选包的可复现结果。候选包仅用于本地发布准备，不属于正式 Release。

## 候选包

| 项目 | 结果 |
| --- | --- |
| 源码提交 | `f24cfca6121b1df10e1fc58ccfee50ab0db29c30` |
| 文件名 | `SiaoCut_0.2.0_x64-setup.exe` |
| 文件大小 | 77,116,423 字节 |
| SHA-256 | `7c2a0bd3820a248215294a9f2baa4d4c9bb8ac236613b12c9b12c4db9aa0488b` |
| 构建时间 | 2026-07-21 12:00:45 UTC |
| Authenticode | `NotSigned`，符合本轮未签名候选包范围 |
| 测试系统 | Windows 10 22H2，Build 19045 |

候选包由 `npm run desktop:build` 生成。构建包含固定版本的 FFmpeg、whisper.cpp CPU/Vulkan 运行时、Silero VAD 模型和 yt-dlp，不读取正式签名材料。

## 自动验收结果

| 检查项 | 状态 | 证据与边界 |
| --- | --- | --- |
| Release 构建与 NSIS 打包 | 通过 | Tauri 生成 1 个 NSIS 安装包，退出码为 0 |
| 无控制台窗口 | 通过 | 桌面窗口存在；控制台窗口与 Shell 子进程均为 0 |
| Core CLI JSON 健康检查 | 通过 | `status=ok`，API 版本为 `0.1` |
| 隔离安装与桌面启动 | 通过 | 独立的 `SiaoCut Acceptance` 产品安装到临时目录并成功启动 |
| Core Sidecar 与运行时完整性 | 通过 | CPU、Vulkan、VAD、yt-dlp、运行时清单和许可证均存在；固定文件哈希一致 |
| URL 素材预检 | 通过 | 安装后的 Core 可检查授权公开 URL，确认前没有创建项目 |
| 覆盖安装 | 通过 | 同一源码分别打包为 0.1.1 和 0.2.0，验证 NSIS 覆盖安装契约 |
| 升级后数据保留 | 通过 | `%LOCALAPPDATA%\SiaoCut\retention-probes` 中的隔离探针仍存在 |
| 卸载后数据保留 | 通过 | 卸载测试产品后隔离探针仍存在 |
| 验收环境清理 | 通过 | 临时安装目录、配置、进程和卸载注册项均无残留 |

覆盖安装结果的证据类型为 `same-source-installer-contract`，`historicalBinaryUpgrade=false`。该结果只证明安装器的覆盖与保留行为，不证明已发布旧版本升级到当前版本的兼容性。`tools/test-installer-retention.ps1` 可通过 `-FromInstallerPath` 接收历史 `SiaoCut Acceptance` 安装器，以补充真实历史二进制升级证据。

## 待补验收

| 检查项 | 状态 | 后续条件 |
| --- | --- | --- |
| 真实历史版本升级 | 阻塞 | 需要同一验收产品标识的历史安装器；不得用同源码改版本号代替 |
| 正式产品安装器覆盖安装 | 未执行 | 当前机器可能存在日常安装，不能用候选包覆盖；应在隔离 Windows 账户或虚拟机执行 |
| Windows 11 安装、升级与卸载 | 阻塞 | 需要 Windows 11 Build 22000 或更高版本的独立环境 |
| 睡眠与唤醒后的任务恢复 | 未执行 | 需要在不影响当前自动化会话的专用机器上手工执行 |
| 正式 Authenticode 与 Tauri 更新签名 | 不适用 | 正式签名不在本轮范围内 |

在上述阻塞项完成前，0.2.0 只能称为「Windows 10 未签名候选包」，不能称为经过 Windows 10/11 完整升级验收的正式版本。

## 复现命令

```powershell
npm run desktop:build

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-no-console-windows.ps1 `
  -DesktopPath apps/desktop/src-tauri/target/release/siaocut-desktop.exe `
  -CorePath target/release/siaocut-core.exe

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-installer-retention.ps1
```
