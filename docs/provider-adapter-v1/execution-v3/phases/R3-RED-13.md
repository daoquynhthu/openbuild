# R3-RED-13 Evidence

- Files changed:
  - `scripts/provider-v1/tests/test_scan_platform_exclusions.py` (extended)
  - `.gitignore` (un-ignored the test file)
- RED failures:
  1. `test_scan_detects_cfg_attr_windows_ignore` — `windows_ignore` regex expects `target_os = "windows"` but bare `windows` unmatched
  2. `test_scan_detects_not_target_os_windows` — `[^)]+` truncates nested `not(...)` closing paren, `not_windows` regex never matches
  3. `test_scan_detects_short_form_not_windows` — same nested paren truncation
- Tests already PASSing:
  - `test_scan_detects_bare_unix` — `cfg(unix)` captured without nested paren
  - `test_scan_detects_all_test_unix` — `all(test, unix)` ends with `unix` token
  - `test_wx_id_stability` — `wx_id()` deterministic
  - `test_wx_id_stable_across_nonstructural_insertions` — WX ID stable across blank-line/comment insertion
  - `test_legacy_ledger_migration` — old-format ledger write/parse works
  - `test_scan_output_format` — markdown table format correct
- Deviations: `.gitignore` line 32 commented out to track the test file
