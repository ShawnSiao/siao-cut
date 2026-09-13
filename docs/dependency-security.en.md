# Dependency Security

[简体中文](dependency-security.md) | [English](dependency-security.en.md)

## Recorded assessment — July 21, 2026

This page preserves dated results and their scope, not an ongoing security guarantee. The September 13 documentation update did not rerun dependency audits. Recheck the current lockfiles before release.

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

## Desktop contract generation record — September 12, 2026

The root manifest added and pinned `ts-rs 12.0.1` to generate TypeScript types from Rust editing request and response DTOs. `tools/generate-core-contract.mjs --check` continues to check drift. `ts-rs` and `ts-rs-macros` use MIT; the new indirect dependencies `termcolor` and `winapi-util` use Unlicense or MIT.

The recorded `cargo audit` run for the root `Cargo.lock` found no known vulnerabilities and reported the existing yanked `chacha20 0.10.1` release. That result did not cover the desktop's separate lockfile. Generated TypeScript does not implement Serde's `deny_unknown_fields` as runtime validation; Core deserialization still enforces it.

On the same date, the native desktop bridge reused Core's `platform_contract.rs` and pinned the same `ts-rs 12.0.1` in its own manifest. Runtime information, update policy, and update events are generated from actual Rust serialization types; this adds no network behavior. A separate audit of the desktop lockfile recorded zero vulnerabilities, with existing unmaintained-dependency and `glib` unsoundness warnings retained under the platform boundary above. The native library build and 31 unit tests passed. These historical results do not establish installer or native UI acceptance.
