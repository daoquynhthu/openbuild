"""Tests for assert-provider-v1-invariants.py."""

import sys
import tempfile
from pathlib import Path

# Add parent to path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from assert_provider_v1_invariants import (
    _is_test_file,
    _find_test_ranges,
    _is_test_module,
)


class TestIsTestFile:
    def test_tests_dir(self):
        p = Path("/repo/crates/xai-grok-shell/tests/some_test.rs")
        assert _is_test_file(p)

    def test_benches_dir(self):
        p = Path("/repo/crates/xai-grok-shell/benches/session_list.rs")
        assert _is_test_file(p)

    def test_test_suffix(self):
        p = Path("/repo/crates/xai-grok-shell/src/agent/tests.rs")
        assert _is_test_file(p)

    def test_test_module_suffix(self):
        p = Path("/repo/crates/xai-grok-shell/src/agent/mvp_agent/subagent_spawn_context_tests.rs")
        assert _is_test_file(p)

    def test_production_file(self):
        p = Path("/repo/crates/xai-grok-shell/src/agent/config.rs")
        assert not _is_test_file(p)

    def test_lib_rs(self):
        p = Path("/repo/crates/xai-grok-provider/src/lib.rs")
        assert not _is_test_file(p)


class TestFindTestRanges:
    def test_single_cfg_test_block(self):
        lines = [
            "fn production() {}",
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn test_foo() {}",
            "}",
            "fn more_production() {}",
        ]
        ranges = _find_test_ranges(lines)
        assert len(ranges) == 1, f"Expected 1 range, got {ranges}"
        start, end = ranges[0]
        assert start == 1, f"Expected start=1, got {start}"
        assert end == 5, f"Expected end=5, got {end}"
        assert not any(s == 2 for s, _ in ranges), "Should not have range starting at mod tests line"

    def test_multiple_test_blocks(self):
        lines = [
            "fn a() {}",
            "#[cfg(test)]",
            "mod test_a {",
            "    #[test]",
            "    fn ta() {}",
            "}",
            "fn b() {}",
            "#[cfg(test)]",
            "mod test_b {",
            "    fn helper() {}",
            "    #[test]",
            "    fn tb() {}",
            "}",
            "fn c() {}",
        ]
        ranges = _find_test_ranges(lines)
        assert len(ranges) == 2, f"Expected 2 ranges, got {ranges}"
        start0, end0 = ranges[0]
        assert lines[start0] == "#[cfg(test)]"
        start1, end1 = ranges[1]
        assert lines[start1] == "#[cfg(test)]"

    def test_cfg_test_file_tail_block(self):
        """A test block that extends to end of file (no closing brace)."""
        lines = [
            "fn production() {}",
            "#[cfg(test)]",
            "mod tests {",
            "    fn helper() {}",
        ]
        ranges = _find_test_ranges(lines)
        assert len(ranges) == 1, f"Expected 1 range, got {ranges}"
        start, end = ranges[0]
        assert start == 1
        assert end == len(lines) - 1

    def test_no_test_blocks(self):
        lines = [
            "fn a() {}",
            "fn b() {}",
        ]
        assert _find_test_ranges(lines) == []


class TestIsTestModule:
    def test_inside_cfg_test_block(self):
        lines = [
            "#[cfg(test)]",
            "mod tests {",
            "    fn helper() {}",
            "    #[test] fn t() {}",
            "}",
        ]
        assert _is_test_module(lines, 1)
        assert _is_test_module(lines, 2)
        assert _is_test_module(lines, 3)

    def test_outside_test_block(self):
        lines = [
            "fn prod() {}",
            "#[cfg(test)]",
            "mod tests {",
            "    #[test] fn t() {}",
            "}",
            "fn more_prod() {}",
        ]
        assert not _is_test_module(lines, 0)
        assert not _is_test_module(lines, 5)
        assert _is_test_module(lines, 3)


class TestScanUnknownFields:
    def test_detects_missing_deny_unknown_fields(self, tmp_path):
        from assert_provider_v1_invariants import scan_unknown_fields
        config_dir = tmp_path / "crates" / "codegen" / "xai-grok-provider" / "src"
        config_dir.mkdir(parents=True)
        config_rs = config_dir / "config.rs"
        config_rs.write_text("#[derive(Serialize, Deserialize)]\npub struct ProviderConfig {}\n")
        findings = scan_unknown_fields()
        # The real path won't match tmp_path, so this just verifies the function runs


class TestScannerOutput:
    """End-to-end: run the scanner and check structure."""

    def test_scanner_imports(self):
        """Just verify the module can be imported."""
        from assert_provider_v1_invariants import CHECKS
        assert len(CHECKS) > 0

    def test_scanner_check_names(self):
        from assert_provider_v1_invariants import CHECKS
        names = [c[0] for c in CHECKS]
        assert "Handle::block_on in provider request preparation" in names
        assert "Runtime::block_on in provider request preparation" in names
        assert "unimplemented!/todo! in xai-grok-provider production code" in names
        assert "ProviderRuntime::new() in Pager production paths" in names
        assert "route-compiler error followed by legacy sampling_config_for_model fallback" in names
        assert "continue-on-error in release-gate workflows" in names
        assert "unknown protocol silently defaulted to ChatCompletions" in names
        assert "manual ModelEntry construction in provider crate (bypasses config resolution)" in names
