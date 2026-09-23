# claudeTalk - say-server.ps1
# Minimal MCP server (stdio, JSON-RPC, one message per line) exposing a single
# tool, `say`, so Claude can talk while it works or say something different
# from what it writes. The call shows up folded in Claude Code (Ctrl+O shows
# the text). It returns immediately: the phrase goes to the speech queue that
# speak.ps1 -Worker plays in order.
#
# If talk mode is off in this session, `say` stays silent and tells Claude so.
# Claude Code starts one server per session, so the session (and its voice) is
# looked up from the claude.exe this server runs under.

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")

$utf8 = New-Object Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$stdin = New-Object IO.StreamReader([Console]::OpenStandardInput(), $utf8)
$stdout = New-Object IO.StreamWriter([Console]::OpenStandardOutput(), $utf8)
$stdout.AutoFlush = $true

$projectDir = $env:CLAUDE_PROJECT_DIR
if (-not $projectDir) { $projectDir = (Get-Location).Path }

$sayTool = @{
    name        = "say"
    description = "Speak a short phrase out loud to the user (claudeTalk talk mode). Plays in the background and returns at once; several calls play in order. Use only while talk mode is on, with 1-2 natural spoken sentences: no code, paths, symbols or markdown."
    inputSchema = @{
        type       = "object"
        properties = @{ text = @{ type = "string"; description = "What to say, in the user's language." } }
        required   = @("text")
    }
}

function Send($obj) {
    $stdout.WriteLine(($obj | ConvertTo-Json -Depth 10 -Compress))
}

function Send-Result($id, $result) { Send @{ jsonrpc = "2.0"; id = $id; result = $result } }

function Send-Error($id, $code, $message) {
    Send @{ jsonrpc = "2.0"; id = $id; error = @{ code = $code; message = $message } }
}

function Invoke-Say($text) {
    $state = Get-TalkState $projectDir (Get-TalkSessionId)
    if (-not $state.enabled) {
        return "talk mode is off: nothing was spoken. Don't call say until the user turns talk mode on."
    }
    Add-Speech $text $state
    return "spoken"
}

while ($null -ne ($line = $stdin.ReadLine())) {
    if (-not $line.Trim()) { continue }
    try { $msg = $line | ConvertFrom-Json } catch { continue }
    $hasId = $msg.PSObject.Properties.Name -contains "id"
    try {
        switch ($msg.method) {
            "initialize" {
                $version = "2024-11-05"
                if ($msg.params -and $msg.params.protocolVersion) { $version = $msg.params.protocolVersion }
                Send-Result $msg.id @{
                    protocolVersion = $version
                    capabilities    = @{ tools = @{} }
                    serverInfo      = @{ name = "claudeTalk-voice"; version = "0.5.0" }
                }
            }
            "ping" { Send-Result $msg.id @{} }
            "tools/list" { Send-Result $msg.id @{ tools = @($sayTool) } }
            "tools/call" {
                if ($msg.params.name -ne "say") { Send-Error $msg.id -32602 "unknown tool: $($msg.params.name)"; break }
                $text = [string]$msg.params.arguments.text
                $reply = Invoke-Say $text
                Send-Result $msg.id @{ content = @(@{ type = "text"; text = $reply }) }
            }
            default {
                # Notifications (no id) need no answer.
                if ($hasId) { Send-Error $msg.id -32601 "method not found: $($msg.method)" }
            }
        }
    } catch {
        Write-TalkLog "say-server error: $_"
        if ($hasId) { Send-Error $msg.id -32603 "$_" }
    }
}
