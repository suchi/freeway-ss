# Install personal global agent configuration:
#   global-instructions.md   -> ~\.claude\CLAUDE.md   (Claude Code)
#                            -> ~\.codex\AGENTS.md    (Codex)
#   stop-hook-git-check.sh   -> ~\.claude\stop-hook-git-check.sh
# An existing file with different content is backed up with a timestamp
# suffix before being overwritten.
#
# Manual steps that cannot be scripted:
#   - Copilot personal instructions: see copilot-personal-instructions.md
#   - Stop hook registration: merge claude-user-settings-snippet.json into
#     ~\.claude\settings.json (not automated, to avoid clobbering settings)

$ErrorActionPreference = "Stop"

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$ts = Get-Date -Format "yyyyMMddHHmmss"

function Install-One([string]$src, [string]$dest) {
    $dir = Split-Path -Parent $dest
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }
    if ((Test-Path $dest) -and ((Get-FileHash $dest).Hash -ne (Get-FileHash $src).Hash)) {
        Copy-Item $dest "$dest.bak.$ts"
        Write-Host "Backed up existing $dest -> $dest.bak.$ts"
    }
    Copy-Item $src $dest -Force
    Write-Host "Installed $dest"
}

Install-One (Join-Path $here "global-instructions.md") (Join-Path $HOME ".claude\CLAUDE.md")
Install-One (Join-Path $here "global-instructions.md") (Join-Path $HOME ".codex\AGENTS.md")
Install-One (Join-Path $here "stop-hook-git-check.sh") (Join-Path $HOME ".claude\stop-hook-git-check.sh")

Write-Host "Done. Manual steps remaining:"
Write-Host "  - Register the Stop hook: merge $here\claude-user-settings-snippet.json into ~\.claude\settings.json"
Write-Host "  - Copilot personal instructions: see $here\copilot-personal-instructions.md"
