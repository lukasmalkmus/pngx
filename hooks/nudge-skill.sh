#!/usr/bin/env bash
# Nudge the agent to use the "paperless" skill when running pngx commands
# directly, instead of invoking them without the skill's guidance.
#
# Uses PostToolUse additionalContext to inject a nudge after the command runs.

input=$(cat)

# Check if the bash command involves pngx.
command=$(echo "$input" | jq -r '.tool_input.command // empty' 2>/dev/null)
if [[ -z "$command" ]] || ! echo "$command" | grep -q 'pngx'; then
  exit 0
fi

# Only nudge once per session to avoid spamming context.
marker="${TMPDIR:-/tmp}/.pngx-skill-nudge-${PPID}"
[ -f "$marker" ] && exit 0
touch "$marker"

nudge='<system-reminder>The "paperless" skill provides guided pngx workflows. Invoke it with /paperless or the Skill tool.</system-reminder>'

jq -n --arg nudge "$nudge" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    additionalContext: $nudge
  }
}'
