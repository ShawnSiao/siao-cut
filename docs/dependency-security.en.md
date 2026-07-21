# Dependency Security

[简体中文](dependency-security.md) | [English](dependency-security.en.md)

## Current assessment

As of July 21, 2026, Dependabot reports GHSA-wrw7-89jp-8q8g for `glib 0.18.5`. The two alerts originate from the desktop and updater-contract `Cargo.lock` files.

SiaoCut currently releases only `x86_64-pc-windows-msvc` artifacts. These commands confirm that `glib 0.18.5` is absent from the supported target graph:

```powershell
cargo tree --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc -i glib@0.18.5
cargo tree --manifest-path tools/updater-contract/Cargo.toml --target x86_64-pc-windows-msvc -i glib@0.18.5
```

`glib 0.20.0` cannot be applied as an isolated lockfile update: `gtk 0.18.2`, reached through Tauri 2.11.5, requires `glib ^0.18`. The alerts are therefore dismissed as unused by the supported target and tracked in [Issue #12](https://github.com/ShawnSiao/siao-cut/issues/12) pending an upstream migration.

## Reassessment triggers

Run the target dependency checks again when any of these conditions changes:

- Tauri, GTK, or `glib` is updated;
- a Linux GTK build or release target is added;
- the Windows target graph starts resolving `glib 0.18.5`; or
- upstream provides a compatible patched dependency line.

Dismissing the alerts does not mean the dependency was upgraded and does not expand the supported platform scope.
