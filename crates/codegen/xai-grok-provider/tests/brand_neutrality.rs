//! Brand neutrality scan: ensure generic resolver/catalog/sampler/TUI code
//! does NOT contain brand-specific branches for `xai`.
//!
//! Allowed xAI-specific files: `providers/xai.rs`, `auth.rs`.
//! Any `ProviderId::XAI` comparison outside these files is a violation.

use std::path::Path;

const ALLOWED_XAI_FILES: &[&str] = &["providers/xai.rs", "auth.rs", "types.rs"];

#[test]
fn generic_code_must_not_branch_on_xai() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = crate_dir.join("src");

    let mut violations = Vec::new();

    for entry in walkdir::WalkDir::new(&src_dir) {
        let entry = entry.expect("walk error");
        if !entry.file_type().is_file()
            || !entry.path().extension().is_some_and(|e| e == "rs")
        {
            continue;
        }

        let rel_path = entry
            .path()
            .strip_prefix(&src_dir)
            .unwrap()
            .to_string_lossy()
            .to_string()
            .replace('\\', "/");

        if ALLOWED_XAI_FILES.iter().any(|a| rel_path.contains(a)) {
            continue;
        }
        if rel_path.contains("/tests/") || rel_path.ends_with("_tests.rs") {
            continue;
        }

        let content = std::fs::read_to_string(entry.path()).expect("read file");
        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("#[cfg(test)]") {
                continue;
            }
            // Only flag direct equality checks against ProviderId::XAI constant
            if trimmed.contains("ProviderId::XAI") && !trimmed.contains("const") {
                violations.push(format!("  {}:{}: {}", rel_path, line_no + 1, trimmed));
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "Brand neutrality violation: found {} xAI-specific branch(es) in generic code:\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

