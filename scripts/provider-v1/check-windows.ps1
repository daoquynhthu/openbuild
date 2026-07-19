#!/usr/bin/env pwsh
# Provider V1 Windows gate script.
# Exits non-zero on first failure.

$ErrorActionPreference = "Stop"

Write-Host "=== Provider V1: Windows Gate ===" -ForegroundColor Cyan

Write-Host "`n--- cargo check ---" -ForegroundColor Yellow
cargo check --workspace --all-targets --locked
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "`n--- cargo clippy ---" -ForegroundColor Yellow
cargo clippy --workspace --all-targets --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { exit 1 }

Write-Host "`n=== All checks passed ===" -ForegroundColor Green
