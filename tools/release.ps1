<#
.SYNOPSIS
Packs the release build into what a player installs: a folder and a zip of
it holding version.dll and the eu4-harmony\ folder, with the core and the
command line tool, eu4-harmony.exe (player-files.ps1 lists them).

A player puts the folder's contents into the game's root folder, as the
README's installing section says. The player build is built first, with no
features, so a developer build built last is never packed. -Out names the
folder, target\eu4-harmony by default; the zip is next to it, with the same
name. Both are made anew.
#>
param(
    [string]$Out
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'player-files.ps1')
$repo = Split-Path -Parent $PSScriptRoot
$built = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'
if (-not $Out) { $Out = Join-Path $repo 'target\eu4-harmony' }
$zip = "$Out.zip"

# From the repository, whose .cargo\config.toml names the target.
Push-Location $repo
try { & cargo build --release --locked } finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed' }
foreach ($f in $PlayerFiles) {
    $from = Join-Path $built $f.Built
    if (-not (Test-Path $from)) { throw "missing $from after cargo build --release" }
}
if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
if (Test-Path $zip) { Remove-Item $zip -Force }
foreach ($f in $PlayerFiles) {
    $to = Join-Path $Out $f.Installed
    New-Item -ItemType Directory -Force (Split-Path $to) | Out-Null
    Copy-Item (Join-Path $built $f.Built) $to
}
Compress-Archive -Path (Join-Path $Out '*') -DestinationPath $zip
Write-Host "packed:`n  $Out`n  $zip"
Get-ChildItem $Out -Recurse -File | ForEach-Object { Write-Host "    $($_.FullName.Substring($Out.Length + 1))" }
