#!/bin/sh
# Install personal global agent configuration:
#   global-instructions.md   -> ~/.claude/CLAUDE.md   (Claude Code)
#                            -> ~/.codex/AGENTS.md    (Codex)
#   stop-hook-git-check.sh   -> ~/.claude/stop-hook-git-check.sh
# An existing file with different content is backed up with a timestamp
# suffix before being overwritten.
#
# Manual steps that cannot be scripted:
#   - Copilot personal instructions: see copilot-personal-instructions.md
#   - Stop hook registration: merge claude-user-settings-snippet.json into
#     ~/.claude/settings.json (not automated, to avoid clobbering settings)

set -eu

here=$(cd "$(dirname "$0")" && pwd)
ts=$(date +%Y%m%d%H%M%S)

install_one() {
  src=$1
  dest=$2
  mkdir -p "$(dirname "$dest")"
  if [ -f "$dest" ] && ! cmp -s "$src" "$dest"; then
    cp "$dest" "$dest.bak.$ts"
    echo "Backed up existing $dest -> $dest.bak.$ts"
  fi
  cp "$src" "$dest"
  echo "Installed $dest"
}

install_one "$here/global-instructions.md" "$HOME/.claude/CLAUDE.md"
install_one "$here/global-instructions.md" "$HOME/.codex/AGENTS.md"
install_one "$here/stop-hook-git-check.sh" "$HOME/.claude/stop-hook-git-check.sh"
chmod +x "$HOME/.claude/stop-hook-git-check.sh"

echo "Done. Manual steps remaining:"
echo "  - Register the Stop hook: merge $here/claude-user-settings-snippet.json into ~/.claude/settings.json"
echo "  - Copilot personal instructions: see $here/copilot-personal-instructions.md"
