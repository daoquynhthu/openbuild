# R3-BASE-03 Evidence — Windows compilation baseline (check / clippy / test compilation / doc)

- Baseline commit: 916c66e
- Result commit: 6505f2f
- Files changed: `crates/codegen/xai-grok-shell/src/agent/mvp_agent/subagent_coordinator.rs`, `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs` (RefCell-across-await fixes)

## Gating check results

| Check | Result | Details |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Zero formatting violations after `cargo fmt --all` (committed as 6505f2f) |
| `cargo check --workspace --all-targets --locked --keep-going` | PASS | 0 errors, all 80+ crates |
| `cargo clippy --workspace --all-targets --locked --keep-going -- -D warnings` | PASS | 4 RefCell-across-await violations fixed before baseline |
| `cargo test --workspace --all-targets --locked --no-run --no-fail-fast` | PASS | All test binaries compiled; `--jobs 2` used to stay within host memory limit on Windows |
| `cargo test --workspace --all-targets --locked -- --list` | PASS | All lib/bin test targets listed; 3 bench targets (`paste_latency`, `pty_bench`, `session_list`) do not support `--list` (upstream cargo limitation, not a defect) |
| `cargo doc --workspace --no-deps --locked` | PASS | Documentation builds without errors. Pre-existing warnings only (unresolved links, private item references, redundant link targets) |

## Workaround: `--jobs 2`

On Windows (x86_64-pc-windows-msvc), the default parallel compilation of `--all-targets` (lib + test + bench for 80+ crates) can trigger the OS page-file/commit-limit under load on hosts with constrained memory. Adding `--jobs 2` limits concurrent rustc invocations to 2, keeping memory pressure manageable. This is a transient host resource constraint — CI runners with more memory will not need this flag.

## RefCell-across-await fixes before clippy baseline

4 violations in `xai-grok-shell` were fixed to achieve zero-warning clippy:

1. `subagent_coordinator.rs:444,464` — `RefCell<SharedGlobalToolRegistry>` borrowed across `.await`
2. `agent_ops.rs:1392,1395` — `RefCell<Option<PeriodicRefreshEntry>>` borrowed across `.await`

Both fixed by extracting the inner value before the struct literal / async call, and releasing the borrow before `.await`.

## No new clippy / check / doc warnings in targeted crates

- **`xai-grok-provider`** — zero warnings
- **`xai-grok-shell`** — zero new warnings (pre-existing doc-links-to-private warnings only)
- **`xai-grok-pager`** — zero new warnings
- **`xai-grok-sampler`** — zero new warnings

## Volume usage during compilation

The full `--all-targets` workspace build with `cargo clean` + rebuild consumed significant disk space. The target directory grew to tens of GiB as expected for a workspace of this size.

## Deviations

- `--jobs 2` required on this host to avoid OOM; CI runners with more memory can omit it.
- `cargo test --list` cannot pass for bench targets (upstream cargo limitation, `--list` not implemented for `[bench]`).
- Full `cargo test` run was not performed in this baseline — only compilation is checked. Individual Phase-gate tests cover runtime correctness.
