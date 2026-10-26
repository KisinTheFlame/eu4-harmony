<#
.SYNOPSIS
Copies the built proxy and core into a game directory, or removes them.

Mirrors the player install instructions: winmm.dll in the game root,
everything else under eu4-harmony\. A foreign winmm.dll already in the root is
moved to eu4-harmony\winmm.dll so the proxy chain-loads it.
#>
param(
    [string]$GameDir = 'E:\SteamLibrary\steamapps\common\Europa Universalis IV',
    [switch]$Uninstall
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repo 'target\x86_64-pc-windows-msvc\release'
$proxySrc = Join-Path $out 'winmm.dll'
$coreSrc = Join-Path $out 'eu4_harmony_core.dll'
$root = $GameDir
$harmony = Join-Path $root 'eu4-harmony'
$proxyDst = Join-Path $root 'winmm.dll'
$coreDst = Join-Path $harmony 'eu4_harmony_core.dll'
$chained = Join-Path $harmony 'winmm.dll'

if (-not (Test-Path (Join-Path $root 'eu4.exe'))) { throw "no eu4.exe in $root" }
if (Get-Process -Name eu4 -ErrorAction SilentlyContinue) { throw 'eu4.exe is running; close it first' }

function IsOurs($path) {
    # Our proxy exports eu4h_* stub symbols; a foreign winmm.dll does not mention them.
    (Test-Path $path) -and (Select-String -Path $path -Pattern 'eu4_harmony_bootstrap' -Quiet)
}

if ($Uninstall) {
    if (IsOurs $proxyDst) { Remove-Item $proxyDst; Write-Host "removed $proxyDst" }
    elseif (Test-Path $proxyDst) { Write-Host "left $proxyDst alone: not ours" }
    if (Test-Path $coreDst) { Remove-Item $coreDst; Write-Host "removed $coreDst" }
    if (Test-Path $chained) { Move-Item $chained $proxyDst; Write-Host "restored chained winmm.dll to $proxyDst" }
    return
}

foreach ($f in $proxySrc, $coreSrc) { if (-not (Test-Path $f)) { throw "missing $f; run cargo build --release" } }
New-Item -ItemType Directory -Force (Join-Path $harmony 'logs') | Out-Null
if ((Test-Path $proxyDst) -and -not (IsOurs $proxyDst)) {
    if (Test-Path $chained) { throw "both $proxyDst and $chained exist and neither is ours" }
    Move-Item $proxyDst $chained
    Write-Host "moved existing third-party winmm.dll to $chained (will be chain-loaded)"
}
Copy-Item $proxySrc $proxyDst -Force
Copy-Item $coreSrc $coreDst -Force
Write-Host "installed:`n  $proxyDst`n  $coreDst"
