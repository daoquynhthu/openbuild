# R3-BASE-02 Evidence — Toolchain and host environment

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Files changed: none (evidence only)

## Rust toolchain

```
rustc 1.92.0 (ded5c06cf 2025-12-08)
binary: rustc
commit-hash: ded5c06cf21d2b93bffd5d884aa6e96934ee4234
commit-date: 2025-12-08
host: x86_64-pc-windows-msvc
release: 1.92.0
LLVM version: 21.1.3
```

```
cargo 1.92.0 (344c4567c 2025-10-21)
```

```
Default host: x86_64-pc-windows-msvc
rustup home:  <redacted>

installed toolchains
--------------------
stable-x86_64-pc-windows-msvc (default)
nightly-x86_64-pc-windows-msvc
1.92.0-x86_64-pc-windows-msvc (active)

active toolchain
----------------
name: 1.92.0-x86_64-pc-windows-msvc
active because: overridden by 'D:\grok_build\rust-toolchain.toml'
installed targets:
  aarch64-unknown-linux-gnu
  x86_64-pc-windows-msvc
  x86_64-unknown-linux-gnu
```

Rust is 1.92.0. ✅

## Protoc

```
libprotoc 35.0
```

## CMake

```
cmake not found in PATH
```

CMake is not required for the 4 targeted crates (xai-grok-provider, xai-grok-sampler, xai-grok-pager, xai-grok-pager-bin) but may be needed for full workspace builds depending on native dependencies.

## Python

```
Python 3.14.3
```

## Git

```
git version 2.54.0.windows.1
```

## Windows host

Host platform is x86_64-pc-windows-msvc. PowerShell 7 is available.

## Deviations

- cmake not in PATH (may affect full workspace builds with native deps)
- No `node --version` recorded (not requested on Windows)
