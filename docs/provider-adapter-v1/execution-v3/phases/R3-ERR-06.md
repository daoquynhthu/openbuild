# R3-ERR-06 Evidence

- Baseline commit: `7ad93ea`
- Files changed:
  - `scripts/provider-v1/assert_provider_v1_invariants.py`
  - `scripts/provider-v1/tests/test_assert_provider_v1_invariants.py`
- New checks added (both PASS with 0 violations):
  1. "unknown protocol silently defaulted to ChatCompletions" — regex `_\s*=>\s*(?:.*\s*::\s*)?ApiBackend::ChatCompletions` in xai-grok-provider
  2. "manual ModelEntry construction in provider crate (bypasses config resolution)" — regex `ModelEntry` in xai-grok-provider
- Pre-existing baseline violations unchanged (5)
- Scanner tests: `test_scanner_check_names` PASS
- Scanner run: 0 violations from new checks, 5 baseline PASS
