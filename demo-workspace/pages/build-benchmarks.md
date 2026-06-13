# Build Benchmarks

Machine: Intel i5-660 class, Windows
Toolchain: stable-x86_64-pc-windows-gnu (via rust-toolchain.toml)
Profiles: debug=1, codegen-units=256, incremental=true
Linker: default GNU ld

## Clean build (cargo check)

| Metric | Time |
|--------|------|
| `cargo check` (clean) | 7m 33s (453.6s) |

## Incremental builds (cargo check)

| Change | Time |
|--------|------|
| Touch `widget.rs` (UI layer) | 25.8s |
| Touch `editor/mod.rs` (core logic) | 33.1s |

## Observations

- The vast majority of build time is spent compiling `wgpu`, `naga`, and their transitive dependencies
- Incremental builds on pure-logic changes (editor, vim) are ~30s
- The `debug=1` profile saves ~5-10% on incremental link time versus full debug info
- `codegen-units=256` maximizes parallel codegen during development

## Notes

- D: drive filesystem does not support hard links, causing a cargo warning during incremental compilation. No functional impact.
- Using `rust-lld` linker is not viable on GNU toolchain without full MSVC SDK setup
