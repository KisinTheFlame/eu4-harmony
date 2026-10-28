<#
.SYNOPSIS
Prints the game folder: EU4_GAME_DIR when it is set, or else the Steam
library that holds Europa Universalis IV. The other tools take it as their
-GameDir by default.
#>
$ErrorActionPreference = 'Stop'
if ($env:EU4_GAME_DIR) { return $env:EU4_GAME_DIR }
$steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
if ($steam) {
    $libraries = @($steam)
    $folders = Join-Path $steam 'steamapps\libraryfolders.vdf'
    if (Test-Path $folders) {
        $libraries += Select-String -Path $folders -Pattern '"path"\s+"([^"]+)"' | ForEach-Object { $_.Matches[0].Groups[1].Value -replace '\\\\', '\' }
    }
    foreach ($library in $libraries) {
        $game = Join-Path $library 'steamapps\common\Europa Universalis IV'
        if (Test-Path (Join-Path $game 'eu4.exe')) { return (Resolve-Path $game).Path }
    }
}
throw 'cannot find the game: pass -GameDir or set EU4_GAME_DIR'
