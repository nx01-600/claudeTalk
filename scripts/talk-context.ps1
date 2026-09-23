# claudeTalk - talk-context.ps1
# 'UserPromptSubmit' hook. A new prompt cuts whatever Claude was saying, like
# in a real conversation. While talk mode is on, it also reminds Claude, every
# turn, how to answer for someone who is listening. Off: no output, no tokens.

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")

try {
    $raw = Read-HookInput
    $payload = if ($raw) { $raw | ConvertFrom-Json } else { $null }
    $cwd = if ($payload -and $payload.cwd) { $payload.cwd } else { (Get-Location).Path }

    Stop-Speech

    $state = Get-TalkState $cwd
    if (-not $state -or -not $state.enabled) { exit 0 }
    # Talk mode may have been left on from an earlier session: re-arm "Oye Claude".
    Set-TalkFlag $true

    $rules = @"
claudeTalk talk mode is ON: the user hears you through text-to-speech. Answer for a listener:
- LANGUAGE: write AND speak in the language of the user's messages (usually Spanish). These rules are in English only for you; that is not a reason to switch. Never change language unless the user asks for it.
- If the answer fits in 2-3 plain sentences, just write it, conversational, no markdown. It is read aloud automatically. Do not call say.
- If the answer needs code, tables, lists or more than ~3 sentences: first call the claudeTalk say tool with 1-2 sentences that give the gist or point to the screen (e.g. "Te deje en pantalla los tres pasos"), then write the full detail. Never say the same thing you write.
- For work with tools: call say briefly when you start ("Voy a revisar el hook") and, if the result is long, again before the final write-up.
- Never put code, paths, symbols or markdown in say.
- If the user asks to stop talking or to turn talk mode off/on, use the claudeTalk talk skill.
"@

    @{ hookSpecificOutput = @{ hookEventName = "UserPromptSubmit"; additionalContext = $rules } } |
        ConvertTo-Json -Depth 5 -Compress
} catch {
    Write-TalkLog "talk-context error: $_"
}
exit 0
