# Test that check.ps1 properly fails when clippy returns non-zero
# Uses a fake cargo that simulates clippy failure

$ErrorActionPreference = "Stop"
$tmpDir = Join-Path $env:TEMP "check-ps1-test-$(Get-Random)"
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

try {
    # Create a fake cargo that returns 0 for check, 1 for clippy
    $fakeCargoDir = Join-Path $tmpDir "fake-bin"
    New-Item -ItemType Directory -Force -Path $fakeCargoDir | Out-Null

    $fakeScript = @'
param([string[]]$Args)
if ($Args -contains "clippy") { exit 1 }
exit 0
'@
    $fakeCargo = Join-Path $fakeCargoDir "cargo.cmd"
    Set-Content -Path $fakeCargo -Value $fakeScript -Encoding utf8

    # Temporarily set PATH to find fake cargo first
    $oldPath = $env:PATH
    $env:PATH = "$fakeCargoDir;$oldPath"

    # Run check.ps1 — it should fail
    $checkScript = Join-Path $PSScriptRoot "..\..\check.ps1"
    & $checkScript -Targets @("test-crate") 2>&1
    $exitCode = $LASTEXITCODE

    $env:PATH = $oldPath

    if ($exitCode -eq 0) {
        Write-Error "FAIL: check.ps1 should have failed when clippy fails"
        exit 1
    }
    Write-Host "PASS: check.ps1 correctly fails on clippy error (exit $exitCode)"
    exit 0
}
finally {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}
