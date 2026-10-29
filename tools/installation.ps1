<#
.SYNOPSIS
The player's installation: the files the release build makes that go into
the game folder, and where each goes.

Dot-source it for $Installation: each item's Built is its name in the build
output (target\x86_64-pc-windows-msvc\release), Installed its path in the
game folder. dev-install.ps1 installs them, release.ps1 packs them for
players; the README's installing section names the same files.
#>
$Installation = @(
    # The proxy, which loads the core into eu4.exe (ADR 0011).
    [pscustomobject]@{ Built = 'version.dll'; Installed = 'version.dll' }
    [pscustomobject]@{ Built = 'eu4_harmony_core.dll'; Installed = 'eu4-harmony\eu4_harmony_core.dll' }
    # The command line tool, for remote control (eu4-harmony game) and for
    # making mods.
    [pscustomobject]@{ Built = 'eu4-harmony.exe'; Installed = 'eu4-harmony\eu4-harmony.exe' }
)
