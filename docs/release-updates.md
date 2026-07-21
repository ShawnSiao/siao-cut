# SiaoCut 签名更新发布

[简体中文](release-updates.md) | [English](release-updates.en.md)

## 发布状态与术语

截至 2026 年 7 月 21 日，GitHub 仓库没有标签或 Release。源码可以构建，Windows 10 未签名候选包已经完成部分本地验收，但仍不属于公开发行版。

| 状态 | 含义 | 当前情况 |
| --- | --- | --- |
| 源码 Beta | 从仓库构建，需要完整开发环境，只适合受邀测试 | 可用；外部 Creator Beta 验收仍未完成 |
| 未签名候选包 | 本地生成、`NotSigned`，用于安装与恢复测试 | 已生成 Windows 10 候选包；不公开上传 |
| GitHub prerelease | 经过正式签名并上传，仍需真实升级与恢复验收 | 尚未创建 |
| 正式 Release | 签名、校验、SBOM、来源证明和 Windows 10/11 验收全部通过 | 尚不可用 |

候选包证据见 [Windows 候选包验收记录](windows-candidate-acceptance.md)。未完成项不得通过改名、手工上传或取消 prerelease 标记来绕过。

SiaoCut 的 Windows 更新同时使用 Tauri 更新签名和 Authenticode。发布构建还会在 `latest.json` 中记录安装包的 SHA-256 与大小。任一校验未通过时，桌面端不会执行安装器。

## 密钥边界

- Tauri 更新公钥可以提交或注入构建配置。
- Tauri 更新私钥必须保存在仓库之外，并保留离线备份。丢失私钥后，既有安装无法继续验证新版本。
- Authenticode 证书必须包含代码签名用途和私钥，并安装在 `Cert:\CurrentUser\My`。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 只通过进程环境传入，不写入 `.env`、脚本参数或仓库文件。

生成 Tauri 密钥时，使用官方 CLI，并把私钥写入仓库之外的受控目录：

```powershell
npm --prefix apps/desktop run tauri signer generate -- -w C:\secure\siaocut-updater.key
```

## 发布前检查

GitHub 发布检查只读取仓库、Actions 权限和 Secret 名称，不读取 Secret 值：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-release-readiness.ps1 `
  -Mode GitHub `
  -Repository '<owner>/<repo>' `
  -RequireWindows11
```

本机签名检查需要显式传入证书指纹和仓库外的密钥路径：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<从密码管理器注入>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-release-readiness.ps1 `
  -Mode Local `
  -CertificateThumbprint '<证书指纹>' `
  -UpdaterPrivateKeyPath 'C:\secure\siaocut-updater.key' `
  -UpdaterPublicKeyPath 'C:\secure\siaocut-updater.key.pub'
```

检查结果为 JSON。存在缺失项时退出码为 `2`；诊断阶段可以增加 `-AllowIncomplete`，保留缺失项但返回退出码 `0`。

## 构建发布产物

当前仓库没有固定 Git remote。发布命令必须显式传入稳定的 `latest.json` 地址和当前版本安装包地址：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '<从密码管理器注入>'
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-signed-release.ps1 `
  -CertificateThumbprint '<证书指纹>' `
  -UpdaterPrivateKeyPath 'C:\secure\siaocut-updater.key' `
  -UpdaterPublicKeyPath 'C:\secure\siaocut-updater.key.pub' `
  -UpdateEndpoint 'https://github.com/<owner>/<repo>/releases/latest/download/latest.json' `
  -DownloadBaseUrl 'https://github.com/<owner>/<repo>/releases/download/v0.2.0' `
  -ReleaseNotes 'SiaoCut 0.2.0'
```

脚本只在以下条件全部成立时返回成功：

1. NSIS 安装包的 Authenticode 状态为 `Valid`。
2. Tauri 生成与安装包对应的 `.sig` 文件。
3. `latest.json` 包含 HTTPS 下载地址、内联 Tauri 签名、文件大小和 SHA-256。

tag 触发的工作流会先生成 SPDX JSON SBOM 和 `SHA256SUMS`，再通过 GitHub OIDC 为发布文件生成来源证明，并为安装包生成 SBOM 证明。工作流会把以下 7 个文件上传到 prerelease：

1. Windows NSIS 安装包；
2. 同名 Tauri `.sig`；
3. `latest.json`；
4. SPDX JSON SBOM；
5. `SHA256SUMS`；
6. Sigstore 来源证明包；
7. Sigstore SBOM 证明包。

GitHub 的稳定「Latest」版本和客户端更新入口在 prerelease 阶段保持不变。SBOM 由 [Anchore SBOM Action](https://github.com/anchore/sbom-action) 生成，证明由 [GitHub Artifact Attestations](https://github.com/actions/attest) 生成；两个 Action 都固定到已审核的提交。

完成真实下载、升级、数据保留和 Windows 10/11 验收后，手动运行提升工作流：

```powershell
gh workflow run promote-windows-release.yml -f tag=v0.2.0
```

提升工作流会重新下载全部 7 个发布文件，并核对文件集合、SBOM 和 Sigstore 包结构、`SHA256SUMS`、GitHub 来源证明、安装包 Authenticode、Tauri 签名、大小、版本和下载地址。全部通过后才移除 prerelease 标记并设为「Latest」。

## 客户端行为

- 正式签名构建每 24 小时最多自动检查一次，也保留手动检查入口。
- 开发构建、未注入签名更新配置的构建，以及当前 EXE 未通过 Authenticode 校验的构建，不会连接更新源。
- 只显示高于当前版本的更新；相同版本和降级版本由 Tauri 的默认 SemVer 比较拒绝。
- 安装前显示版本、变更说明和安装包大小。
- 明确确认后才下载和安装。Windows 安装阶段会关闭应用，但 SiaoCut 不调用自动重启。
- 下载后依次校验 Tauri 签名、文件大小、SHA-256 和 Authenticode。

## 本地契约验证

以下命令使用临时 Tauri 密钥和回环更新源，不需要生产私钥或证书，也不会执行安装器：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/test-local-updater.ps1
```

测试覆盖 0.1.1 升级到 0.2.0、同版本、降级、安装包篡改、SHA-256 错误、缺少 Tauri 签名和 Authenticode 非 `Valid`。回环 HTTP 只在隔离测试配置中启用；生产清单仍只接受 HTTPS 地址。

Tauri 官方说明：[Updater](https://v2.tauri.app/plugin/updater/)、[Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)。
