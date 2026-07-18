# Rollback Guide — Provider Adapter V1

## Quick rollback to legacy xAI-only mode

If you encounter issues with the multi-provider system, revert to
the legacy xAI-only configuration:

### Step 1: Remove multi-provider config

Edit `~/.grok/config.toml` and remove all `[provider.*]` sections
except `[provider.xai]`:

```toml
# Keep only:
[provider.xai]
api_key = "xai-..."       # or env_key

# Remove all others:
# [provider.openai]
# [provider.anthropic]
# ...
```

### Step 2: Remove CLI provider flags

Remove any `--provider`, `--api-key`, `--base-url` flags from your
workflows and scripts. The default provider will be xAI.

### Step 3: Verify

```bash
grok providers
# Should show only xAI
grok models
# Should list xAI models (grok-3, grok-build, etc.)
```

## Rollback via git (development environments)

If you are on the `feat/provider-adapter` branch and need to revert
the entire change:

```bash
# Find the commit before Phase 10 changes
git log --oneline

# Revert specific commits
git revert <commit-hash>

# Or reset to a known-good baseline
git checkout <baseline-commit> -- crates/codegen/xai-grok-provider/
git checkout <baseline-commit> -- crates/codegen/xai-grok-pager/
```

## Configuration file recovery

If `config.toml` becomes corrupted, the application will refuse to
overwrite it and report an error. To recover:

```bash
# Backup the corrupted file
cp ~/.grok/config.toml ~/.grok/config.toml.bak

# Restore from backup or create a minimal config
cat > ~/.grok/config.toml << 'EOF'
[provider.xai]
env_key = ["XAI_API_KEY"]
EOF
```

## Known incompatible changes

- `[model.*]` entries with explicit provider/route references require
  the provider to be configured in `[provider.*]`
- The `--model` flag now supports `provider/model` syntax; bare model
  names resolve to the default provider (usually xAI)
- `[endpoints]` legacy section is still supported but emits a
  deprecation notice
