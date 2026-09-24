# claudeTalk - talk-context.ps1
# 'UserPromptSubmit' hook. While talk mode is on, it reminds Claude, every
# turn, how to answer for someone who is listening. Off: no output, no tokens.
# A new prompt does NOT cut what Claude is saying: answers queue up, so the
# user can send the next message while still listening to the last one.

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")

try {
    $raw = Read-HookInput
    $payload = if ($raw) { $raw | ConvertFrom-Json } else { $null }
    $cwd = if ($payload -and $payload.cwd) { $payload.cwd } else { (Get-Location).Path }

    $sid = if ($payload) { Get-TalkSessionId $payload.session_id } else { Get-TalkSessionId }
    # The `say` server and /talk find their session through live.json. If
    # SessionStart didn't record it (plugin updated mid-session), do it now:
    # this prompt may be the /talk that needs it.
    if ($sid -and -not $env:CLAUDETALK_SESSION_ID) {
        $live = Read-JsonFile $script:TalkLiveFile
        $known = $live -and (@($live.PSObject.Properties | ForEach-Object { $_.Value }) -contains $sid)
        if (-not $known) { Register-TalkSession $sid }
    }

    $state = Get-TalkState $cwd $sid
    if (-not $state.enabled) { exit 0 }
    # Talk mode may have been left on from an earlier run: re-arm "Oye Claude".
    Set-TalkFlag $true

    $spoken = Test-IsSpoken $payload.prompt
    if ($state.onlySpoken -and -not $spoken) {
        $rules = @"
claudeTalk talk mode is ON, but this message was typed, not spoken, and the user asked for spoken answers only to spoken messages. Answer this one in text only: do not call the claudeTalk say tool. Nothing will be read aloud. Keep writing in the language of the user's messages.
"@
        @{ hookSpecificOutput = @{ hookEventName = "UserPromptSubmit"; additionalContext = $rules } } |
            ConvertTo-Json -Depth 5 -Compress
        exit 0
    }

    $rules = @"
claudeTalk talk mode is ON: the user hears you through text-to-speech. Answer for a listener:
- LANGUAGE: write AND speak in the language of the user's messages (usually Spanish). These rules are in English only for you; that is not a reason to switch. Never change language unless the user asks for it.
- If the answer fits in 2-3 plain sentences, just write it, conversational, no markdown. It is read aloud automatically. Do not call say.
- If the answer needs code, tables, lists or more than ~3 sentences: first call the claudeTalk say tool with 1-2 sentences that give the gist or point to the screen (e.g. "Te deje en pantalla los tres pasos"), then write the full detail. Never say the same thing you write.
- For work with tools: call say briefly when you start ("Voy a revisar el hook") and, if the result is long, again before the final write-up.
- Never put code, paths, symbols or markdown in say.
- If the user asks to stop talking or to turn talk mode off/on, or to change any claudeTalk setting (voice, speed, volume, silence, sensitivity, hotkey, Enter, wake word, speak only to spoken messages, screen share, glass, position, language), use the claudeTalk talk skill.
"@
    if ($spoken) {
        $rules += "- This message starts with a microphone mark: the user SPOKE it and Whisper transcribed it. Read it charitably: expect misheard words (e.g. Cloud for Claude), and stray phrases at the end that are really your own voice picked up by the mic; ignore those. Ask only if the meaning is truly unclear.`n"
    }

    @{ hookSpecificOutput = @{ hookEventName = "UserPromptSubmit"; additionalContext = $rules } } |
        ConvertTo-Json -Depth 5 -Compress
} catch {
    Write-TalkLog "talk-context error: $_"
}
exit 0
