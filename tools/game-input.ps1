<#
.SYNOPSIS
Brings the running EU4 window forward and sends it mouse and keyboard input,
for trying things in a running game (tools/README.md).

.DESCRIPTION
Coordinates are physical screen pixels (the script is DPI aware), as
tools/screenshot.ps1 captures them. The steps run in this order: -Wheel, -Click,
-RightClick, -Drag, -Type, -Keys.

Injected keys are not quite a keyboard: one injected Backspace press deletes
more than one character in the game's edit boxes, on builds with and without
the framework alike. Count deletions with a real keyboard.

.PARAMETER Wheel
Mouse wheel notches, positive away from the user, at -At or the screen centre.

.PARAMETER Click
x, y to left-click.

.PARAMETER RightClick
x, y to right-click.

.PARAMETER Drag
x1, y1, x2, y2 to drag from and to with the left button.

.PARAMETER Type
Text sent as Unicode key events, one character at a time.

.PARAMETER Keys
Key names, each optionally with modifiers: LEFT, SHIFT+END, CTRL+V. Names:
the keys in $keyCodes below, and any letter or digit. Unlike -Type, these
are key presses, so an input method takes them (type pinyin with
-Keys B,E,I,J,I,N,G).
#>
param([int]$Wheel = 0, [int[]]$At, [int[]]$Click, [int[]]$RightClick, [int[]]$Drag, [string]$Type, [string[]]$Keys)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class GameInput {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit, Size = 40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("user32.dll")] public static extern uint MapVirtualKeyW(uint code, uint type);
  public static void Key(ushort vk, ushort scan, uint flags) {
    var i = new INPUT[1]; i[0].type = 1; i[0].ki.wVk = vk; i[0].ki.wScan = scan; i[0].ki.dwFlags = flags;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }
}
"@
$KEY_UP = 0x2; $KEY_UNICODE = 0x4
$MOUSE_LEFT_DOWN = 0x02; $MOUSE_LEFT_UP = 0x04; $MOUSE_RIGHT_DOWN = 0x08; $MOUSE_RIGHT_UP = 0x10; $MOUSE_WHEEL = 0x0800

# Per-monitor DPI awareness v2: coordinates are physical pixels.
[GameInput]::SetProcessDpiAwarenessContext([IntPtr]::new(-4)) | Out-Null
$game = Get-Process eu4 -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $game) { throw 'eu4.exe is not running' }
[GameInput]::ShowWindow($game.MainWindowHandle, 9) | Out-Null
[GameInput]::SetForegroundWindow($game.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 500

function Press([int[]]$from, [uint32]$down = $MOUSE_LEFT_DOWN) { [GameInput]::SetCursorPos($from[0], $from[1]) | Out-Null; Start-Sleep -Milliseconds 150; [GameInput]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero) }
function Release([uint32]$up = $MOUSE_LEFT_UP) { [GameInput]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 300 }

if ($Wheel) {
    Add-Type -AssemblyName System.Windows.Forms
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $where = if ($At) { $At } else { @([int]($bounds.Width / 2), [int]($bounds.Height / 2)) }
    [GameInput]::SetCursorPos($where[0], $where[1]) | Out-Null
    for ($i = 0; $i -lt [Math]::Abs($Wheel); $i++) { [GameInput]::mouse_event($MOUSE_WHEEL, 0, 0, [Math]::Sign($Wheel) * 120, [UIntPtr]::Zero); Start-Sleep -Milliseconds 120 }
}
if ($Click) { Press $Click; Start-Sleep -Milliseconds 60; Release }
if ($RightClick) { Press $RightClick $MOUSE_RIGHT_DOWN; Start-Sleep -Milliseconds 60; Release $MOUSE_RIGHT_UP }
if ($Drag) {
    Press $Drag[0..1]
    for ($s = 1; $s -le 10; $s++) {
        [GameInput]::SetCursorPos([int]($Drag[0] + ($Drag[2] - $Drag[0]) * $s / 10), [int]($Drag[1] + ($Drag[3] - $Drag[1]) * $s / 10)) | Out-Null
        Start-Sleep -Milliseconds 40
    }
    Release
}
if ($Type) {
    foreach ($c in $Type.ToCharArray()) {
        [GameInput]::Key(0, [uint16]$c, $KEY_UNICODE); [GameInput]::Key(0, [uint16]$c, $KEY_UNICODE -bor $KEY_UP)
        Start-Sleep -Milliseconds 80
    }
}
# Virtual key, scan code, extended-key flag.
$keyCodes = @{
    LEFT = 0x25, 0x4B, 1; RIGHT = 0x27, 0x4D, 1; UP = 0x26, 0x48, 1; DOWN = 0x28, 0x50, 1
    HOME = 0x24, 0x47, 1; END = 0x23, 0x4F, 1; BACK = 0x08, 0x0E, 0; DELETE = 0x2E, 0x53, 1
    RETURN = 0x0D, 0x1C, 0; ESCAPE = 0x1B, 0x01, 0; SPACE = 0x20, 0x39, 0
    CTRL = 0x11, 0x1D, 0; SHIFT = 0x10, 0x2A, 0
    F1 = 0x70, 0x3B, 0; F2 = 0x71, 0x3C, 0; F3 = 0x72, 0x3D, 0
    PLUS = 0xBB, 0x0D, 0; MINUS = 0xBD, 0x0C, 0
}
# Letters and digits: the virtual key is the character itself.
foreach ($c in [char[]]'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789') {
    $keyCodes["$c"] = [int]$c, [int][GameInput]::MapVirtualKeyW([uint32]$c, 0), 0
}
foreach ($k in $Keys) {
    $parts = $k.ToUpperInvariant() -split '\+'
    foreach ($p in $parts) { if (-not $keyCodes.ContainsKey($p)) { throw "unknown key $p" } }
    $modifiers = if ($parts.Count -gt 1) { $parts[0..($parts.Count - 2)] } else { @() }
    foreach ($m in $modifiers) { $v, $s, $e = $keyCodes[$m]; [GameInput]::Key($v, $s, $e); Start-Sleep -Milliseconds 30 }
    $v, $s, $e = $keyCodes[$parts[-1]]
    [GameInput]::Key($v, $s, $e); Start-Sleep -Milliseconds 5; [GameInput]::Key($v, $s, $e -bor $KEY_UP); Start-Sleep -Milliseconds 40
    foreach ($m in $modifiers) { $v, $s, $e = $keyCodes[$m]; [GameInput]::Key($v, $s, $e -bor $KEY_UP); Start-Sleep -Milliseconds 30 }
    Start-Sleep -Milliseconds 120
}
Start-Sleep -Milliseconds 600
