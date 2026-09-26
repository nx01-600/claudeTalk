# Builds the parity fixtures for the Rust port from the v0.5 PowerShell code:
# cuts real Claude Code transcripts at random points, runs Read-Turn and
# ConvertTo-Speech on each cut and saves input + expected output under
# native/tests/golden-local/ (git-ignored: transcripts are private).
# Needs the v0.5 scripts (git show v0.5.4:scripts/speak.ps1 works too).
#
#   powershell -File native\tests\gen-golden.ps1 [-Speak <speak.ps1>] [-Cases 80]

param(
    [string]$Speak = (Join-Path $PSScriptRoot "..\..\scripts\speak.ps1"),
    [int]$Cases = 80
)

$ErrorActionPreference = "Stop"
$src = Get-Content -Raw -Encoding UTF8 $Speak
$common = Join-Path (Split-Path $Speak) "talk-common.ps1"
. $common
# Only the functions of speak.ps1, not its worker or hook body.
$start = $src.IndexOf('function ConvertTo-Speech')
$end = [regex]::Match($src, "(?m)^try {").Index
Invoke-Expression $src.Substring($start, $end - $start)

$out = Join-Path $PSScriptRoot "golden-local"
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $out | Out-Null

$all = Get-ChildItem (Join-Path $env:USERPROFILE ".claude\projects") -Recurse -Filter *.jsonl |
    Where-Object { $_.Length -gt 20KB -and $_.Length -lt 3MB }
# Half of them from sessions that used talk mode, so `say` turns are covered.
$talk = @($all | Where-Object { Select-String -Path $_.FullName -SimpleMatch "claudeTalk_voice__say" -Quiet } | Get-Random -Count 20)
$files = @($talk) + @($all | Where-Object { $talk -notcontains $_ } | Get-Random -Count (40 - $talk.Count))
$rand = New-Object Random 7
$n = 0
foreach ($f in $files) {
    $lines = Get-Content -Encoding UTF8 $f.FullName
    for ($k = 0; $k -lt 2 -and $n -lt $Cases; $k++) {
        $cut = $rand.Next([math]::Min(5, $lines.Count), $lines.Count + 1)
        $case = Join-Path $out ("{0:D3}.jsonl" -f $n)
        [IO.File]::WriteAllLines($case, [string[]]$lines[0..($cut - 1)], (New-Object Text.UTF8Encoding($false)))
        $turn = Read-Turn $case
        $expected = [ordered]@{
            spoke = [bool]$turn.spoke; work_after_say = [bool]$turn.workAfterSay
            text_after = [string]$turn.textAfter; spoken = [bool]$turn.spoken
            speech = (ConvertTo-Speech $turn.textAfter $true)
            speech_code = (ConvertTo-Speech $turn.textAfter $false)
        }
        [IO.File]::WriteAllText(($case -replace '\.jsonl$', '.expected.json'),
            ($expected | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding($false)))
        $n++
    }
}
"$n cases in $out"
