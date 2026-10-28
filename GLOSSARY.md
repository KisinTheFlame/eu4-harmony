# EU4 Harmony

A framework that lets developers extend Europa Universalis IV with native code, and the mods it loads.

## Language

### Mods and the framework

**Mod**:
What a player subscribes to and enables in the Paradox launcher, that is, a Paradox mod. It may hold only game scripts and assets, or carry an extension as well.
_Avoid_: mod pack

**Extension**:
The native code part of a mod that the framework loads and runs inside the game process, declared by the mod's manifest. A mod has at most one.
_Avoid_: plugin, module, DLL mod

**Manifest**:
The file in a mod that declares its extension to the framework.
_Avoid_: descriptor (that is Paradox's `descriptor.mod`)

**Playset**:
The mods enabled in the Paradox launcher for the next time the game starts.
_Avoid_: load list, mod list

**Framework (EU4 Harmony)**:
The program that runs inside the game process, finds and loads extensions, and gives them capabilities.
_Avoid_: Harmony on its own (easily confused with other projects of that name), loader, engine

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

### Capabilities

**Capability**:
One thing the framework lets an extension or a remote client read or change in the game, such as reading an army, ordering a move, adding a button to a panel, or keeping data per save. Extensions and remote clients reach the game only through capabilities.
_Avoid_: API, service, hook

**Safe point**:
A moment on the game's main thread when game state can be read and changed safely. The framework calls extensions only at safe points.
_Avoid_: tick

**Snapshot**:
All of a game object's fields at one moment, fetched by an extension in one go.
_Avoid_: view, reference

**Extension save data**:
An extension's own data kept with a particular save: tied to that save, but stored outside the save file.
_Avoid_: sidecar, companion file

**Game console**:
The game's own console, where commands such as `add_treasury` are typed. The framework offers running a command in it as a capability.
_Avoid_: console on its own

**Map position**:
A point on the game's map, in pixels of the province map (`map/provinces.bmp`): x from its left edge, y from its bottom edge, as `map/positions.txt` writes them. The camera looks at a map position.
_Avoid_: coordinates on their own; screen position (a pixel of the game's window)

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
_Avoid_: notification, message; game event (one of the game's own events, whose options the player chooses from)

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

**Notice window**:
The window in the framework UI that shows notices to the player. To the player it is a window, but it is not a window in the game's sense. When the framework fails to start, a Windows message box is used instead.
_Avoid_: dialog; message box (that is the Windows message box used when startup fails)

**Settings window**:
The window in the framework UI where the player changes framework settings, opened from the framework button in the main menu's corner. Like the notice window, it is not a window in the game's sense.
_Avoid_: options menu (that is the game's own settings)

**Framework settings**:
The player's preferences for the framework, such as which fonts the Unicode module uses; they take effect at the next start.
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
A font installed in Windows, such as Microsoft YaHei.
_Avoid_: external font, TrueType font

**Legacy escape**:
The escape byte sequences the CJK font plugin uses to stand for Chinese characters in game text, found in Chinese language mods made for it and in saves played with it.
_Avoid_: double-byte encoding, mojibake
