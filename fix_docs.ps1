function Fix-File {
    param([string]$path)
    if (-not (Test-Path $path)) { Write-Host "MISSING: $path"; return $false }
    $content = Get-Content $path -Raw
    $original = $content

    # Fix unclosed HTML tags in doc comments (line by line)
    $tags = @('Mutex', 'str', 'R', 'scope', 'hex8', 'name', 'CR', 'ref', 'reason', 'repo', 'dest', 'ToolBridge')
    $lines = $content -split "`n"
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        if ($line -match '^\s*///' -or $line -match '^\s*//!') {
            foreach ($tag in $tags) {
                $line = $line -replace "(?<!``)<$tag>(?!``)", "``<$tag>``"
            }
            $lines[$i] = $line
        }
    }
    $content = $lines -join "`n"

    # Fix specific patterns
    $content = $content -replace '\*\*\[(context|delta|accumulated)\]\*\*', '**`$1`**'

    if ($content -ne $original) {
        Set-Content -Path $path -Value $content -NoNewline
        Write-Host "FIXED: $path"
        return $true
    }
    return $false
}

$files = @(
    "crates\codegen\xai-codebase-graph\src\index_manager.rs"
    "crates\codegen\xai-codebase-graph\src\scope_graph\graph.rs"
    "crates\codegen\xai-codebase-graph\src\types\mod.rs"
    "crates\codegen\xai-codebase-graph\src\bin\code_graph.rs"
    "crates\codegen\xai-grok-pager-render\src\render\renderable.rs"
    "crates\codegen\xai-grok-mermaid\src\mmdc.rs"
    "crates\codegen\xai-grok-agent\src\agent.rs"
    "crates\codegen\ptyctl-cli\src\cli.rs"
    "crates\codegen\xai-fast-worktree\src\bin\cli.rs"
    "crates\codegen\xai-grok-workspace\src\session\git.rs"
    "crates\codegen\xai-grok-workspace-types\src\rpc\hooks.rs"
    "crates\codegen\xai-hooks-plugins-types\src\lib.rs"
    "crates\codegen\xai-grok-shell\src\agent\feedback_client.rs"
)

$count = 0
foreach ($f in $files) {
    $full = Join-Path "D:\grok_build" $f
    if (Fix-File $full) { $count++ }
}
Write-Host "Fixed $count files"
