<#
.SYNOPSIS
Registers a development mod with the Paradox launcher by writing a .mod
descriptor into the user directory that points at the mod's folder.
Enable the mod in the launcher's playset afterwards.
#>
param(
    [Parameter(Mandatory)] [string]$ModDir,
    # File name of the registration (without .mod); defaults to the folder name.
    [string]$Name,
    [string]$UserDir = (Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Paradox Interactive\Europa Universalis IV')
)
$ErrorActionPreference = 'Stop'
$ModDir = (Resolve-Path $ModDir).Path
$descriptor = Join-Path $ModDir 'descriptor.mod'
if (-not (Test-Path $descriptor)) { throw "no descriptor.mod in $ModDir" }
$name = if ($Name) { $Name } else { Split-Path -Leaf $ModDir }
$target = Join-Path $UserDir "mod\eu4-harmony-$name.mod"
$body = (Get-Content $descriptor -Raw).TrimEnd() + "`npath=`"$($ModDir -replace '\\', '/')`"`n"
New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
[IO.File]::WriteAllText($target, $body, (New-Object System.Text.UTF8Encoding $false))
Write-Host "wrote $target"
