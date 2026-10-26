<#
.SYNOPSIS
Builds the framework and the hello example, and copies hello.dll into its mod.
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    $win64 = Join-Path $repo 'examples\hello\eu4-harmony\win64'
    New-Item -ItemType Directory -Force $win64 | Out-Null
    Copy-Item (Join-Path $repo 'target\x86_64-pc-windows-msvc\release\hello.dll') $win64 -Force
    Write-Host "built; hello.dll copied to $win64"
} finally {
    Pop-Location
}
