#!/bin/sh
# Stop hook (registered in .claude/settings.json).
#
# Blocks the end of a turn whose last reply to the user is written in English
# and asks the agent to rewrite it in Japanese (AGENTS.md: language rules).
# Code blocks, inline code and URLs are ignored, so a Japanese reply that
# quotes commands passes. Notifications and tool output in English have made
# the agent answer in English before; this guard catches that.
#
# Needs jq; without it the hook does nothing. After one rewrite request
# (stop_hook_active) it lets the turn end, so it can never loop.

command -v jq >/dev/null 2>&1 || exit 0

input=$(cat)

[ "$(printf '%s' "$input" | jq -r '.stop_hook_active // false')" = "true" ] && exit 0

transcript=$(printf '%s' "$input" | jq -r '.transcript_path // empty')
[ -n "$transcript" ] && [ -r "$transcript" ] || exit 0

# The text of the last assistant entry that has any, read from the end of the
# transcript only (one JSON object per line; partial lines are skipped).
verdict=$(tail -n 400 "$transcript" | jq -Rrs '
  [split("\n")[] | fromjson? | select(.type == "assistant")
   | [.message.content[]? | select(.type == "text") | .text]
   | select(length > 0) | join("\n")] | last // ""
  | gsub("```[\\s\\S]*?```"; "") | gsub("`[^`]*`"; "") | gsub("https?://\\S+"; "")
  | if test("[\\p{Hiragana}\\p{Katakana}\\p{Han}]") then "ok"
    elif ([scan("[A-Za-z]")] | length) >= 20 then "english"
    else "ok" end' 2>/dev/null)

if [ "$verdict" = "english" ]; then
  printf '%s\n' '{"decision":"block","reason":"最後の応答が英語で書かれている。AGENTS.mdの言語ルールに従い、ユーザーへの応答は日本語で書く。同じ内容を日本語で書き直すこと。"}'
fi

exit 0
