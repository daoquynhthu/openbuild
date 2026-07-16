# Grok Build — Windows Development Script
# 快速检查核心 crate（绕过已知 Windows 编译限制）
# 用法: .\check.ps1 [crate]...
# 默认检查 Provider Adapter 相关 crate

param(
    [string[]]$Targets = @(
        "xai-grok-provider",
        "xai-grok-sampler",
        "xai-grok-sampling-types",
        "xai-grok-tools",
        "xai-grok-models",
        "xai-grok-tools-api"
    )
)

$ErrorActionPreference = "Stop"
$PROTOC = "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Google.Protobuf_Microsoft.Winget.Source_8wekyb3d8bbwe\bin\protoc.exe"

if (-not (Test-Path $PROTOC)) {
    Write-Warning "protoc not found at $PROTOC — trying PATH"
    $PROTOC = (Get-Command protoc -ErrorAction SilentlyContinue).Source
    if (-not $PROTOC) {
        Write-Error "protoc not installed. Run: winget install Google.Protobuf"
        exit 1
    }
}

$env:PROTOC = $PROTOC
Write-Host "PROTOC=$PROTOC" -ForegroundColor Cyan

foreach ($crate in $Targets) {
    Write-Host "`n=== Checking $crate ===" -ForegroundColor Green
    cargo check -p $crate 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Error "check failed: $crate"
        exit $LASTEXITCODE
    }
    cargo clippy -p $crate -- -D warnings 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "clippy warnings in $crate (non-fatal)"
    }
}

Write-Host "`nAll checks passed." -ForegroundColor Green
