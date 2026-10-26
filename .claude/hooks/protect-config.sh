#!/bin/sh
# PreToolUse hook for Edit/Write (registered in .claude/settings.json).
#
# Asks for user confirmation before the agent modifies files that make up
# this repository's security guardrails (AGENTS.md: security section), so
# the guard also works in auto-accept permission modes.

input=$(cat)

if command -v jq >/dev/null 2>&1; then
  path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')
else
  path=$(printf '%s' "$input" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
fi

[ -n "$path" ] || exit 0

# Normalize relative paths (hooks run in the project directory) so the
# guard cannot be bypassed by passing a relative file_path.
case "$path" in
  /*) ;;
  *) path="${CLAUDE_PROJECT_DIR:-$PWD}/$path" ;;
esac

case "$path" in
  */.claude/settings.json|*/.claude/settings.local.json|*/.claude/hooks/*|*/.github/workflows/*|*/.github/dependabot.yml)
    printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"ask","permissionDecisionReason":"Modifying security-related configuration: %s. Requires user approval (AGENTS.md: security)."}}\n' "$path"
    ;;
esac

exit 0
