<#
.SYNOPSIS
Copies the built proxy, core and command line tool into a game directory, or
removes them.

Mirrors the player install instructions, with the files player-files.ps1
lists: version.dll in the game root, the core and eu4-harmony.exe under
eu4-harmony\. A foreign version.dll already in the root (a CJK font plugin's
loader, say) is moved to eu4-harmony\backup\version.dll; the proxy never
loads it (ADR 0011), and uninstalling puts it back. The game folder is found
by game-dir.ps1 unless -GameDir names it.
#>
param(
    [string]$GameDir = (& (Join-Path $PSScriptRoot 'game-dir.ps1')),
    [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'player-files.ps1')
$repo = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'
$root = $GameDir
$harmony = Join-Path $root 'eu4-harmony'
$proxyDst = Join-Path $root 'version.dll'
$backup = Join-Path $harmony 'backup\version.dll'

if (-not (Test-Path (Join-Path $root 'eu4.exe'))) { throw "no eu4.exe in $root" }
if (Get-Process -Name eu4 -ErrorAction SilentlyContinue) { throw 'eu4.exe is running; close it first' }

function IsOurs($path) {
    # Our proxy names the core's bootstrap export; a foreign DLL does not.
    (Test-Path $path) -and (Select-String -Path $path -Pattern 'eu4_harmony_bootstrap' -Quiet)
}

if ($Uninstall) {
    if (IsOurs $proxyDst) { Remove-Item $proxyDst; Write-Host "removed $proxyDst" }
    elseif (Test-Path $proxyDst) { Write-Host "left $proxyDst alone: not ours" }
    if (Test-Path $backup) {
        if (Test-Path $proxyDst) { throw "cannot restore ${backup}: ${proxyDst} still exists" }
        Move-Item $backup $proxyDst
        Write-Host "restored the backed-up version.dll to $proxyDst"
    }
    # Everything else under eu4-harmony\ is ours: the core, the command line
    # tool, and the logs the proxy, and the core when it cannot find the user
    # folder, write there.
    if (Test-Path $harmony) { Remove-Item $harmony -Recurse -Force; Write-Host "removed $harmony" }
    Write-Host 'logs, reports and extension save data stay in Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony'
    return
}

foreach ($f in $PlayerFiles) {
    $built = Join-Path $out $f.Built
    if (-not (Test-Path $built)) { throw "missing $built; run cargo build --release" }
}
New-Item -ItemType Directory -Force $harmony | Out-Null
if ((Test-Path $proxyDst) -and -not (IsOurs $proxyDst)) {
    if (Test-Path $backup) { throw "both $proxyDst and $backup exist and neither is ours" }
    New-Item -ItemType Directory -Force (Split-Path $backup) | Out-Null
    Move-Item $proxyDst $backup
    Write-Host "moved the existing version.dll to $backup (kept, never loaded)"
}
$installed = foreach ($f in $PlayerFiles) {
    $to = Join-Path $root $f.Installed
    Copy-Item (Join-Path $out $f.Built) $to -Force
    "  $to"
}
Write-Host "installed:`n$($installed -join "`n")"
