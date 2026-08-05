# Windows 应用程序包验收记录

本文记录 SiaoCut Windows 应用程序包的可复现验收要求。候选包仅用于本地发布准备，不属于正式 Release。

当前包配置为 `app-only`：安装包只包含桌面主程序、`siaocut-core`、前端资源、图标和静态组件元数据。FFmpeg、FFprobe、Whisper CPU/Vulkan、VAD、模型权重和 `yt-dlp` 均由共享 `Component Store` 在安装包外部按需管理。

## 候选包

| 项目 | 结果 |
| --- | --- |
| 源码提交 | 本批变更（见 Git commit） |
| 文件名 | `SiaoCut_0.2.0_x64-setup.exe` |
| 文件大小 | 6,634,877 字节（约 6.33 MiB） |
| SHA-256 | `bdaaaf31e411d91ab9a6eeb2971f6fe885461724434e24f24643cc1991847677` |
| 构建时间 | 2026-08-02 18:11:51（Asia/Shanghai） |
| Authenticode | `NotSigned`，符合本轮未签名候选包范围 |
| 测试系统 | Windows 10 22H2，Build 19045 |

此前 0.2.0 候选包包含运行时文件的记录只代表历史包，不代表当前 `app-only` 包。当前候选包由 `npm run desktop:build` 生成，不读取正式签名材料，也不会在构建前下载或编译运行时组件。

## 自动验收结果

| 检查项 | 状态 | 证据与边界 |
| --- | --- | --- |
| Release 构建与 NSIS 打包 | 通过 | Tauri 生成 1 个 NSIS 安装包，退出码为 0 |
| 无控制台窗口 | 通过 | 桌面窗口存在；控制台窗口与 Shell 子进程均为 0 |
| Core CLI JSON 健康检查 | 通过 | `status=ok`，API 版本为 `0.1` |
| 隔离安装与桌面启动 | 通过 | 独立的 `SiaoCut Acceptance` 产品安装到临时目录并成功启动 |
| Core Sidecar 与 app-only 包边界 | 通过 | `siaocut-core` 和静态清单存在；运行时目录、可执行文件和模型权重不存在 |
| 缺少依赖时启动 | 通过 | 桌面应用可以启动，Core 健康检查返回 `not_configured`，不会把缺失依赖视为安装失败 |
| 共享组件边界 | 通过 | 安装包不携带运行时或模型；正式执行只接受 common v2 的已验证组件，旧 `SIAOCUT_*` 路径仅作为迁移输入 |
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

## 包体积与静态资源边界

- 清洁构建后的安装包为 6,634,877 字节，低于 SiaoVPlay 的约 `30 MB` 参考值。
- 验收脚本记录压缩包和安装目录大小；超过 `50 MiB` 时阻断并列出最大文件。
- `notices/runtime-manifest.json` 只用于展示 canonical catalog、版本和许可信息，不表示对应组件已随包安装；运行时归档统一来自 `ShawnSiao/siao-components`。

## 复现命令

```powershell
npm run desktop:build

$corePath = & .\skills\siaocut\bin\resolve-core-path.ps1 -Profile Release
$tauriMetadata = cargo metadata `
  --manifest-path apps/desktop/src-tauri/Cargo.toml `
  --no-deps `
  --format-version 1 | ConvertFrom-Json
$desktopPath = Join-Path $tauriMetadata.target_directory "release\siaocut-desktop.exe"
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-no-console-windows.ps1 `
  -DesktopPath $desktopPath `
  -CorePath $corePath

powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-installer-retention.ps1
```
