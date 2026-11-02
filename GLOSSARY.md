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

**Lua state**:
The Lua interpreter the framework gives one extension. Extensions share nothing.
_Avoid_: VM, interpreter on its own, sandbox

**Sandbox**:
What an extension's Lua state can reach: Lua's own libraries for strings, tables, numbers and coroutines, the framework's globals and capabilities, and nothing of the process or the system.
_Avoid_: jail, safe mode

**Handler**:
A function an extension gives the framework for one event, which the framework calls at a safe point.
_Avoid_: callback, listener, hook

**Time limit**:
The longest one call of a handler may run before the framework stops it. An extension whose calls are stopped several times in a row is stopped for the rest of the game.
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

**Installation**:
The one-off step of copying the framework's files into the game folder; uninstalling is deleting them.
_Avoid_: using it for the hooking done at startup

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
The game's own console, where commands such as `cash` are typed. The framework offers running a command in it as a capability.
_Avoid_: console on its own

**Map position**:
A point on the game's map, in pixels of the province map (`map/provinces.bmp`): x from its left edge, y from its bottom edge, as `map/positions.txt` writes them. The camera looks at a map position.
_Avoid_: coordinates on their own; screen position (a pixel of the game's window)

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

**CJK font plugin**:
The third-party plugin `plugin64.dll` that lets the game draw Chinese characters, commonly called the double-byte patch. The Unicode module replaces it.
_Avoid_: Chinese localisation (that means the text of a Chinese language mod)

### Text

**Unicode module**:
The part built into the framework that makes the game keep, show and take input of text as UTF-8, replacing the CJK font plugin. It belongs to the framework itself and is not an extension.
_Avoid_: text engine, font plugin, Chinese patch

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
The game's interface text in its smaller fonts, named in its `.gfx` files `vic_18`, `Main_14`, `ToolTip_Font` and the like, whose lines are at most 18 GUI units tall in vanilla: what windows hold, tooltips, lists and numbers. Body text is drawn in one system font and weight, which the framework UI uses too.
_Avoid_: text font, normal text, small text

**Headings**:
The game's interface text in its larger fonts, named `vic_22`, `vic_29`, `vic_36`, `Main_24` and the like, whose lines are 20 to 36 GUI units tall in vanilla: window titles, section headings, the large titles of dialogs. Headings are drawn in a system font and weight of their own. A Chinese language mod that gives these fonts other files keeps their names, so the names, not the lines, say which text is which.
_Avoid_: titles (a ruler's title is a game concept), large text

**Legacy escape**:
The escape byte sequences the CJK font plugin uses to stand for Chinese characters in game text, found in Chinese language mods made for it and in saves played with it.
_Avoid_: double-byte encoding, mojibake
