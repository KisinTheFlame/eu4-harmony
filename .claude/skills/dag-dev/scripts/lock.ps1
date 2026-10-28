<#
.SYNOPSIS
Takes or releases a named lock shared by every worktree of this repository.

A lock is a file, dag-locks\<name>.lock under the repository's common git
directory, created only when absent and holding its holder and when it was
taken (UTC). Taking a lock one already holds succeeds. A lock older than
-StaleHours is taken over, with a warning.

Exit codes: 0 done; 2 the lock is held by someone else (without -Wait, or when
-Wait gave up after -TimeoutMinutes); 1 an error.

.EXAMPLE
& .\.claude\skills\dag-dev\scripts\lock.ps1 -Acquire game -Holder C2 -Wait
& .\.claude\skills\dag-dev\scripts\lock.ps1 -Release game -Holder C2
& .\.claude\skills\dag-dev\scripts\lock.ps1 -ReleaseHolder C2
& .\.claude\skills\dag-dev\scripts\lock.ps1
#>
[CmdletBinding(DefaultParameterSetName = 'Status')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Acquire')][string]$Acquire,
    [Parameter(Mandatory, ParameterSetName = 'Release')][string]$Release,
    [Parameter(Mandatory, ParameterSetName = 'ReleaseHolder')][string]$ReleaseHolder,
    [Parameter(Mandatory, ParameterSetName = 'Acquire')]
    [Parameter(Mandatory, ParameterSetName = 'Release')]
    [string]$Holder,
    [Parameter(ParameterSetName = 'Acquire')][switch]$Wait,
    [Parameter(ParameterSetName = 'Acquire')][double]$TimeoutMinutes = 9,
    [Parameter(ParameterSetName = 'Acquire')][double]$StaleHours = 3,
    [Parameter(ParameterSetName = 'Acquire')][int]$PollSeconds = 15
)
$ErrorActionPreference = 'Stop'

$common = git -C $PSScriptRoot rev-parse --path-format=absolute --git-common-dir
if ($LASTEXITCODE -ne 0) { [Console]::Error.WriteLine('not inside a git repository'); exit 1 }
$dir = Join-Path $common 'dag-locks'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

function LockPath($name) {
    if ($name -notmatch '^[A-Za-z0-9._-]+$') { throw "bad lock name: $name" }
    Join-Path $dir "$name.lock"
}

# The holder and the time it was taken; $null when the lock is free. A lock
# being written right now reads as held by "?" since now.
function ReadLock($path) {
    try { $lines = [IO.File]::ReadAllLines($path) }
    catch [IO.FileNotFoundException] { return $null }
    catch [IO.IOException] { $lines = @() }
    $since = [datetime]::UtcNow
    if ($lines.Count -ge 2) {
        try { $since = [datetime]::Parse($lines[1], $null, 'RoundtripKind').ToUniversalTime() } catch {}
    }
    [pscustomobject]@{ Holder = $(if ($lines.Count -ge 1 -and $lines[0]) { $lines[0] } else { '?' }); Since = $since }
}

# Creates the lock file only if it does not exist yet, which is atomic.
function TryTake($path) {
    try { $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read) }
    catch [IO.IOException] { return $false }
    try {
        $writer = New-Object IO.StreamWriter($stream)
        $writer.WriteLine($Holder)
        $writer.WriteLine([datetime]::UtcNow.ToString('o'))
        $writer.Flush()
    } finally { $stream.Dispose() }
    $true
}

function Describe($lock) { "$($lock.Holder) since $($lock.Since.ToString('u'))" }

switch ($PSCmdlet.ParameterSetName) {
    'Acquire' {
        $path = LockPath $Acquire
        $deadline = [datetime]::UtcNow.AddMinutes($TimeoutMinutes)
        while ($true) {
            if (TryTake $path) { Write-Output "acquired $Acquire for $Holder"; exit 0 }
            $lock = ReadLock $path
            if (-not $lock) { continue }
            if ($lock.Holder -eq $Holder) { Write-Output "$Holder already holds $Acquire ($(Describe $lock))"; exit 0 }
            if ($lock.Holder -ne '?' -and $lock.Since -lt [datetime]::UtcNow.AddHours(-$StaleHours)) {
                Write-Warning "taking over the stale lock $Acquire from $(Describe $lock)"
                Remove-Item -LiteralPath $path -ErrorAction SilentlyContinue
                continue
            }
            if (-not $Wait -or [datetime]::UtcNow -ge $deadline) {
                Write-Output "$Acquire is held by $(Describe $lock); try again later"
                exit 2
            }
            Start-Sleep -Seconds $PollSeconds
        }
    }
    'Release' {
        $path = LockPath $Release
        $lock = ReadLock $path
        if (-not $lock) { Write-Output "$Release is not held"; exit 0 }
        if ($lock.Holder -ne $Holder) { [Console]::Error.WriteLine("$Release is held by $(Describe $lock), not $Holder"); exit 1 }
        Remove-Item -LiteralPath $path
        Write-Output "released $Release"
        exit 0
    }
    'ReleaseHolder' {
        foreach ($file in Get-ChildItem -LiteralPath $dir -Filter '*.lock') {
            $lock = ReadLock $file.FullName
            if ($lock -and $lock.Holder -eq $ReleaseHolder) {
                Remove-Item -LiteralPath $file.FullName
                Write-Output "released $($file.BaseName) held by $ReleaseHolder"
            }
        }
        exit 0
    }
    'Status' {
        $files = @(Get-ChildItem -LiteralPath $dir -Filter '*.lock')
        if (-not $files) { Write-Output 'no locks held' }
        foreach ($file in $files) {
            $lock = ReadLock $file.FullName
            if ($lock) { Write-Output "$($file.BaseName): $(Describe $lock)" }
        }
        exit 0
    }
}
