<#
.SYNOPSIS
Copies the built proxy and core into a game directory, or removes them.

Mirrors the player install instructions: version.dll in the game root, the
core under eu4-harmony\. A foreign version.dll already in the root (a CJK
font plugin's loader, say) is moved to eu4-harmony\backup\version.dll; the
proxy never loads it (ADR 0011), and uninstalling puts it back. The game
folder is found by game-dir.ps1 unless -GameDir names it.
#>
param(
    [string]$GameDir = (& (Join-Path $PSScriptRoot 'game-dir.ps1')),
    [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'
$proxySrc = Join-Path $out 'version.dll'
$coreSrc = Join-Path $out 'eu4_harmony_core.dll'
$root = $GameDir
$harmony = Join-Path $root 'eu4-harmony'
$proxyDst = Join-Path $root 'version.dll'
$coreDst = Join-Path $harmony 'eu4_harmony_core.dll'
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
    # Everything else under eu4-harmony\ is ours: the core and the logs the
    # proxy, and the core when it cannot find the user folder, write there.
    if (Test-Path $harmony) { Remove-Item $harmony -Recurse -Force; Write-Host "removed $harmony" }
    Write-Host 'logs, reports and extension save data stay in Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony'
    return
}

foreach ($f in $proxySrc, $coreSrc) { if (-not (Test-Path $f)) { throw "missing $f; run cargo build --release" } }
New-Item -ItemType Directory -Force $harmony | Out-Null
if ((Test-Path $proxyDst) -and -not (IsOurs $proxyDst)) {
    if (Test-Path $backup) { throw "both $proxyDst and $backup exist and neither is ours" }
    New-Item -ItemType Directory -Force (Split-Path $backup) | Out-Null
    Move-Item $proxyDst $backup
    Write-Host "moved the existing version.dll to $backup (kept, never loaded)"
}
Copy-Item $proxySrc $proxyDst -Force
Copy-Item $coreSrc $coreDst -Force
Write-Host "installed:`n  $proxyDst`n  $coreDst"
