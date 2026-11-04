# EU4 Harmony

A framework that lets developers extend Europa Universalis IV with Lua scripts, and the mods it loads.

## Language

### Mods and the framework

**Mod**:
What a player subscribes to and enables in the Paradox launcher, that is, a Paradox mod. It may hold only game scripts and assets, or carry an extension as well.
_Avoid_: mod pack

**Harmony mod**:
A mod with a manifest, whether or not it carries an extension. Where players see it, it is an EU4 Harmony mod.
_Avoid_: framework mod, extension mod

**Extension**:
A mod's Lua scripts, which the framework runs inside the game process in a Lua state of their own. A mod has at most one.
_Avoid_: plugin, module, script mod, DLL mod

**Extension reload**:
Replacing an extension's code and texts with what its files now hold, in the running game, its state carried across through its save data. Only the extension changes; the session goes on.
_Avoid_: reload alone (the game's own `reload` reads a `.gui` file again), hot swap, restart

**Lua state**:
The Lua interpreter the framework gives one extension. Extensions share nothing.
_Avoid_: VM, interpreter on its own, sandbox

**Sandbox**:
What an extension's Lua state can reach: Lua's own libraries for strings, tables, numbers and coroutines, the framework's globals and capabilities, and nothing of the process or the system.
_Avoid_: jail, safe mode

**Handler**:
A function an extension gives the framework for one event, which the framework calls at a safe point; and `save_data`, the framework's question for the extension's data.
_Avoid_: callback, listener, hook

**Task**:
One run of a handler, or of a function an extension starts on its own, that may wait for something, such as an answer from the network, and is taken up again at a later safe point.
_Avoid_: coroutine (that is how it is built), promise, job, thread

**Time limit**:
The longest a task may run without waiting before the framework stops it. An extension whose calls are stopped several times in a row is stopped for the rest of the game.
_Avoid_: budget, watchdog

**Data folder**:
An extension's own folder in the user folder, where it keeps files it writes while running, apart from any save.
_Avoid_: data directory, storage, sandbox folder

**Manifest**:
The file in a mod that declares its extension to the framework.
_Avoid_: descriptor (that is Paradox's `descriptor.mod`)

**Playset**:
The mods enabled in the Paradox launcher for the next time the game starts.
_Avoid_: load list, mod list

**Framework (EU4 Harmony)**:
The program that runs inside the game process, finds and loads extensions, and gives them capabilities.
_Avoid_: Harmony on its own where players or other projects see the name (other projects share it; there it is EU4 Harmony or `eu4-harmony`), loader, engine

**Framework files**:
Files of the framework's own that the game reads as it reads its own and the mods', such as the framework fonts' files. The framework makes them in memory and mounts them into the game's file system; they are never on disk (ADR framework-files-in-the-game-file-system).
_Avoid_: virtual files, framework mod, assets

**Installation**:
The one-off step of copying the framework's files into the game folder; uninstalling is deleting them.
_Avoid_: using it for the hooking done at startup

**Developer build**:
A build of the framework with development tools built in, such as the call trace or the command log, for trying things in the game; it says so at the top of its startup report. Players get only the player build, which has none of them and no hidden switches.
_Avoid_: trace build, debug build (a build without optimizations), test build

**Heap census**:
A development tool that counts every live block of the game's heap by its site, the code that allocated it, and its generation, so that what the game keeps of its sessions can be found (docs/leak-finding.md).
_Avoid_: memory profiler, allocation tracker

**Generation**:
The heap census's span of time: one ends at every world reset, and when remote control asks for one to end.

**Leftover**:
A block of the game's heap still live two generations after its own: it outlived two resets of the world it was made in.
_Avoid_: leak (for one block; a leak is what a site's leftovers add up to, session after session)

**Leak run**:
Rounds of sessions that `tools/leak-run.ps1` plays on a developer build with the heap census, and the report it writes of what the game's memory kept.

**Quarantine**:
A developer build's keeping of what the framework's repairs let go, filled with a pattern instead of freed, so that the game writing to it or reading it shows the repair let go memory still in use (docs/leak-finding.md).
_Avoid_: guard, sandbox

**Startup**:
What the framework does inside the game process each time the game starts: identify the baseline, hook, load extensions.
_Avoid_: installation

**Baseline**:
One particular `eu4.exe` build the framework supports, recognized by where its original sections lie; an exe with that layout is taken as the baseline only when every dependency point holds its bytes. The exe is not hashed.
_Avoid_: version, base exe

**Dependency point**:
One place in the baseline's code or data that the framework relies on: a location, the bytes that should be there, and what the framework changes there (possibly nothing).
_Avoid_: patch point, hook point

**Repair**:
A defect of the game's own that the framework puts right where the game goes wrong, so that it holds in vanilla's paths too: memory a world reset leaks, what it leaves of a session, what the map manager leaves of the session's interface. All of them are in one module of the core.
_Avoid_: fix, patch, workaround

### Capabilities

**Capability**:
One thing the framework lets an extension or a remote client read or change in the game, such as reading an army, ordering a move, adding a button to a panel, or keeping data per save. Extensions and remote clients reach the game only through capabilities, the same ones, named and shaped after what a player sees and does rather than after how the game keeps it.
_Avoid_: API, service, hook

**Capability group**:
The capabilities about one kind of thing, such as armies, provinces or time; the same groups in Lua and in remote control.
_Avoid_: namespace, module

**Capability catalogue**:
The one description of every capability: its group, name, arguments, result and failures, and whether extensions, remote clients or both have it.
_Avoid_: API reference, schema

**Sensitive capability**:
A capability that reaches beyond the game, such as the real time, files or the network. An extension can use one only when its manifest declares it as a permission.
_Avoid_: privilege, dangerous capability

**Permission**:
A sensitive capability as a mod's manifest declares it. In this version the player sees a mod's permissions in the mods window; confirming them is to come.
_Avoid_: grant, right, access

**Frame inside a day**:
A whole frame the game draws while a day is still being worked out, once the day has taken longer than an interval since the last one: 50 ms at the fastest speed. It executes no commands and draws no random numbers.
_Avoid_: inserted frame, nested frame

**Safe point**:
A moment on the game's main thread when game state can be read and changed safely. The framework calls extensions only at safe points.
_Avoid_: tick

**Snapshot**:
All of a game object's fields at one moment, fetched by an extension in one go.
_Avoid_: view, reference

**Extension save data**:
An extension's own data kept with a particular save, written into the save file itself. It is kept only for the extensions loaded when the save is loaded.
_Avoid_: sidecar, companion file, store

**Game console**:
The game's own console, where console commands such as `cash` are typed. The framework offers running a console command in it as a capability. Its window draws in a framework font, in a system font and weight of its own.
_Avoid_: console on its own

**Console command**:
What is typed into the game console, such as `cash 100`. Not a command: the game console changes the game itself.
_Avoid_: cheat, command on its own

**Command**:
One of the game's own instructions that changes the game, such as recruiting a regiment, proposing an alliance or choosing an event's option, which the game checks and carries out. The player's orders, the AI's choices and the game's own daily update all reach the game as commands. Each is of a type, named by the game, and has a written form that says everything it carries.
_Avoid_: order (what the player gives, which the game makes into a command), action, console command

**Map position**:
A point on the game's map, in pixels of the province map (`map/provinces.bmp`): x from its left edge, y from its bottom edge, as `map/positions.txt` writes them. The camera looks at a map position.
_Avoid_: coordinates on their own; screen position (a pixel of the frame the game draws)

**Screen position**:
A pixel of the frame the game draws, as `screenshot` takes it: x from its left edge, y from its top edge. Input is given at screen positions.
_Avoid_: coordinates on their own; map position (a point on the game's map)

**Question**:
One of the game's own events that waits for the player's answer, shown in an event window with the options the player may choose. Answering it is choosing one of those options; one left unanswered for a few months is answered by the game itself.
_Avoid_: event (that is remote control's numbered events), game event, popup, decision (the game's decisions are something the player takes, not answers)

**Offer**:
A proposal another country made to the player that waits for the player to accept or decline it, such as an alliance or a royal marriage, shown as a diplomatic message icon or a dialog.
_Avoid_: request, diplomatic request, diplomatic message, question (that is one of the game's events)

### Sessions

**Session**:
One stretch of play of one game, from when a save is loaded or a new game is started until it is left: by returning to the main menu, by an in-game load, or by quitting. Sessions are numbered.
_Avoid_: campaign, game on its own

**Restart**:
The game ending its own process and starting `eu4.exe` again, as vanilla does on returning to the main menu.
_Avoid_: relaunch, reboot

**Return to the main menu**:
Leaving a session for the main menu, by the game's own question, the end-of-game summary or remote control. It says where the game goes, not how: by a restart or by an in-process return.
_Avoid_: exit, quit (that is leaving the game for the desktop)

**In-process return**:
A return to the main menu without a restart: the process goes on with the game's interface, graphics and scripts still loaded, and only the session is left behind.
_Avoid_: fast return, soft return

**World reset**:
The game putting its world back as a new start has it, without loading its content again: before every in-game load without the game's history, which the save that follows makes up for, and with the history before a new game. It leaves some of a session behind, and some of its memory, which the framework's repairs put right.
_Avoid_: reload, rebuild

**In-game load**:
Loading a save while a session goes on, which ends that session and begins another without the main menu in between. Vanilla offers it only outside ironman.
_Avoid_: switching saves, reload

**Ironman exit save**:
The save the game writes of an ironman session as it is left, from which that ironman game goes on. Leaving without it goes back to the last save.
_Avoid_: autosave (the game's periodic save), checkpoint

**Game checksum**:
The checksum of the game's content files that the main menu shows. A mod that changes those files changes it; vanilla gives no achievements while it is not vanilla's.
_Avoid_: checksum on its own, data checksum

**Save checksum**:
The checksum at the end of a save, over what comes before it. A save whose save checksum does not match counts as modified, and vanilla gives no achievements in a game that loaded one.
_Avoid_: checksum on its own

### Remote control

**Remote control**:
The part of the framework that lets programs outside the game process use capabilities. It acts as the player: its orders count as orders the player gave by hand.
_Avoid_: API, server, RPC, console (that is the game console)

**Remote client**:
A program outside the game process that uses remote control, such as `eu4-harmony game` or an AI driving it.
_Avoid_: bot, agent

**Transport**:
How requests and responses travel between remote clients and remote control, such as the named pipe carrying JSON lines.
_Avoid_: protocol adapter, channel, connection

**Input**:
Mouse and keyboard input a remote client gives the game as the player's own: clicks, the wheel, keys and typed text at screen positions. The game takes it as it takes the player's.
_Avoid_: simulated, synthetic or fake input; SendInput (that is how a program outside the game sends input to whatever is in front)

**Event**:
Something extensions hear about at a safe point, such as a new day or the player giving an army an order. Remote control keeps the latest ones, numbered, for remote clients to read.
_Avoid_: notification, message; game event, question (one of the game's own events, whose options the player chooses from)

### Interface

**Window**:
A unit of the game's interface defined by a `windowType` in a `.gui` file, holding elements such as buttons and text boxes; the single army panel and the top bar are examples. The game builds an instance from the definition when it needs to show one, and may later destroy and rebuild it.
_Avoid_: meaning the Windows window (the whole game has only one), dialog

**Host window**:
A vanilla window that a mod or the framework adds its own elements to.
_Avoid_: target window, injection target, parent window

**Binding**:
The link between the framework and the current instance of a host window; through it the framework controls whether added elements show and what they say. It is undone when the instance is destroyed and made again when the window is rebuilt.
_Avoid_: hook (that means intercepting a game function)

**Button area**:
A region the framework reserves on a vanilla panel, where the extensions' buttons are laid out automatically.
_Avoid_: toolbar, slot

**Framework UI**:
The interface the framework draws on top of the game's picture, without using the game's interface system.
_Avoid_: overlay (that is how it is drawn), HUD

**Widget**:
A building block of the framework UI, such as a button or a drop-down list.
_Avoid_: element (that means a button, text box and so on in the game's `.gui`)

**Toggle**:
A widget that is either on or off, switched by a click.
_Avoid_: checkbox, switch, radio button (a different widget: one choice among several)

**Choice bar**:
A widget of a few named options side by side in one bar; the chosen one sits under a highlight, which slides to another when it is clicked.
_Avoid_: slider (that sets a number on a scale), radio buttons, segmented control

**Info mark**:
A small i in a circle after a label in the framework UI; while the pointer is on it, a short explanation shows beside it.
_Avoid_: help icon, tooltip on its own (any text that shows under the pointer)

**Side list**:
A list down a window's left side in the framework UI, one item of which is chosen with a click; the rest of the window shows the chosen one.
_Avoid_: sidebar, tabs, master-detail

**Notice window**:
The window in the framework UI that shows notices to the player. To the player it is a window, but it is not a window in the game's sense. When the framework fails to start, a Windows message box is used instead.
_Avoid_: dialog; message box (that is the Windows message box used when startup fails)

**Settings window**:
The window in the framework UI where the player changes framework settings, opened from the framework button in the main menu's corner. Like the notice window, it is not a window in the game's sense.
_Avoid_: options menu (that is the game's own settings)

**Mods window**:
The window in the framework UI, opened from the main menu's corner, that lists the Harmony mods of this start, each with its name, version, description and the sensitive capabilities it declares.
_Avoid_: mod list (that is a playset), mod manager (it manages nothing)

**Framework settings**:
The player's preferences for the framework, such as which fonts the Unicode module uses. Some take effect when they are saved, others, such as the fonts, at the next start.
_Avoid_: configuration; game settings (the game's own `settings.txt`)

### Localisation

**Player language**:
The language the framework's and every mod's texts use in this session, decided once by the framework at startup.
_Avoid_: game language (the language in the game's settings, which for Chinese players is usually English)

**Localisation file**:
A file of texts in one language that a mod provides under `eu4-harmony/localisation/`. It keeps the game's format but holds plain UTF-8; the framework hands it to the game.
_Avoid_: translation file, language pack

### Text

**Unicode module**:
The part built into the framework that makes the game keep, show and take input of text as UTF-8. It belongs to the framework itself and is not an extension.
_Avoid_: text engine

**Game text**:
Text as the game keeps it once the Unicode module runs: a character Windows-1252 has is its single 1252 byte, as the game always kept it; any other character is its UTF-8 sequence.
_Avoid_: UTF-8 text (formatting codes and European letters are still 1252 bytes), double-byte text

**Bitmap font**:
A font shipped with the game or a mod that stores its glyphs as an image (a `.fnt` character table plus a texture).
_Avoid_: vanilla font, pixel font

**System font**:
A font family installed in Windows, such as Microsoft YaHei, with all its weights.
_Avoid_: external font, TrueType font

**Weight**:
How heavy a system font's strokes are, such as Light, Regular or Bold, numbered from 100 (Thin) to 900 (Black) with Regular at 400. A system font comes only in the weights its maker made.
_Avoid_: boldness, thickness, style (that is upright or italic)

**Body text**:
The game's interface text in its smaller fonts, named in its `.gfx` files `vic_18`, `Main_14`, `ToolTip_Font` and the like, whose lines are at most 18 GUI units tall in vanilla: what windows hold, tooltips, lists and numbers, but not the game console's window. Body text is drawn in one system font and weight, which the framework UI uses too.
_Avoid_: text font, normal text, small text

**Headings**:
The game's interface text in its larger fonts, named `vic_22`, `vic_29`, `vic_36`, `Main_24` and the like, whose lines are 20 to 36 GUI units tall in vanilla: window titles, section headings, the large titles of dialogs. Headings are drawn in a system font and weight of their own. A Chinese language mod that gives these fonts other files keeps their names, so the names, not the lines, say which text is which.
_Avoid_: titles (a ruler's title is a game concept), large text

**Framework font**:
A font the framework defines among its framework files, for text of the game's that draws in a system font of its own, such as the game console's window. The game's windows that draw in it are given it as their files are read.
_Avoid_: custom font, own font, console font on its own

**Legacy escape**:
The escape byte sequences older Chinese language mods and saves use to stand for Chinese characters in the game's single-byte text.
_Avoid_: double-byte encoding, mojibake
