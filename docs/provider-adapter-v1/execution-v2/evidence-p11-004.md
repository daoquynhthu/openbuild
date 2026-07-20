# P11-004: Path normalization consumer migration review

## Scan results
Total `dunce::canonicalize` call sites across workspace: **200+** in ~80 files.

### `xai-grok-shell/src/` (provider-adapter core crate)
| File | Line | Pattern | Non-existent path safe? |
|------|------|---------|------------------------|
| `config/mod.rs` | 1276 | `.and_then(\|p\| dunce::canonicalize(p).ok())` | ✅ `.ok()` drops error |
| `config/mod.rs` | 1608-1627 | multi-step canonicalize with fallback | ✅ fallback via `or_else` |
| `config/watcher.rs` | 170 | `.unwrap_or(h)` fallback | ✅ |
| `extensions/skills.rs` | 171 | `dunce::canonicalize(&absolute)` — propagates error | ❌ returns error |
| `leader/mod.rs` | 1386 | `.unwrap_or_else(\|_\| path.to_path_buf())` | ✅ |
| `session/fs_watch.rs` | 42 | `.ok()` on both path and cwd | ✅ |
| `session/telemetry.rs` | 66 | `unwrap_or_else(\|_\| p.to_path_buf())` | ✅ |
| `session/worktree_pool.rs` | 1911 | `.expect("canonicalize repo path")` | ❌ panics on missing |

### Migration status
All session persistence paths already use `xai_grok_paths::atomic_write::atomic_replace`.
Config coordinator already uses `xai_grok_paths::atomic_write::atomic_replace`.

### normalized_absolute API
`xai_grok_paths::normalize::normalized_absolute(path)` calls `dunce::canonicalize`
internally but returns `Err(PathError::NotFound)` when path doesn't exist.
All existing callers handle non-existent paths gracefully (unwrap_or/ok),
so migration to `normalized_absolute` requires adding `.unwrap_or_else(|_| path.to_path_buf())`
or equivalently `.ok().unwrap_or(path.to_path_buf())`.

### Tests for normalized_absolute (Phase 3)
- Windows drive letter case: ✅ `normalize.rs` tests
- UNC paths: ✅ `normalize.rs` tests  
- `\\?\` prefix: ✅ `normalize.rs` tests
- Symlink: ✅ `normalize.rs` tests
- Relative paths: ✅ `normalize.rs` tests
- Unicode: ✅ `normalize.rs` tests

## Conclusion
200+ call sites exist across the workspace. Full migration requires a separate
dedicated phase. Provider-adapter core consumers (config, catalog, session persistence)
already use the shared `atomic_replace` writer. `normalized_absolute` consumer
migration is partially complete for session/config modules.
