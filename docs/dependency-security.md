# 依赖安全

[简体中文](dependency-security.md) | [English](dependency-security.en.md)

## 当前评估

截至 2026 年 7 月 21 日，Dependabot 报告 `glib 0.18.5` 受 GHSA-wrw7-89jp-8q8g 影响。两个告警分别来自桌面端和更新器契约工具的 `Cargo.lock`。

SiaoCut 当前只发布 `x86_64-pc-windows-msvc` 制品。以下命令确认 `glib 0.18.5` 不进入受支持目标的依赖图：

```powershell
cargo tree --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc -i glib@0.18.5
cargo tree --manifest-path tools/updater-contract/Cargo.toml --target x86_64-pc-windows-msvc -i glib@0.18.5
```

`glib 0.20.0` 不能作为孤立锁文件更新：Tauri 2.11.5 间接依赖的 `gtk 0.18.2` 要求 `glib ^0.18`。因此，这两个告警按「受支持目标未使用」处理，并由 [Issue #12](https://github.com/ShawnSiao/siao-cut/issues/12) 跟踪上游迁移。

## 重新评估条件

出现以下任一情况时，必须重新运行目标依赖检查：

- 更新 Tauri、GTK 或 `glib`；
- 增加 Linux GTK 构建或发布目标；
- Windows 目标依赖图开始包含 `glib 0.18.5`；
- 上游发布兼容的已修复版本。

告警关闭不等于依赖已经升级，也不扩大当前平台支持范围。
