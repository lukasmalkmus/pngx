#!/usr/bin/env bash
# Nudge the agent to use the "paperless" skill when running pngx commands
# directly, instead of invoking them without the skill's guidance.

set -euo pipefail

input=$(cat)

# Check if the bash command involves pngx.
echo "$input" | jq -e '.tool_input.command' 2>/dev/null | grep -q 'pngx' || exit 0

# Only nudge once per session to avoid spamming context.
marker="${TMPDIR:-/tmp}/.pngx-skill-nudge-${PPID}"
[ -f "$marker" ] && exit 0
touch "$marker"

jq -n '{"additionalContext": "Tip: Use the \"paperless\" skill (/paperless) for guided pngx workflows."}'
