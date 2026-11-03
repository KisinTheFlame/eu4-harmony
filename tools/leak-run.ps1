<#
.SYNOPSIS
Plays rounds of sessions on a developer build with the heap census, and
reports what the game's memory keeps of them (docs/leak-finding.md).

.DESCRIPTION
Needs the developer build with the heap census installed (cargo build
--release --features eu4-harmony-core/dev-heap-census, then dev-install.ps1),
and the game claimed and its player's files backed up: the run loads saves
and starts games, which autosave. It starts eu4.exe unless it runs, and plays
-Rounds rounds through remote control. Each round:
1. loads the first save from the main menu, and lets -Days days run;
2. loads the second save from that game, and lets a third of them run;
3. returns to the main menu;
4. starts a new game as -Country, and lets a third of the days run;
5. returns to the main menu.
After each round it asks memory and memory.leftovers and prints a line; after
the last, memory.leftovers looks for what still points at the leftovers too,
which stops the game for seconds. At the end it writes into -Out report.json,
every answer, and report.md, the rounds, the leftover sites that leave the
most per generation, and what holds them. The game is closed unless
-KeepGame.

.PARAMETER Saves
The two saves, by the names menu.saves lists; the two newest by default.
#>
param(
    [int]$Rounds = 10,
    [string[]]$Saves,
    [string]$Country = 'FRA',
    [int]$Days = 30,
    [int]$Sites = 40,
    [Parameter(Mandatory)][string]$Out,
    [string]$GameDir = (& (Join-Path $PSScriptRoot 'game-dir.ps1')),
    [switch]$KeepGame
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$cli = Join-Path $GameDir 'eu4-harmony\eu4-harmony.exe'
if (-not (Test-Path $cli)) { throw "no command line tool at ${cli}: install the developer build with dev-install.ps1" }
New-Item -ItemType Directory -Force $Out | Out-Null

# Asks the game; the result, or a stop with what it answered.
function Ask([string[]]$words, [int]$timeout = 600) {
    $text = (& $cli game @words --compact --timeout $timeout 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { throw "game $($words -join ' '): $text" }
    $text | ConvertFrom-Json
}

function MainMenu {
    $deadline = (Get-Date).AddMinutes(5)
    while ((Get-Date) -lt $deadline) {
        if (-not (Get-Process eu4 -ErrorAction SilentlyContinue)) { throw 'the game is gone' }
        try { if ((Ask 'menu' 10).place -eq 'main_menu') { return } } catch { }
        Start-Sleep 2
    }
    throw "the main menu did not come up in 5 minutes; whether the framework started is in the user folder's eu4-harmony\logs\core.log"
}

function Run([int]$days) {
    $null = Ask 'time.advance', "$days", '--speed', '5'
}

if (-not (Get-Process eu4 -ErrorAction SilentlyContinue)) {
    Start-Process -FilePath (Join-Path $GameDir 'eu4.exe') -WorkingDirectory $GameDir | Out-Null
}
MainMenu
try { $null = Ask 'memory' } catch { throw "the game has no memory group: install a developer build with the heap census ($_)" }
if (-not $Saves) { $Saves = @((Ask 'menu.saves') | Select-Object -First 2 | ForEach-Object name) }
if ($Saves.Count -lt 2) { throw 'two saves are needed: pass -Saves, or have two in the save games folder' }
"saves: $($Saves -join ', '); new games as $Country; $Days days"

$mb = { param($bytes) '{0:N1}' -f ($bytes / 1MB) }
$played = @()
$start = Ask 'memory'
"startup: private $(& $mb $start.private) MB, outside heaps $(& $mb $start.outside_heaps) MB, framework $(& $mb $start.framework_heap) MB, handles $($start.handles)"
for ($i = 1; $i -le $Rounds; $i++) {
    $began = Get-Date
    $null = Ask 'menu.load', $Saves[0]
    Run $Days
    $null = Ask 'menu.load', $Saves[1]
    Run ([Math]::Max(1, [int]($Days / 3)))
    $null = Ask 'menu.return'
    MainMenu
    $null = Ask 'menu.start', $Country
    Run ([Math]::Max(1, [int]($Days / 3)))
    $null = Ask 'menu.return'
    MainMenu
    Start-Sleep 5
    $memory = Ask 'memory'
    $asked = @('memory.leftovers', '--sites', "$Sites")
    if ($i -eq $Rounds) { $asked += '--holders', 'true' }
    $leftovers = Ask $asked
    $round = [ordered]@{ round = $i; seconds = [int]((Get-Date) - $began).TotalSeconds; memory = $memory; leftovers = $leftovers }
    $played += $round
    $grew = if ($i -gt 1) { ' ({0:+0.0;-0.0} MB)' -f (($memory.private - $played[-2].memory.private) / 1MB) } else { '' }
    "round $i ($($round.seconds) s): private $(& $mb $memory.private) MB$grew, heaps $(& $mb $memory.heaps_allocated) of $(& $mb $memory.heaps_committed) MB, outside heaps $(& $mb $memory.outside_heaps) MB, framework $(& $mb $memory.framework_heap) MB, handles $($memory.handles), generation $($memory.generation)"
}
$report = [ordered]@{ saves = $Saves; country = $Country; days = $Days; startup = $start; rounds = $played }
$report | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $Out 'report.json')

$md = @("# Leak run, $(Get-Date -Format 'yyyy-MM-dd HH:mm')", '', "Saves $($Saves -join ' and '), new games as $Country, $Days days; $Rounds rounds.", '')
$md += '| Round | Seconds | Private MB | Change | Heaps allocated MB | Outside heaps MB | Framework MB | Handles |', '| --- | --- | --- | --- | --- | --- | --- | --- |'
$before = $start.private
foreach ($r in $played) {
    $m = $r.memory
    $md += "| $($r.round) | $($r.seconds) | $(& $mb $m.private) | $('{0:+0.0;-0.0}' -f (($m.private - $before) / 1MB)) | $(& $mb $m.heaps_allocated) | $(& $mb $m.outside_heaps) | $(& $mb $m.framework_heap) | $($m.handles) |"
    $before = $m.private
}
if ($played) {
    $last = $played[-1].leftovers
    $md += '', "## Leftovers at generation $($last.generation)", '', "$($last.blocks) blocks, $(& $mb $last.bytes) MB in all.", ''
    $md += '| Site | Function | Trend | Per generation KB | Leftover KB | Blocks | Let go at (blocks) |', '| --- | --- | --- | --- | --- | --- | --- |'
    foreach ($s in $last.sites) {
        $freed = ($s.freed_at | ForEach-Object { "$($_.site) in $($_.function) ($($_.blocks))" }) -join '; '
        $md += "| $($s.site) | $($s.function) | $($s.trend) | $('{0:N0}' -f ($s.per_generation / 1KB)) | $('{0:N0}' -f ($s.bytes / 1KB)) | $($s.blocks) | $freed |"
    }
}
if ($played -and $last.sites[0].holders) {
    $md += '', '## What holds them', '', 'Each leftover counted once, by the best that points at it: the game''s data, other live blocks, only the leftovers listed, or nothing.', ''
    $md += '| Site | Trend | Leftovers | By data | By blocks | By leftovers | By nothing | Held most by (pointers) |', '| --- | --- | --- | --- | --- | --- | --- | --- |'
    foreach ($s in $last.sites) {
        $h = $s.holders
        $top = ($h.top | ForEach-Object { if ($_.kind -eq 'data') { "data $($_.at) ($($_.references))" } else { "$($_.at) in $($_.function) ($($_.references))" } }) -join '; '
        $md += "| $($s.site) | $($s.trend) | $($s.blocks) | $($h.by_data) | $($h.by_blocks) | $($h.by_leftovers) | $($h.by_nothing) | $top |"
    }
}
$md | Set-Content -Encoding utf8 (Join-Path $Out 'report.md')
"report: $(Join-Path $Out 'report.md')"
if (-not $KeepGame) { Get-Process eu4 -ErrorAction SilentlyContinue | Stop-Process -Force }
