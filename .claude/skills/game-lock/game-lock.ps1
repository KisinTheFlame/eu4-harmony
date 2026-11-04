<#
.SYNOPSIS
Claims the game for one session at a time: the game folder, eu4.exe, the
player's files and the game window.

The claim is one record on this machine, %LOCALAPPDATA%\eu4-harmony-dev\
game-lock.json, shared by every checkout. A named mutex guards each read and
write of it.

-Claim takes the game for -Owner (by default the leaf name of the checkout's
folder) and backs up the player's files with eu4-harmony backup, unless
-NoBackup (for a player who wants their own play kept). If the game is held, it
says by whom and exits with 2; -Wait polls until the game is free instead. A
claim holder that has not renewed for its lease (-Lease minutes, 30 by
default) while eu4.exe is not running has gone: the next claim takes over,
first restoring the backup that holder left.

Every -Claim and -Renew by the holder starts the lease again, and so does
every look at the claim, by -Watch or a waiting -Claim, while eu4.exe runs:
the lease runs out no sooner than its length after the game closes. -Watch,
run in the background by the holder, exits with 3 once the lease is down to
-Warn minutes (5 by default), or with 0 once the holder no longer holds the
game. -Release puts
the player's files back from the backup (-KeepCrashes keeps the run's crash
folders) and frees the game. With no switch, prints who holds the game.
#>
[CmdletBinding(DefaultParameterSetName = 'Status')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Claim')][switch]$Claim,
    [Parameter(ParameterSetName = 'Claim')][string]$Purpose = '',
    [Parameter(ParameterSetName = 'Claim')][switch]$Wait,
    [Parameter(ParameterSetName = 'Claim')][switch]$NoBackup,
    [Parameter(ParameterSetName = 'Claim')][ValidateRange(1, 1440)][int]$Lease = 30,
    [Parameter(Mandatory, ParameterSetName = 'Renew')][switch]$Renew,
    [Parameter(Mandatory, ParameterSetName = 'Watch')][switch]$Watch,
    [Parameter(ParameterSetName = 'Watch')][ValidateRange(1, 60)][int]$Warn = 5,
    [Parameter(Mandatory, ParameterSetName = 'Release')][switch]$Release,
    [Parameter(ParameterSetName = 'Release')][switch]$KeepCrashes,
    [string]$Owner
)
$ErrorActionPreference = 'Stop'

$Base = Join-Path $env:LOCALAPPDATA 'eu4-harmony-dev'
$LockFile = Join-Path $Base 'game-lock.json'
$Backups = Join-Path $Base 'backups'
# The checkout this skill is in, three folders up from .claude\skills\game-lock.
$Checkout = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
New-Item -ItemType Directory -Force $Base | Out-Null

# Backs up the player's files into $dir, or restores them from it, with this
# checkout's command line tool (eu4-harmony backup and restore), built by
# cargo as needed. Stops on a failure, a restore's NOT RESTORED included.
function PlayerState([ValidateSet('backup', 'restore')][string]$what, [string]$dir, [switch]$keepCrashes) {
    $words = @($what, $dir)
    if ($keepCrashes) { $words += '--keep-crashes' }
    & cargo run --release --quiet --manifest-path (Join-Path $Checkout 'Cargo.toml') -p eu4-harmony-cli -- @words
    if ($LASTEXITCODE -ne 0) { throw "eu4-harmony $what $dir failed" }
}

if (-not $Owner -and $PSCmdlet.ParameterSetName -ne 'Status') {
    $top = git rev-parse --show-toplevel 2>$null
    if (-not $top) { throw 'not in a checkout; pass -Owner' }
    $Owner = Split-Path -Leaf $top
}

function Now { [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() }
function GameRunning { [bool](Get-Process -Name eu4 -ErrorAction SilentlyContinue) }

# Runs $action holding the mutex that guards the record.
function Locked([scriptblock]$action) {
    $mutex = [Threading.Mutex]::new($false, 'Global\eu4-harmony-dev-game-lock')
    try {
        try { [void]$mutex.WaitOne() } catch [Threading.AbandonedMutexException] {}
        & $action
    } finally {
        $mutex.ReleaseMutex()
        $mutex.Dispose()
    }
}

function ReadHeld {
    if (Test-Path $LockFile) { Get-Content -Raw $LockFile | ConvertFrom-Json }
}

function WriteHeld($held) {
    $held | ConvertTo-Json | Set-Content -LiteralPath $LockFile -Encoding utf8
}

function Stale($held) {
    -not (GameRunning) -and (Now) -gt $held.renewed + 60 * $held.leaseMinutes
}

# Starts the lease of the claim again while the game runs; under the mutex.
function KeepWhileRunning($held) {
    if ($held -and (GameRunning)) {
        $held.renewed = Now
        WriteHeld $held
    }
}

function Describe($held) {
    $left = [math]::Round(($held.renewed + 60 * $held.leaseMinutes - (Now)) / 60)
    $lease = if ($left -gt 0) { "lease ends in $left min" } else { "lease ran out $(-$left) min ago" }
    "$($held.owner) since $($held.since) ($($held.purpose)); $lease; backup: $(if ($held.backup) { $held.backup } else { 'none' })"
}

# Renews the claim if $Owner holds it; throws otherwise.
function RenewOwn {
    Locked {
        $held = ReadHeld
        if (-not $held -or $held.owner -ne $Owner) { throw "$Owner does not hold the game" }
        $held.renewed = Now
        WriteHeld $held
        $held
    }
}

switch ($PSCmdlet.ParameterSetName) {
    'Status' {
        $held = ReadHeld
        if ($held) {
            Write-Host "held by $(Describe $held)"
            if (Stale $held) { Write-Host 'the lease has run out: the next claim takes over' }
        } else { Write-Host 'free' }
        Write-Host "eu4.exe running: $(GameRunning)"
        if (Test-Path $Backups) {
            foreach ($dir in Get-ChildItem -LiteralPath $Backups -Directory) {
                if (-not $held -or $held.backup -ne $dir.FullName) { Write-Host "backup no claim refers to: $($dir.FullName)" }
            }
        }
        return
    }
    'Renew' {
        $held = RenewOwn
        Write-Host "renewed: $(Describe $held)"
        return
    }
    'Watch' {
        while ($true) {
            $held = Locked {
                $held = ReadHeld
                if ($held -and $held.owner -eq $Owner) { KeepWhileRunning $held }
                $held
            }
            if (-not $held -or $held.owner -ne $Owner) { Write-Host "$Owner no longer holds the game"; exit 0 }
            $left = $held.renewed + 60 * $held.leaseMinutes - (Now)
            if ($left -le 60 * $Warn) {
                Write-Host "the lease ends in $([math]::Max(0, [math]::Round($left / 60))) min: -Renew and -Watch again to keep the game, or -Release"
                exit 3
            }
            Start-Sleep -Seconds 30
        }
    }
    'Release' {
        $held = RenewOwn
        if (GameRunning) { throw 'eu4.exe is running; close it first' }
        if ($held.backup) {
            PlayerState restore $held.backup -keepCrashes:$KeepCrashes
        }
        Locked {
            $now = ReadHeld
            if ($now -and $now.owner -eq $Owner) { Remove-Item -LiteralPath $LockFile }
        }
        Write-Host 'released: the game is free'
        return
    }
}

# Claim: one attempt under the mutex.
function TryClaim {
    Locked {
        $held = ReadHeld
        if ($held -and $held.owner -eq $Owner) {
            $held.renewed = Now
            WriteHeld $held
            return @{ State = 'held'; Claim = $held }
        }
        KeepWhileRunning $held
        if ($held -and -not (Stale $held)) { return @{ State = 'busy'; Claim = $held } }
        if (-not $held -and (GameRunning)) { return @{ State = 'busy'; Claim = $null } }
        WriteHeld ([pscustomobject]@{
                owner = $Owner
                purpose = $Purpose
                checkout = (Get-Location).Path
                since = (Get-Date -Format 'yyyy-MM-dd HH:mm')
                renewed = Now
                leaseMinutes = $Lease
                backup = $null
            })
        @{ State = 'claimed'; Claim = $held }
    }
}

$said = $null
while ($true) {
    $try = TryClaim
    if ($try.State -ne 'busy') { break }
    $who = if ($try.Claim) { Describe $try.Claim } else { 'nobody, but eu4.exe is running: the player has it' }
    if (-not $Wait) { Write-Host "busy: held by $who"; exit 2 }
    if ($who -ne $said) { Write-Host "waiting: held by $who"; $said = $who }
    Start-Sleep -Seconds 15
}

if ($try.State -eq 'held') {
    Write-Host "already held: $(Describe $try.Claim)"
    return
}

$gone = $try.Claim
if ($gone) {
    Write-Host "took over from $(Describe $gone)"
    if ($gone.backup -and (Test-Path $gone.backup)) {
        Write-Host "restoring the backup $($gone.owner) left"
        PlayerState restore $gone.backup
    }
}

if (-not $NoBackup) {
    $dir = Join-Path $Backups "$Owner-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    try {
        PlayerState backup $dir
    } catch {
        # No claim without its backup: give the game back. A partial backup is
        # never restored; the status lists it.
        Locked {
            $held = ReadHeld
            if ($held -and $held.owner -eq $Owner) { Remove-Item -LiteralPath $LockFile }
        }
        Write-Host "not claimed: the backup failed ($_); the game is free again"
        exit 1
    }
    # Recorded only once complete: a partial backup must never be restored.
    Locked {
        $held = ReadHeld
        if (-not $held -or $held.owner -ne $Owner) { throw "lost the claim while backing up; the backup is at $dir" }
        $held.backup = $dir
        $held.renewed = Now
        WriteHeld $held
    }
}
Write-Host "claimed: $(Describe (ReadHeld))"
