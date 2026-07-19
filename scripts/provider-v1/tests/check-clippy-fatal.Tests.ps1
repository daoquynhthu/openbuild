# P3-002: check.ps1 must exit non-zero when clippy fails.

$tmpDir = Join-Path $env:TEMP "check-fake-cargo-$(Get-Random)"
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
$fakeCargo = @"
@echo off
if /i "%1"=="clippy" exit /b 1
exit /b 0
"@
Set-Content -Path (Join-Path $tmpDir "cargo.cmd") -Value $fakeCargo -Encoding utf8
$originalPath = $env:PATH
$checkScript = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\check.ps1")

Describe "check.ps1 clippy failure" {
    It "should exit non-zero when clippy fails" {
        $env:PATH = "$tmpDir;$originalPath"
        try {
            & $checkScript -Targets @("fake-crate") 2>&1 | Out-Null
            $global:LASTEXITCODE = 0
        } catch {
            $global:LASTEXITCODE = 1
        }
        $LASTEXITCODE | Should Not Be 0
    }

    It "should not print 'All checks passed' on failure" {
        $env:PATH = "$tmpDir;$originalPath"
        $output = ""
        try {
            $output = & $checkScript -Targets @("fake-crate") 2>&1 | Out-String
            $global:LASTEXITCODE = 0
        } catch {
            $output = $_.Exception.Message
            $global:LASTEXITCODE = 1
        }
        $output | Should Not Match "All checks passed"
    }
}

$env:PATH = $originalPath
Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
