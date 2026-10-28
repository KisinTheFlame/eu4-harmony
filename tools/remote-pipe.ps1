<#
.SYNOPSIS
Sends JSON requests to remote control's named pipe and prints the answers,
a JSON line each (tools/README.md).

.DESCRIPTION
Opens \\.\pipe\eu4-harmony, waiting up to -ConnectSeconds for a game with
the framework to open it, and sends each request as one line, reading its
answer before it sends the next. With -Together every request is written
before the first answer is read, as a client that does not wait would.

Exit codes: 0 every answer is {"ok": true, ...}; 1 an answer is an error;
2 the pipe could not be opened, or broke before an answer came (the game
ended, or returned to the main menu, which starts eu4.exe again).

.PARAMETER Requests
The requests, each a JSON object such as '{"capability": "session"}'.

.PARAMETER ConnectSeconds
How long to wait for the pipe (default 5).

.PARAMETER Together
Writes every request before reading the answers.

.PARAMETER Pipe
The pipe's name under \\.\pipe\ (default eu4-harmony).

.EXAMPLE
& .\tools\remote-pipe.ps1 '{"capability": "session"}' '{"capability": "army.list"}'
#>
param(
    [Parameter(Position = 0, ValueFromRemainingArguments)][string[]]$Requests,
    [double]$ConnectSeconds = 5,
    [switch]$Together,
    [string]$Pipe = 'eu4-harmony'
)
$ErrorActionPreference = 'Stop'
# A blank line is not answered, so it is not sent.
$Requests = @($Requests | Where-Object { $_ -match '\S' })
if (-not $Requests) { throw 'no request given' }

function Fail($message) {
    [Console]::Error.WriteLine($message)
    exit 2
}

$client = New-Object System.IO.Pipes.NamedPipeClientStream('.', $Pipe, [System.IO.Pipes.PipeDirection]::InOut)
try { $client.Connect([int]($ConnectSeconds * 1000)) }
catch { Fail "cannot open \\.\pipe\$Pipe within $ConnectSeconds s: no game with EU4 Harmony has it open ($($_.Exception.InnerException.Message))" }
$utf8 = New-Object System.Text.UTF8Encoding $false
$writer = New-Object System.IO.StreamWriter($client, $utf8)
$writer.NewLine = "`n"
$writer.AutoFlush = $true
$reader = New-Object System.IO.StreamReader($client, $utf8)
$broken = 'the pipe broke before the answer came: the game ended, or returned to the main menu, which starts it again'
$script:failed = $false

function Send($request) {
    try { $writer.WriteLine($request) } catch { Fail $broken }
}

function Receive {
    try { $line = $reader.ReadLine() } catch { $line = $null }
    if ($null -eq $line) { Fail $broken }
    Write-Output $line
    # The keys come in alphabetical order: {"ok":true,"result":...} or
    # {"error":{...},"ok":false}.
    if (-not $line.StartsWith('{"ok":true')) { $script:failed = $true }
}

if ($Together) {
    foreach ($request in $Requests) { Send $request }
    foreach ($request in $Requests) { Receive }
} else {
    foreach ($request in $Requests) { Send $request; Receive }
}
$client.Dispose()
if ($script:failed) { exit 1 }
exit 0
