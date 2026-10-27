<#
.SYNOPSIS
Captures the primary screen to a PNG, for checking a running game
(tools/README.md).

.DESCRIPTION
Captures at physical resolution (the script is DPI aware), then either
shrinks the whole screen by -Scale, or saves the -Crop rectangle unscaled.
Coordinates are the ones tools/game-input.ps1 takes.

.PARAMETER Out
The PNG to write.

.PARAMETER Scale
Integer factor the whole screen is shrunk by (default 2).

.PARAMETER Crop
x, y, width, height in physical pixels, saved at full size.
#>
param([Parameter(Mandatory)][string]$Out, [int]$Scale = 2, [int[]]$Crop)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System.Runtime.InteropServices;
public static class ScreenDpi { [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(System.IntPtr v); }
"@
[ScreenDpi]::SetProcessDpiAwarenessContext([System.IntPtr]::new(-4)) | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$full = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($full)
$g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
if ($Crop) {
    $part = $full.Clone([System.Drawing.Rectangle]::new($Crop[0], $Crop[1], $Crop[2], $Crop[3]), $full.PixelFormat)
    $part.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    "crop $($Crop -join ',') -> $Out"
    return
}
$small = New-Object System.Drawing.Bitmap ([int]($bounds.Width / $Scale)), ([int]($bounds.Height / $Scale))
$g2 = [System.Drawing.Graphics]::FromImage($small)
$g2.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g2.DrawImage($full, 0, 0, $small.Width, $small.Height)
$small.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"$($bounds.Width)x$($bounds.Height) -> $($small.Width)x$($small.Height) $Out"
