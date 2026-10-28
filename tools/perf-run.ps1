<#
.SYNOPSIS
Measures one run of the game, for comparing setups (docs/performance.md).

.DESCRIPTION
Starts eu4.exe directly and records, into <Out>\<Name>.json:
- the seconds from launch to the main menu, which it recognizes on screen;
- memory 20 s later (working set and private bytes);
- unless -MenuOnly: a new game as Ming from the 1444 East Asia bookmark, the
  seconds it takes to load, CPU while paused for 30 s and while running at
  speed 5 for 120 s (in cores: CPU seconds per second), memory, and crops of
  the date before and after the run (<Name>-date0.png, <Name>-date1.png).
With -MenuOnly it records CPU at the idle main menu for 30 s instead.
The game is then closed. Back up saves and settings first: the run autosaves.

Clicks are physical pixels for a 3840x2160 screen at the game's default
interface scale. The run recognizes two screens by comparing a region with a
reference taken on this machine, once:
- the main menu: with the main menu showing, run with -Reference;
- the start-of-game introduction: with it open, save its bottom left
  ornament with screenshot.ps1 -Crop 1195,1500,100,80 -Out <Out>\intro-corner.png.
Keep the game window in front, and the machine otherwise idle, while it runs.

.PARAMETER Name
The run's name, used for its files.

.PARAMETER Out
The folder for results and references.
#>
param([string]$Name, [Parameter(Mandatory)][string]$Out, [string]$GameDir = (& (Join-Path $PSScriptRoot 'game-dir.ps1')), [switch]$Reference, [switch]$MenuOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$gdi = [AppDomain]::CurrentDomain.GetAssemblies() | Where-Object { $_.GetName().Name -in 'System.Drawing.Common', 'System.Drawing.Primitives', 'System.Private.Windows.GdiPlus', 'System.Private.Windows.Core', 'System.Runtime', 'System.Runtime.InteropServices' } | ForEach-Object Location
Add-Type -ReferencedAssemblies $gdi @"
using System; using System.Drawing; using System.Runtime.InteropServices;
public static class ScreenRegion {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  // Every fourth pixel across and down, as RGB bytes.
  public static byte[] Sample(Bitmap b) {
    var o = new byte[(b.Width / 4) * (b.Height / 4) * 3]; int i = 0;
    for (int y = 0; y + 4 <= b.Height; y += 4) for (int x = 0; x + 4 <= b.Width; x += 4) {
      var c = b.GetPixel(x, y); o[i++] = c.R; o[i++] = c.G; o[i++] = c.B; }
    return o;
  }
  public static byte[] Grab(int x, int y, int w, int h) {
    using (var b = new Bitmap(w, h)) {
      using (var g = Graphics.FromImage(b)) g.CopyFromScreen(x, y, 0, 0, new Size(w, h));
      return Sample(b);
    }
  }
  public static byte[] Load(string path) { using (var b = new Bitmap(path)) return Sample(b); }
  // Mean difference per colour value, 0 to 255.
  public static double Diff(byte[] a, byte[] b) { long s = 0; for (int i = 0; i < a.Length; i++) s += Math.Abs(a[i] - b[i]); return (double)s / a.Length; }
}
"@
[ScreenRegion]::SetProcessDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
# The "IV" of the logo, top left of the main menu: a texture, the same in every setup.
$menuArea = 690, 60, 240, 200
# The bottom left ornament of the start-of-game introduction window.
$introArea = 1195, 1500, 100, 80
New-Item -ItemType Directory -Force $Out | Out-Null
$menuRef = Join-Path $Out 'menu-reference.bin'
if ($Reference) { [IO.File]::WriteAllBytes($menuRef, [ScreenRegion]::Grab($menuArea[0], $menuArea[1], $menuArea[2], $menuArea[3])); "saved $menuRef"; return }
$menuBytes = [IO.File]::ReadAllBytes($menuRef)

function Sample($p) { $p.Refresh(); [pscustomobject]@{ cpu = $p.TotalProcessorTime.TotalSeconds; ws = [math]::Round($p.WorkingSet64 / 1MB); priv = [math]::Round($p.PrivateMemorySize64 / 1MB); peak = [math]::Round($p.PeakWorkingSet64 / 1MB) } }
function Click($x, $y) { & "$PSScriptRoot\game-input.ps1" -Click $x, $y }
function Shot($file, $crop) { & "$PSScriptRoot\screenshot.ps1" -Out (Join-Path $Out $file) -Crop $crop | Out-Null }
function WaitFor($bytes, $area, $limit) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt $limit) {
        Start-Sleep -Milliseconds 250
        if ([ScreenRegion]::Diff($bytes, [ScreenRegion]::Grab($area[0], $area[1], $area[2], $area[3])) -lt 12) { return $clock.Elapsed.TotalSeconds }
    }
    throw "not seen within $limit s"
}
function Save($result) { $result | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $Out "$Name.json"); $result | ConvertTo-Json -Depth 3 }

# A game closed a moment ago may take a few seconds to exit.
foreach ($i in 1..30) { if (-not (Get-Process eu4 -ErrorAction SilentlyContinue)) { break }; Start-Sleep -Milliseconds 500 }
if (Get-Process eu4 -ErrorAction SilentlyContinue) { throw 'eu4 is running' }
$result = [ordered]@{ name = $Name; started = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss') }
Start-Process -FilePath "$GameDir\eu4.exe" -WorkingDirectory $GameDir
$result.menu_seconds = [math]::Round((WaitFor $menuBytes $menuArea 300), 1)
$p = Get-Process eu4 | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
Start-Sleep -Seconds 20
$result.menu = Sample $p
if ($MenuOnly) {
    $a = Sample $p; Start-Sleep -Seconds 30; $b = Sample $p
    $result.menu_idle_cpu_share = [math]::Round(($b.cpu - $a.cpu) / 30, 3)
    Stop-Process -Id $p.Id -Force
    Save $result
    return
}

$introBytes = [ScreenRegion]::Load((Join-Path $Out 'intro-corner.png'))
# Single player, the 1444 East Asia bookmark, Ming, Start, normal mode.
Click 1815 2025; Start-Sleep -Seconds 15
Click 120 300; Start-Sleep -Seconds 5
Click 2550 1200; Start-Sleep -Seconds 3
Click 3645 2109; Start-Sleep -Seconds 3
Click 2064 1347
$result.start_seconds = [math]::Round((WaitFor $introBytes $introArea 120), 1)
Start-Sleep -Seconds 2
Click 1920 1587; Start-Sleep -Seconds 10
$a = Sample $p; Start-Sleep -Seconds 30; $b = Sample $p
$result.paused = [ordered]@{ cpu_share = [math]::Round(($b.cpu - $a.cpu) / 30, 3); ws = $b.ws; priv = $b.priv }
Shot "$Name-date0.png" @(3380, 0, 460, 90)
# The date panel's + button, four times: speed 5.
foreach ($i in 1..4) { Click 3725 40 }
$a = Sample $p; & "$PSScriptRoot\game-input.ps1" -Keys SPACE; $t = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds 120
& "$PSScriptRoot\game-input.ps1" -Keys SPACE; $ran = $t.Elapsed.TotalSeconds; $b = Sample $p
$result.running = [ordered]@{ seconds = [math]::Round($ran, 1); cpu_share = [math]::Round(($b.cpu - $a.cpu) / $ran, 3); ws = $b.ws; priv = $b.priv; peak = $b.peak }
Start-Sleep -Seconds 1
Shot "$Name-date1.png" @(3380, 0, 460, 90)
Stop-Process -Id $p.Id -Force
Save $result
