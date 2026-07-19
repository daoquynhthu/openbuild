# P0-004: Baseline Command Matrix
# Generated: 2026-07-19T10:28:29Z
# Base SHA: 1ec582912c9edce9813b0b02deed21b13f91b84a

## Disk
- Drive D:\: 33.6 GB free after cargo clean (was 6.2 GB before)
- Threshold per plan: ≥45 GB for full workspace commands
- Status: BELOW THRESHOLD — owner directed to proceed via cargo clean

## Available Platforms
- Windows (local): x86_64-pc-windows-msvc, Rust 1.92.0
- Linux: via GitHub Actions only
- macOS: via GitHub Actions only

## Commands (Windows)
- cargo check --workspace --all-targets --locked --keep-going
- cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings
- cargo test --workspace --all-targets --locked --no-run --keep-going
- cargo doc --workspace --no-deps --locked

## P0-004 Rules
- Full workspace commands may trigger disk pressure at 33.6 GB
- If disk exhaustion occurs during build, individual crate/shard approach will be used
- No test failures will be attributed to disk issues
- CI will provide Linux/macOS baselines in Phase 1
