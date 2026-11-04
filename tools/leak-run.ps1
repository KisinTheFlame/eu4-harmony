<#
.SYNOPSIS
Plays rounds of sessions on a developer build, and reports what the game's
memory keeps of them (docs/leak-finding.md).

.DESCRIPTION
Needs a developer build with the memory account installed (cargo build
--release --features eu4-harmony-core/dev-memory, then dev-install.ps1), which
costs the game nothing between rounds; with the heap census
(eu4-harmony-core/dev-heap-census) the report adds the leftovers by site and
what holds them, but the census slows the game and holds hundreds of
megabytes of its own. And the game claimed and its player's files backed up:
the run loads saves and starts games, which autosave. It starts eu4.exe
unless it runs, and plays -Rounds rounds through remote control. Each round:
1. loads the first save from the main menu, and lets -Days days run;
2. loads the second save from that game, and lets a third of them run;
3. returns to the main menu;
4. starts a new game as -Country, and lets a third of the days run;
5. returns to the main menu.
After each round it asks memory, and with the census memory.leftovers, and
prints a line; after the last, memory.leftovers looks for what still points at
the leftovers too, which stops the game for seconds. At the end it writes into
-Out report.json, every answer, and report.md: the rounds; where each round's
growth went, part by part and heap by heap, as the least-squares slope over
the rounds from the second on (the first loads what stays for good, and one
round's figures jump about); and with the census the leftover sites that
leave the most per generation, and what holds them. The game is closed unless
-KeepGame. -FromReport writes report.md again from the report.json in -Out,
with no game.

With the census and -Watch N, after round -WatchAfter (3 by default) it looks
for what holds the leftovers, and watches the N steady sites that leave the
most and whose leftovers nothing, or only other leftovers, mostly points at
(memory.watch); the report then says, for each, which owners were let go
while they last pointed at its leftovers, and the code that let them go.
Watching slows every letting go of a block.

With the census and -Quarantine, the quarantine is on from the start
(memory.quarantine): what the framework's repairs let go is kept instead of
freed and filled with a pattern, and compared every two seconds. The report
says what it kept by repair, and every block written to after a repair let it
go. A read of such a block can end the game at once; the report then says in
which round, and what was found until then. The memory it keeps counts in
the game's heap, so a run with it measures no growth.

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
    [string]$GameDir,
    [switch]$KeepGame,
    [switch]$FromReport,
    [int]$Watch = 0,
    [int]$WatchAfter = 3,
    [switch]$Quarantine
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
New-Item -ItemType Directory -Force $Out | Out-Null
$mb = { param($bytes) '{0:N1}' -f ($bytes / 1MB) }
$line = { param($m) "private $(& $mb $m.private) MB, game heap $(& $mb $m.game_heap) MB, free in heaps $(& $mb $m.free_in_heaps) MB, outside heaps $(& $mb $m.outside_heaps) MB, framework $(& $mb $m.framework_heap) MB, handles $($m.handles)" }

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

if ($FromReport) {
    $report = Get-Content -Encoding utf8 -Raw (Join-Path $Out 'report.json') | ConvertFrom-Json
} else {
    if (-not $GameDir) { $GameDir = & (Join-Path $PSScriptRoot 'game-dir.ps1') }
    $cli = Join-Path $GameDir 'eu4-harmony\eu4-harmony.exe'
    if (-not (Test-Path $cli)) { throw "no command line tool at ${cli}: install the developer build with dev-install.ps1" }
    if (-not (Get-Process eu4 -ErrorAction SilentlyContinue)) {
        Start-Process -FilePath (Join-Path $GameDir 'eu4.exe') -WorkingDirectory $GameDir | Out-Null
    }
    MainMenu
    try { $null = Ask 'memory' } catch { throw "the game has no memory group: install a developer build with the memory account ($_)" }
    if (-not $Saves) { $Saves = @((Ask 'menu.saves') | Select-Object -First 2 | ForEach-Object name) }
    if ($Saves.Count -lt 2) { throw 'two saves are needed: pass -Saves, or have two in the save games folder' }
    "saves: $($Saves -join ', '); new games as $Country; $Days days"

    $played = @()
    $start = Ask 'memory'
    $census = [bool]$start.census
    "startup: $(& $line $start); the census $(if ($census) { 'is' } else { 'is not' }) built in"
    if ($Quarantine) {
        if (-not $census) { throw '-Quarantine needs the census built in' }
        $null = Ask 'memory.quarantine', '--on', 'true'
    }
    $failed = $null
    $kept = $null
    # A round that fails, the game gone among others, ends the run with what
    # the rounds before found.
    try {
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
        $leftovers = $null
        if ($census) {
            $asked = @('memory.leftovers', '--sites', "$Sites")
            if ($i -eq $Rounds) { $asked += '--holders', 'true' }
            $leftovers = Ask $asked
        }
        $watching = $null
        if ($census -and $Watch -gt 0 -and $i -eq $WatchAfter -and $i -lt $Rounds) {
            $held = Ask 'memory.leftovers', '--sites', "$Sites", '--holders', 'true'
            $lost = @($held.sites | Where-Object { $_.trend -eq 'steady' -and ($_.holders.by_nothing + $_.holders.by_leftovers) * 2 -ge $_.blocks } | Select-Object -First $Watch)
            if ($lost) {
                $watching = Ask 'memory.watch', '--sites', (($lost | ForEach-Object site) -join ',')
                "watching from round ${i}: $($watching.sites -join ', ') ($($watching.blocks) live blocks)"
            }
        }
        $quarantined = if ($Quarantine) { Ask 'memory.quarantine' } else { $null }
        $pages = if ($census -and $i -eq $Rounds) { Ask 'memory.pages', '--sites', "$Sites" } else { $null }
        $round = [ordered]@{ round = $i; seconds = [int]((Get-Date) - $began).TotalSeconds; memory = $memory; leftovers = $leftovers; watching = $watching; quarantine = $quarantined; pages = $pages }
        $played += $round
        $grew = if ($i -gt 1) { ' ({0:+0.0;-0.0;0.0} MB)' -f (($memory.private - $played[-2].memory.private) / 1MB) } else { '' }
        $held = if ($quarantined) { "; quarantine $($quarantined.blocks) blocks, $(& $mb $quarantined.bytes) MB, $(@($quarantined.written).Count) written to" } else { '' }
        "round $i ($($round.seconds) s)${grew}: $(& $line $memory)$held"
    }
    if ($Quarantine) { $kept = Ask 'memory.quarantine', '--on', 'false' }
    } catch {
        $failed = "round $($played.Count + 1): $_"
        "stopped in $failed"
    }
    $report = [ordered]@{ saves = $Saves; country = $Country; days = $Days; startup = $start; rounds = $played; failed = $failed; quarantine = $kept }
    $report | ConvertTo-Json -Depth 12 | Set-Content -Encoding utf8 (Join-Path $Out 'report.json')
    if (-not $KeepGame) { Get-Process eu4 -ErrorAction SilentlyContinue | Stop-Process -Force }
}
$Saves = @($report.saves); $Country = $report.country; $Days = $report.days; $start = $report.startup; $played = @($report.rounds)
# What the quarantine said last: as it was turned off, or after the last
# round played when the run stopped.
$kept = $report.quarantine
if (-not $kept -and $played) { $kept = $played[-1].quarantine }
$census = [bool]$start.census

$md = @("# Leak run, $(Get-Date -Format 'yyyy-MM-dd HH:mm')", '', "Saves $($Saves -join ' and '), new games as $Country, $Days days; $($played.Count) rounds.", '')
if ($report.failed) { $md += "**The run stopped** in $($report.failed)", '' }
$md += '| Round | Seconds | Private MB | Change | Game heap MB | Free in heaps MB | Outside heaps MB | Framework MB | Handles |', '| --- | --- | --- | --- | --- | --- | --- | --- | --- |'
$before = $start.private
foreach ($r in $played) {
    $m = $r.memory
    $md += "| $($r.round) | $($r.seconds) | $(& $mb $m.private) | $('{0:+0.0;-0.0;0.0}' -f (($m.private - $before) / 1MB)) | $(& $mb $m.game_heap) | $(& $mb $m.free_in_heaps) | $(& $mb $m.outside_heaps) | $(& $mb $m.framework_heap) | $($m.handles) |"
    $before = $m.private
}

if ($played.Count -ge 2) {
    # The least-squares slope per round, over the rounds from the second on
    # (or both, when there are two), of what $of reads of a round's memory.
    $from = if ($played.Count -ge 3) { 1 } else { 0 }
    $slope = {
        param([scriptblock]$of)
        $x = @($played[$from..($played.Count - 1)] | ForEach-Object { [double]$_.round })
        $y = @($played[$from..($played.Count - 1)] | ForEach-Object { [double](& $of $_.memory) })
        $mx = ($x | Measure-Object -Average).Average
        $my = ($y | Measure-Object -Average).Average
        $sxy = 0.0; $sxx = 0.0
        for ($k = 0; $k -lt $x.Count; $k++) { $sxy += ($x[$k] - $mx) * ($y[$k] - $my); $sxx += ($x[$k] - $mx) * ($x[$k] - $mx) }
        $sxy / $sxx
    }
    $per = { param([scriptblock]$of) '{0:+0.00;-0.00;0.00}' -f ((& $slope $of) / 1MB) }
    $md += '', "## Where each round's growth goes", '', "The least-squares slope over rounds $($from + 1) to $($played.Count).", ''
    $md += '| Part | MB a round |', '| --- | --- |'
    $md += "| Private bytes | $(& $per { $args[0].private }) |"
    $md += "| The game's heap, what the game holds | $(& $per { $args[0].game_heap }) |"
    $md += "| The other heaps, what they hold | $(& $per { $args[0].heaps_allocated - $args[0].game_heap - $args[0].framework_heap }) |"
    $md += "| The heaps, committed and unused | $(& $per { $args[0].free_in_heaps }) |"
    $md += "| Outside the heaps (Direct3D, the driver, whole pages) | $(& $per { $args[0].outside_heaps }) |"
    $md += "| The framework's heap | $(& $per { $args[0].framework_heap }) |"
    $md += "| Handles | $('{0:+0.0;-0.0;0.0}' -f (& $slope { $args[0].handles })) |"
    $md += '', '### Each heap', '', 'At the end of the last round, in MB, and the slope as above.', ''
    $md += "| Heap | Game's | Default | Allocated | Allocated a round | Committed | Committed a round |", '| --- | --- | --- | --- | --- | --- | --- |'
    foreach ($h in $played[-1].memory.heaps) {
        $handle = $h.handle
        $allocated = { ($args[0].heaps | Where-Object handle -eq $handle | Select-Object -First 1).allocated }.GetNewClosure()
        $committed = { ($args[0].heaps | Where-Object handle -eq $handle | Select-Object -First 1).committed }.GetNewClosure()
        $md += "| $handle | $(if ($h.game) { 'yes' }) | $(if ($h.process) { 'yes' }) | $(& $mb $h.allocated) | $(& $per $allocated) | $(& $mb $h.committed) | $(& $per $committed) |"
    }
}

if ($census -and $played) {
    $last = $played[-1].leftovers
    $md += '', "## Leftovers at generation $($last.generation)", '', "$($last.blocks) blocks, $(& $mb $last.bytes) MB in all.", ''
    $md += '| Site | Function | Trend | Per generation KB | Leftover KB | Blocks | Let go at (blocks) |', '| --- | --- | --- | --- | --- | --- | --- |'
    foreach ($s in $last.sites) {
        $freed = ($s.freed_at | ForEach-Object { "$($_.site) in $($_.function) ($($_.blocks))" }) -join '; '
        $md += "| $($s.site) | $($s.function) | $($s.trend) | $('{0:N0}' -f ($s.per_generation / 1KB)) | $('{0:N0}' -f ($s.bytes / 1KB)) | $($s.blocks) | $freed |"
    }
    if ($last.sites -and $last.sites[0].holders) {
        $md += '', '## What holds them', '', "Each leftover counted once, by the best that points at it: the game's data, other live blocks, only the leftovers listed, or nothing.", ''
        $md += '| Site | Trend | Leftovers | By data | By blocks | By leftovers | By nothing | Held most by (pointers) |', '| --- | --- | --- | --- | --- | --- | --- | --- |'
        foreach ($s in $last.sites) {
            $h = $s.holders
            $top = ($h.top | ForEach-Object { if ($_.kind -eq 'data') { "data $($_.at) ($($_.references))" } else { "$($_.at) in $($_.function) ($($_.references))" } }) -join '; '
            $md += "| $($s.site) | $($s.trend) | $($s.blocks) | $($h.by_data) | $($h.by_blocks) | $($h.by_leftovers) | $($h.by_nothing) | $top |"
        }
        $watched = @($last.sites | Where-Object { $_.holders.watched })
        if ($watched) {
            $since = ($played | Where-Object { $_.watching } | Select-Object -First 1).round
            $md += '', '## Who let them go', '', "Watched from round $since on. For the leftovers of each watched site that nothing points at: the owners let go while they last pointed at them, and the code that let them go. Unrecorded: lost before watching began, or the pointer was written over.", ''
            $md += '| Site | Nothing points at | Owner (site in function) | Let go at (in function) | Leftovers |', '| --- | --- | --- | --- | --- |'
            foreach ($s in $watched) {
                $h = $s.holders
                foreach ($l in $h.lost_with) {
                    $md += "| $($s.site) | $($h.by_nothing) | $($l.owner) in $($l.owner_function) | $($l.let_go_at) in $($l.let_go_function) | $($l.blocks) |"
                }
                $md += "| $($s.site) | $($h.by_nothing) | unrecorded | | $($h.unrecorded) |"
            }
        }
    }
}
$pages = if ($played) { $played[-1].pages } else { $null }
if ($pages) {
    $md += '', '## What keeps the free pages', '', "The game's heap after the last round, page by page: a page blocks are on stays committed however little of it they use. Walked in $($pages.took_ms) ms$(if (-not $pages.complete) { ', not to its end' })."
    $md += '', '| Pages | MB |', '| --- | --- |'
    $md += "| Committed | $(& $mb $pages.committed) |", "| Used by blocks | $(& $mb $pages.used) |", "| Pages no block is on | $(& $mb $pages.free_pages) |"
    $md += "| Free on the pages blocks are on | $(& $mb $pages.free_on_used_pages) |", "| of it, kept by leftovers | $(& $mb $pages.by_leftovers) |"
    $md += '', "The pages blocks are on, by how full: $((0..($pages.by_use.Count - 1) | ForEach-Object { "up to $($_ + 1)/8: $($pages.by_use[$_])" }) -join ', ').", ''
    $md += '| Site | Function | Blocks | KB | Free KB it keeps | Leftovers |', '| --- | --- | --- | --- | --- | --- |'
    foreach ($s in $pages.sites) { $md += "| $($s.site) | $($s.function) | $($s.blocks) | $('{0:N0}' -f ($s.bytes / 1KB)) | $('{0:N0}' -f ($s.pinned / 1KB)) | $($s.leftovers) |" }
}
if ($kept) {
    $md += '', '## What the quarantine kept', '', "What the framework's repairs let go, kept instead of freed and filled with a pattern; compared $($kept.checks) times. $($kept.freed) blocks were freed again to keep it under its most.", ''
    $md += '| Repair | Blocks | MB |', '| --- | --- | --- |'
    foreach ($r in $kept.repairs) { $md += "| $($r.repair) | $($r.blocks) | $(& $mb $r.bytes) |" }
    $written = @($kept.written)
    if ($written) {
        $md += '', '### Written to after a repair let it go', '', "The first write to each block, as found; the generations are the census's.", ''
        $md += '| Repair | Site in function | Size | Offset | Bytes from it | Kept at | Found at |', '| --- | --- | --- | --- | --- | --- | --- |'
        foreach ($w in $written) { $md += "| $($w.repair) | $($w.site) in $($w.function) | $($w.size) | $($w.offset) | $($w.bytes) | $($w.generation) | $($w.found) |" }
    } else {
        $md += '', 'Nothing was written to after a repair let it go.'
    }
}
$md | Set-Content -Encoding utf8 (Join-Path $Out 'report.md')
"report: $(Join-Path $Out 'report.md')"
