<#
.SYNOPSIS
Backs up the player's files a run of the game may change, and puts them back
afterwards, as AGENTS.md asks of a run in the game.

-Backup <dir> copies them into <dir>, a folder that does not exist yet, outside
the game and user folders. -Restore <dir> first lists what the run changed,
then puts every file back as the backup has it: what the run added is deleted,
what it changed or deleted is copied back. It then compares each with the
backup, removes the crash folders the run left (-KeepCrashes keeps them, for
a look at their dumps first), and deletes the backup once all match (-Keep
keeps it).

The files: save games\, the framework's folder eu4-harmony\ (its logs, settings
and extension save data) and every file at the top of the user folder (the
game's settings, its login cache, the playset in dlc_load.json, ...), in the
user folder: the one userdir.txt in the game folder names, or else
Documents\Paradox Interactive\Europa Universalis IV, as the game finds it;
-UserDir names it instead. The game folder is found by game-dir.ps1 unless
-GameDir names it.
#>
param(
    [Parameter(Mandatory, ParameterSetName = 'Backup')][string]$Backup,
    [Parameter(Mandatory, ParameterSetName = 'Restore')][string]$Restore,
    [Parameter(ParameterSetName = 'Restore')][switch]$Keep,
    [Parameter(ParameterSetName = 'Restore')][switch]$KeepCrashes,
    [string]$GameDir,
    [string]$UserDir
)
$ErrorActionPreference = 'Stop'

# The folders a run may change, relative to the user folder; every file at
# its top is added to them.
$Folders = @('save games', 'eu4-harmony')

if (Get-Process -Name eu4 -ErrorAction SilentlyContinue) { throw 'eu4.exe is running; close it first' }
if (-not $UserDir) {
    if (-not $GameDir) { $GameDir = & (Join-Path $PSScriptRoot 'game-dir.ps1') }
    $named = Join-Path $GameDir 'userdir.txt'
    # An empty file reads as null.
    $text = if (Test-Path $named) { "$(Get-Content -Raw $named)".Trim([char]0xfeff, ' ', "`t", "`r", "`n") } else { '' }
    $UserDir = if ($text) {
        if ([IO.Path]::IsPathRooted($text)) { $text } else { Join-Path $GameDir $text }
    } else {
        Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Paradox Interactive\Europa Universalis IV'
    }
}
$UserDir = [IO.Path]::GetFullPath($UserDir)
if (-not (Test-Path $UserDir)) { throw "no user folder at $UserDir" }
Write-Host "user folder: $UserDir"

# Every file under $root\$item, by its path under $root, with its hash.
function Files($root, $item) {
    $path = Join-Path $root $item
    $found = @{}
    if (Test-Path $path -PathType Leaf) {
        $found[$item] = (Get-FileHash -LiteralPath $path).Hash
    } elseif (Test-Path $path -PathType Container) {
        foreach ($f in Get-ChildItem -LiteralPath $path -Recurse -File -Force) {
            $found[$f.FullName.Substring($root.Length).TrimStart('\')] = (Get-FileHash -LiteralPath $f.FullName).Hash
        }
    }
    $found
}

# The files at the top of the user folder now.
function TopFiles {
    @(Get-ChildItem -LiteralPath $UserDir -File -Force | ForEach-Object Name)
}

# The crash folders there are now.
function Crashes {
    $crashes = Join-Path $UserDir 'crashes'
    if (Test-Path $crashes) { @(Get-ChildItem -LiteralPath $crashes -Directory | ForEach-Object Name) } else { @() }
}

if ($Backup) {
    $Backup = [IO.Path]::GetFullPath($Backup)
    if (Test-Path $Backup) { throw "$Backup exists already; name a new folder" }
    New-Item -ItemType Directory $Backup | Out-Null
    $items = $Folders + (TopFiles)
    foreach ($item in $items) {
        $from = Join-Path $UserDir $item
        if (-not (Test-Path $from)) { Write-Host "  $item (none)"; continue }
        $to = Join-Path $Backup "files\$item"
        New-Item -ItemType Directory -Force (Split-Path $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to -Recurse -Force
        Write-Host "  $item"
    }
    Set-Content -LiteralPath (Join-Path $Backup 'items.txt') -Value $items
    Set-Content -LiteralPath (Join-Path $Backup 'crashes.txt') -Value (Crashes)
    Set-Content -LiteralPath (Join-Path $Backup 'user-folder.txt') -Value $UserDir
    Write-Host "backed up to $Backup; afterwards: player-state.ps1 -Restore '$Backup'"
    return
}

$Restore = [IO.Path]::GetFullPath($Restore)
$saved = Join-Path $Restore 'files'
if (-not (Test-Path (Join-Path $Restore 'user-folder.txt'))) { throw "$Restore holds no backup of player-state.ps1" }
$from = (Get-Content -LiteralPath (Join-Path $Restore 'user-folder.txt')).Trim()
if ($from -ne $UserDir) { throw "the backup is of $from, not of $UserDir; pass -UserDir" }

# What was backed up, and the files the run added at the top.
$items = @(Get-Content -LiteralPath (Join-Path $Restore 'items.txt')) + (TopFiles) | Sort-Object -Unique
$differ = 0
foreach ($item in $items) {
    $before = Files $saved $item
    $now = Files $UserDir $item
    foreach ($path in $now.Keys | Sort-Object) {
        if (-not $before.ContainsKey($path)) { Write-Host "  added:   $path" }
        elseif ($before[$path] -ne $now[$path]) { Write-Host "  changed: $path" }
    }
    foreach ($path in $before.Keys | Sort-Object) {
        if (-not $now.ContainsKey($path)) { Write-Host "  deleted: $path" }
    }
    $target = Join-Path $UserDir $item
    if (Test-Path $target) { Remove-Item -LiteralPath $target -Recurse -Force }
    $copy = Join-Path $saved $item
    if (Test-Path $copy) {
        New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
        Copy-Item -LiteralPath $copy -Destination $target -Recurse -Force
    }
    $after = Files $UserDir $item
    foreach ($path in @($before.Keys) + @($after.Keys) | Sort-Object -Unique) {
        if ($before[$path] -ne $after[$path]) { Write-Host "  NOT RESTORED: $path"; $differ++ }
    }
}

$known = @(Get-Content -LiteralPath (Join-Path $Restore 'crashes.txt') -ErrorAction SilentlyContinue)
foreach ($crash in Crashes | Where-Object { $_ -notin $known }) {
    $path = Join-Path $UserDir "crashes\$crash"
    if ($KeepCrashes) { Write-Host "  crash folder left: $path" }
    else { Remove-Item -LiteralPath $path -Recurse -Force; Write-Host "  crash folder removed: $path" }
}

if ($differ) { throw "$differ file(s) differ from the backup, which stays at $Restore" }
Write-Host 'every file is as the backup has it'
if ($Keep) { Write-Host "the backup stays at $Restore" }
else { Remove-Item -LiteralPath $Restore -Recurse -Force; Write-Host "removed the backup $Restore" }
