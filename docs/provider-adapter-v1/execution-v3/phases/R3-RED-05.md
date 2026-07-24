# R3-RED-05 Evidence

- Baseline commit: 6875625349a4a067dbb41d1249adae4143fc35d9
- Result commit: (to be filled after commit)
- Parent commit: 00107a8c5f8ce992f62a86b489617ccc1a4e58c0
- Files changed:
  - `crates/codegen/xai-grok-provider/tests/builtin_config_fidelity.rs` (new)
- Failing test before fix:
  - (documentation-style red tests — all 5 pass, confirming bug)
- Failure output summary:
  - `openai_extra_headers_silently_lost`: OPEN AI: `extra_headers` configured but absent from `route.static_headers`
  - `anthropic_extra_headers_silently_lost`: Anthropic: same
  - `xai_extra_headers_silently_lost`: xAI: same
  - `opencode_extra_headers_silently_lost`: OpenCode: same
  - `ollama_extra_headers_silently_lost`: Ollama: same
- Targeted check: `cargo check -p xai-grok-provider --test builtin_config_fidelity --locked` ✅
- Targeted test: `cargo test -p xai-grok-provider --test builtin_config_fidelity` ✅ (5 passed)
- `git diff --check HEAD^..HEAD`: ✅
- Deviations: none
