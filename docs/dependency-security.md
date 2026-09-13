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


## 桌面契约生成依赖

2026 年 9 月 12 日新增并锁定 `ts-rs 12.0.1`，用于从 Rust 编辑请求与响应 DTO
生成 TypeScript 类型，沿用 `tools/generate-core-contract.mjs --check` 检查漂移。
`ts-rs` 和 `ts-rs-macros` 使用 MIT 许可证；新增间接依赖 `termcolor`、
`winapi-util` 使用 Unlicense 或 MIT 许可证。

本次根目录 `Cargo.lock` 的 `cargo audit` 检查未发现已知安全漏洞；审计另报告原有
`chacha20 0.10.1` 已撤回发布，该提示不属于本次新增依赖。此结果不覆盖桌面端独立锁文件。
生成器不为 Serde 的 `deny_unknown_fields` 生成 TypeScript 运行时校验；该约束仍由
Core 反序列化执行，不能以生成类型代替输入校验。


同日，桌面原生桥接复用 Core 的 `platform_contract.rs`，在桌面独立清单中加入同一固定版本的 `ts-rs 12.0.1`。用途是让运行环境、更新策略与更新事件由实际 Rust 序列化类型生成，不增加新的联网行为。桌面独立锁文件已重新运行 `cargo audit`，漏洞计数为 0；仍存在原有的停止维护及 `glib` 不健全性提示，继续按上文的平台依赖边界处理。原生库构建与 31 个单元测试通过；这不代表安装包或原生界面验收完成。
