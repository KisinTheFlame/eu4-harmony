# In-game frames and their state access

The in-game frame polls input, executes allowed commands, updates interface state, prepares map markers and draws. A check inside a day may admit such a frame, so the day’s callers, shared renderer state and UI reads must be considered together. Harmony’s deferred work and presentation locking are changes around these native paths, not properties of vanilla.

## The in-game frame's steps, in order (read 2026-11-09)

The direct calls of the in-game frame (`0x8137c0`), by where they lie in it, for which of its steps read or change the interface:

1. Before the commands: the dialog machinery `0x10a03f0` (at `0x813ac5`, given the world's map manager), GUI calls `0x15f42c0` and `0x152b3f0` (twice), the map's `0x6ecae0`, the timers `0x1711d40`, `0x1711eb0`, `0x1711f80`, the date's text (`0x6e4c20`, `EU3_DATE_STRING`), the chat's check (`0x812e20`). Which of these run in every frame and which only when something waits was not read.
2. The commands (`0x20f810`, at `0x813fb5`), with the random-forbidden flag cleared around them; the daily tick among them runs the day.
3. The outliner (`0x128cd30`), input (`0x830fd0`), the time step (`0x757ad0`), the alerts (`0xdf5860`), the map's markers (`0x10a6d00`), the GUI's update (`0x14c5810`).
4. **The drawing (`0x8183b0`), last**, right after the GUI's update (`0x81591c`).

So the frame between days has the interface's steps on both sides of its commands: a drawing handed away at the frame's end would meet the next frame's first steps before that frame's day begins, unless those steps wait for it or touch nothing it draws.

The drawing's own steps, by its calls: the map mode's colors (`0x10a8c30`, at `0x818652`, when they changed), the device check (`0x14df050`), the map's off-screen passes (`0x10a51e0`), BeginScene (`[0x1fd1290]`), the 3D map (`0x10a59a0`), the sky and the rest (`0x10a5f30`, `0xde7480` four times), the GUI (`0x14dd010`, `0x14dd540`), `0x6ebd10` (the GUI manager's, `vic_18`), and from `0x818aff` the debug text, its strings `AI Budget`, `Current Money`, `GUI Objects`, `Rendered Objects`, `2D Render Time` and the rest, drawn when `[0x2340040]` is set (also written by the console's `0xe85d70`).

## Where the game lets a frame in, inside the day (read 2026-11-09)

The direct calls of `0x769ea0`, the check for a frame inside a day, by the function they lie in: 6 in the daily update (`0x758430`), 11 in the day's body (`0x75bbe0`), 14 in the monthly update (`0x75d690`), one in the provinces' phase (`0x75b2d0`) and one in the AI's (`0x75b4b0`), before the AI stage (`0x77d8a0`) and not inside it. In the countries' phase (`0x75b0d0`) it is **inside the loop over the countries**: after each country's daily update (`0x2ee280`, at `0x75b20b`), when the country's index is a multiple of 32 (`and eax, 8000001Fh`, at `0x75b212`), it checks for a frame (`0x75b22f`). So the game itself holds that its whole frame, the interface's update included, may run between two countries' daily updates. The loop walks the list at `world+0x1d08`, its count at `+0x1d14` (_inferred_: the active countries, `+0x1d00` in [architecture.md](../program/architecture.md), "The game state"); how many points that gives a late day was not counted.

## The drawing's first steps (read 2026-11-09)

`0x8183b0(idler)`, called by the in-game frame at `0x81591c` with the in-game idler, does two things that read the world before it draws:

1. **The ambient sound** (`0x818614`): when the master volume (`[settings]+0xb4`) is above a floor, `0x6e7a50(idler+0x5a0, [idler+0x11c0], [idler+0x1200])`, the idler's sound object, the graphical map and the camera; it reads the world and the map (_inferred_: the terrain under the camera, from the debug text's volumes).
2. **The map mode's colors** (`0x818619`–`0x81865c`): when either of the idler's flags `+0x34` and `+0x44` is set and the clock (`0x5bf840`) has reached `[idler+0x38]`, `0x10a8c30([app]+0x360, [idler+0x2c], [idler+0x40])`, the graphical map, the map mode and a second value, after which both flags are cleared.

Then it keeps the frame's times (`[idler+0x11f8]`, a timer made from `[0x23494a0]`, into `0x2458efc` and `0x2458f00`), deals with the camera's lock and a sound while the graphical map's `+0x530` is set (`0x8186c2`–`0x818728`), and goes on to the device check and the drawing proper. After it returns, the in-game frame calls `0xe1ac80(idler+0xb8)` and, while `[graphical map]+0x530` is set, `0x6eb6e0(idler, [idler+0x310])`.

So the drawing can be cut where the drawing proper begins: the two steps that read the world run where the frame calls the drawing, on the main thread, and the rest elsewhere.

## The camera in a frame (read 2026-11-09)

The camera is updated in the in-game frame's input (`0x830fd0`): it is offered the frame's events (`0x109c9e0`, at `0x83117b`), then updated (`0x109bee0`, at `0x8311b3`, only while the application's `+0xb8` is set; its arguments the camera `[idler+0x1200]`, `[idler+0x68]`, `[idler+0x60]`, the input's state and, on the stack, the float at `idler+0x1658`): it takes the frame's movement from the input (`0x109e750`), it glides towards its goal (`0x109f240` → `0x109f3e0`, `0x109cd80`), and is kept on the map (`0x109ec30` → `0x109d0e0`) ([camera.md](camera.md)). The main menu's frame (`0x10dc4f0`) and the nudge editor's (`0x1253d80`) call the same two. Later in the in-game frame, between the time step and the map's markers, `0x109c020` is called (at `0x815711`); what it does was not read. So the camera moves once a frame, from the events of that frame, on the main thread; nothing found moves it from the drawing.

## Where the day reaches the interface and the graphics (read 2026-11-09)

How it was read: every function of `.pdata`, its direct calls (`E8`) and its jumps into other functions, followed breadth-first from the daily update `0x758430`; each function reached is kept with the call it was reached by, so each finding below has a path from the day. Calls through pointers are not followed, so what it finds is what the day reaches at least, and a path says the code can go there, not that a day does. The day reaches 6,005 functions so. Of them:

- **48 lie among `eu4\source\graphics\`** (`0xde6000`–`0x1415000`) and are entered from code outside it: the places where the day's code goes into the interface's.
- **17 use the graphics back end's table** (`0x1fd10f8`–`0x1fd1378`): textures made (`0x1fd1320`) and destroyed (`0x1fd1330`), a render target set (`0x1fd1360`), and others.
- **16 use the GUI manager** (`[0x23494f0]`), **9 the graphics object** (`[0x23494b8]`).
- None calls `d3d9.dll` or `d3dx9_43.dll` itself: the day's Direct3D goes through the back end's table, whose functions call D3DX (as the 2026-11-08 watch found the day waiting in `d3dx9_43.dll`).

What they are, by the strings they load and the source files their asserts name:

| What the day does | Its path from the day | What it changes of the interface or the graphics |
| --- | --- | --- |
| A message, and its popup | a country's month (`0x2f36c0`) → the message handler (`0x8c07b0`, `messagehandler.cpp`) → `0x8c1430` → `0x8c1f80` | Popups by name (`DefaultPopup`, `EndOfLandCombatPopup`, `EndOfNavalCombatPopup`, `NewColonialCountryPopup`, `NewClientVassalPopup`, `NameHeirDialog`, …) through the dialog machinery (`0x10a03f0`, `0x10a06c0`: `AgreeButton`, `DeclineButton`, `popup`); the combat result (`0x8c7790`) → `0xde6300` (`3dcombatresult.cpp`), which sets a render target and makes and destroys textures; the message list's items (`0x123f800`, `MessageInterfaceListItem`) |
| A dialog to the player from a country's day | each country's daily update (`0x2ee280`) → `0xe428b0` (`Title`, `Description`) → `0x10a03f0` → … → the window class's constructor (`0x1543f80`) | A window made, and the textures it makes; the loans' dialog (`0x33d6c0` → `0x11bc630`, `EXTENDLOAN`, `REPAYLOAN`); a new client state or colonial nation (`0x481f90`, `dynamiccountries.cpp` → `0xe2baa0`, `CS_NEW_STATE_DESC`, `CustomizeColony`) |
| A dialog from the battles' step | `0x75bbe0` → `0x2485b0` (`combatmanager.cpp`) → … → `0xd429e0` | `MOVE_AUTO_TRANSPORT_DIALOG`, and exploring (`0x10c22c0`, `CONFIRM_EXPLORE_COAST`) |
| The map mode set again | a province's construction (`0xa1a010`) → the capital moved (`0x3041b0`, `CAPITALMOVED`) → the in-game idler's map mode (`0x821ff0`, its slot `+0xc8`) | The map mode's colors (`0x10a8c30`: a texture made), the map's names (`0x10a9be0`: their meshes), and others (`0x10af270`, `0x12dd0c0`) |
| A province's owner changed | the provinces' month (`0x9f35c0`) → `0xa00270` (`SetOwner`) → `0x139bff0` → `0x1347a50` | The map's markers (`strait_indicator`, `river_indicator`, `province_terrain`), through the GUI manager |
| A center of trade updated | `0xa00270` → … → `0x474d80` (`UpdateCenterOfTrade`) → `0x13238b0` | A port's model (`westerngfx`, `_trade_node_port_0`) |
| The minimap | a country's year (`0x2f7e90`) → a diplomatic action executed (`0x4e7c50` → `0x4ea820`) → `0x10b6c50` (`eu3minimap.cpp`, `map/minimap_base.bmp`) | Sprite types (`0x156e0a0`, `spritetype.cpp`) and their textures made and destroyed |
| A unit made | a native uprising (`0x75f630`) and others → the unit's constructor (`0xd3acd0`, `unit.cpp`) → `0x1448880` | `0x1094e30`, `0x1095870`, `0x1326c60` (_inferred_: the unit's model on the map) |
| A government reform | `0x2ee280` → `0x7b0670` (`government_reform_stack.cpp`) → `0x7b1e10` → `0x1323560` | `idle` (_inferred_: an animation's state) |
| A new age | the month (`0x75d690`) → `0x771000` (`new_age`) | Its window (`0xded0f0`, `new_age_objectives_entry`) |
| The frames inside a day | `0x758430` → `0x769ea0` → `0x14c25a0` | A whole frame, its drawing included |

Through pointers, and so not in the graph, the day also calls the in-game idler's daily refresh (slot `+0x2a8`, `0x8339f0`, which calls the minimap `0x10b6c50` and the date's text among 17 functions) and its monthly refresh (`+0x2b0`, `0x81e280`) ([architecture.md](../program/architecture.md), "Time"); and the commands, executed in the same frame as the day, reach the interface as the player's own actions do.

So the day does not leave the interface and the graphics alone while it runs: it makes and destroys windows, textures and models, sets the map mode and draws a combat result into a texture of its own, in the middle of its phases, from a country's, a province's or a battle's own code. A drawing on another thread meanwhile would read the GUI manager's windows, the textures and the map's scene as they are changed. Which of these places a day of a late game reaches, how often and how far into the day, was not measured.

## What the drawing reaches of the game state (read 2026-11-09)

The same graph followed from the drawing (`0x8183b0`) reaches 1,936 functions, 126 of them referring to the world (`[0x233fe78]`) and 187 to the country manager (`[0x233d8d0]`): nearly all through its first step, the map mode's colors (`0x10a8c30` → `0x12fd9b0` → `0x12f0b40`), which works each province's color out from the live game state ([map-modes.md](../provinces/map-modes.md)). Followed without that step, it reaches 950 functions:

- **The random generator** (`0x2325220`), its count and the random-forbidden flag (`0x1fd10c4`): no function refers to them.
- **The world**, 16 functions, **the country manager**, 11, **the map** (`[0x233fed8]`), 9, in these places:
  - the map's names (`0x10a9be0`: `ADJECTIVE`, `AREANAME`, `ENCLAVE_NAME_FORMAT`), made again from the 3D map's drawing (`0x10a59a0`) when they are asked for, and from the map mode's set (`0x821ff0`) in the day: they read the countries and the provinces, and make their meshes through the back end (`0xfd7140` → `0xfd58b0`);
  - the map's off-screen passes (`0x10a51e0` → `0x12dab90`, `0x12efe40`, `0x13bd3a0`), reading the world and the map;
  - the map's views (`0x13f9520`, `0x12db0c0`, `0x13c36b0`), reading the world;
  - `0x6e7a50`, called by the drawing itself, reading the world and the map (_inferred_ from its neighbor `0x6e80ee` and the debug text's `sea volume:` and `plains volume:`: the ambient sound by the terrain under the camera);
  - the debug text, from `0x818aff`: the country `[0x2340040]` names, its money, its advisors' costs (`0x1ac5e0` → `0x1ac650`), its modifiers (`0x8e8ca0`) worked out through the country's cached data (`0x93d750`, `country_cached_data.cpp`, `CCountryModifiers::EState::CALCULATING`), which works cached values out and keeps them: the one place found where the drawing writes game state, and only with the debug text on.
- **The GUI's drawing** (`0x14dd010`, through `0x14dd350`) reaches nothing by direct calls: it calls each of the frame's 2D objects through its vtable, its `+0xb8`, its `+0x98` and its `+0x58`, which draws. Of every vtable in `.rdata` (runs of pointers to functions' starts), four have a `+0x58` that reaches the world or the country manager by direct calls, and none of the four is a GUI object's: the world's own key reader (`0x751cd0`, vtable `0x1caffe8`) and three others (`0x970f60`, `0xb459d0`, `0x92e420`). So the GUI's objects draw from what they hold, as far as direct calls show; the views set it at their update ([architecture.md](../program/architecture.md), "Presentation"). This agrees with what the write watch saw the GUI's drawing read up to its first wait in the driver, mostly blocks of 0x218, 0x288 and 0x2e0 bytes, the GUI's own objects ("The drawing on a thread of its own while the next day runs").

So, the map mode's colors and the debug text set apart, what the drawing reads of the game state is the map's: its names when made again, its passes and views, and the three fields of every province the 2026-11-08 watch found. What it reads through pointers beyond the map, and the GUI's drawing past the driver's first wait, are still to be watched: the watch held the driver's thread there, which a drawing whose Direct3D calls go nowhere would not need ([drawing-thread.md](../../plans/README.md#drawing), D2).


## Where the map icons are placed (read 2026-11-10)

Shown to the owner, the units' flags and numbers stayed behind the map as it was dragged at speed 5, and caught up with it in steps. They are no models but windows of the interface: `unit_mapicon` and `navy_mapicon` of `interface/mapicons.gui`, among some thirty kinds of map icon (trade, forts, sieges, combats, builds and the others of that file).

- **The units' icons** are filled by `0x13320a0` (the strings `unit_mapicon`, `navy_mapicon`, `unit_strength`, `unit_panel_moral_bar` and the rest), whose one caller is the units' graphics' update `0x1326fd0`, under the map's markers (`0x10a6d00` → `0x10a7a90` → `0x1321880` → `0x1326fd0`), which moves the units' models too (above, "The day's crossings read again"). It works the unit's point on the map out from its place (`0x1449020`, at `0x1327c6a`) and hands it on with two values its callers pass down from the markers' update, which the in-game frame gives the map manager (`0x815816`, `r9` the in-game idler), whose camera is `[idler + 0x1200]`.
- **Every kind is placed by `0x132e300`**, called from 27 sites (all under `0x10a6d00`, via `0x10a6ec0` and `0x10a7a90` → `0x1321880`). It projects the point (its eighth argument, three f32) with `0x132e210` (rcx its ninth argument): the 4×4 matrix at `+0x80`..`+0xbf` of that object times the point, x and y over w kept as the screen's -1 to 1, and z over w returned as the depth, which it checks against a near and a far bound (its twelfth and thirteenth arguments). Then the screen's size in pixels from the graphics object (`[[0x23494b8] + 0x490]`, two i32), the window (`[holder] + 8`, popped from `0x2358400[kind]` or made from the GUI manager's `+0x48`, `0x132e4d5`, and kept in the holder) put at the point less half of its size (`0x132e553`–`0x132e5cc`, the window's slot `+0x30`), its bounds checked against the screen (hidden through `+0x48` when outside), and the window and its depth added to `map_manager+0x650` (its tenth argument). At the end of `0x10a6d00`, that list is sorted by depth (`0x10b06e0`) and every live open icon is prepended to `scene+0x110` via GUI slot `+0x108` (`0x15466d0`, at `0x10a6e95`) so map icons draw behind ordinary interface windows ([window-ordering.md](window-ordering.md#top-level-map-icons-are-a-native-backdrop-layer-read-2026-11-28)).
- **So an icon is where the camera of the last update put it.** The map and the models are drawn with the camera the drawing thread moves; the icons' windows only move when the next update places them again, which at speed 5 comes when a day ends or at the game's points inside a day.

- **The matrix is the camera's own**: the object it is in was the camera's `+0x30` in the game (the camera at `[idler + 0x1200]`), its view, which the camera's moves rebuild. So the drawing thread, which moves the camera, can project a point again with what the camera is then.

The framework keeps each placing (T3a of [drawing-thread.md](../../plans/README.md#drawing)): at `0x132e5c4`, as the window is put, the window, where it is put, the point's place on the screen and the point on the map; the drawing thread, before each frame it draws, projects the point again with the camera's view and moves the window by the difference. Checked against the game's own placing at the next update, with the camera moved between at speed 5: of 5,000 windows whose point had not moved, 99.6% were within a pixel of where the game put them and the rest within 10 ([the run](../../research/interface-evidence.md)).

What was not read: the holder's other fields (`0x1223ee0` wrappers). The other two callers of `0x132e210` (`0x132e860` for `tradegoods_mapicon` and `0x132f1c0` for reformation centers) both place their windows through `0x132e300` first and only re-project to scale/fade icons.


## A function's parts in the exception directory

The compiler splits some functions into parts, each an entry of its own in the exception directory, the later ones chained to the one before (`UNW_FLAG_CHAININFO`) and beginning where it ends: `0x10a6d00`, the map's markers' update, has its prologue to `0x10a6d11` and its body from there. 20,028 of the 79,914 entries of the 1.37.5 `eu4.exe` are such parts. The framework's count of the arguments a function reads looked at the first part alone, and found none for `0x10a6d00`, which reads four (the write watch's observer of it then forwarded none, and the game crashed at `0x10a6d16`); it now takes in the parts that follow (`analysis::bounds`).


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x94398` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x9569f` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0xa392b` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x20f877` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x21f5f0` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x23ce40` | function | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x23e620` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x277570` | function | Referenced while examining the graphics back end, the one way in (read 2026-11-11); complete role not established |
| `0x2c052b` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x2c29ae` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x2d0ae3` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x2f15a4` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x338bd0` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x36a540` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x5c3384` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x6bed3e` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x6e9f68` | code | a floating text's strings destroyed |
| `0x757ad6` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x758c2f` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x75b210` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x75c0ca` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x75c5d2` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x75c5f6` | code | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x75c5fc` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x769bd0` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x769f24` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x76a1b0` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x77f44d` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x77f8fd` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x780836` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x7b0f25` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x7b2388` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x813fba` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x8143b5` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x8143eb` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x815921` | code | the in-game frame after its drawing |
| `0x815949` | code | the tooltips updated after the drawing |
| `0x816357` | code | war enthusiasm icon set, high |
| `0x81635d` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x816383` | code | war enthusiasm icon set, low |
| `0x81639c` | code | war enthusiasm icon set, medium |
| `0x816641` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x816ab0` | leaf | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x8188f4` | code | the frame's length the 3D map is given |
| `0x81a66d` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x81aa92` | code | Referenced while examining what the map's drawing writes for the next update (watched 2026-11-09); complete role not established |
| `0x81aaa3` | code | Referenced while examining what the whole scenario set found (2026-11-09); complete role not established |
| `0x81aab0` | code | Referenced while examining what the map's drawing writes for the next update (watched 2026-11-09); complete role not established |
| `0x8208a0` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x821850` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x8218c0` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x8218dc` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x821c64` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x833b82` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x9406e6` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x943e4d` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x9f4916` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xa00598` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xa0b1f3` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0xa0e734` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xa0e748` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0xa0e755` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0xa1994a` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0xa636f5` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xacfef2` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0xb584f0` | function | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0xc52e52` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xd3aff6` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xd3b130` | function | a unit's destructor |
| `0xd3b166` | code | Referenced while examining what the whole scenario set found (2026-11-09); complete role not established |
| `0xd3b168` | code | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0xd3b17e` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xdc3453` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0xe075b0` | function | Referenced while examining a frame drawn while the ai stage is waited for; complete role not established |
| `0xe0fe50` | function | Referenced while examining a frame drawn while the ai stage is waited for; complete role not established |
| `0xfd7be2` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0xfd7c00` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x1054a20` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x109e9f6` | code | Referenced while examining the drawing thread tried (2026-11-09); complete role not established |
| `0x109eabb` | code | Referenced while examining the drawing thread tried (2026-11-09); complete role not established |
| `0x10a1180` | function | Referenced while examining what the map's drawing writes for the next update (watched 2026-11-09); complete role not established |
| `0x10a521a` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x10a5b55` | code | the map's names due |
| `0x10a5b7f` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x10a5e72` | code | Referenced while examining the game's locks, in one order (found 2026-11-11); complete role not established |
| `0x10a74c0` | function | Referenced while examining a frame drawn while the ai stage is waited for; complete role not established |
| `0x10a7f87` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x10af990` | leaf | the map told of a province's new owner |
| `0x10ba1a0` | function | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x1248900` | function | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x125b470` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x12db1e4` | code | Referenced while examining what the map's drawing writes for the next update (watched 2026-11-09); complete role not established |
| `0x12dd9c2` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x12ddb75` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x12ddba6` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x12de4f6` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x12e99b9` | code | Referenced while examining what the whole scenario set found (2026-11-09); complete role not established |
| `0x1316660` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x1348370` | leaf | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x13f968f` | code | Referenced while examining the game's locks, in one order (found 2026-11-11); complete role not established |
| `0x13f9694` | code | Referenced while examining the game's locks, in one order (found 2026-11-11); complete role not established |
| `0x14159d2` | code | the tooltip's place worked out |
| `0x14164a0` | function | offset the tooltip and keep it wholly on screen |
| `0x14486ea` | code | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x14488dd` | code | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x1449da0` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x14c267c` | code | Referenced while examining two faults a shown game found (2026-11-10); complete role not established |
| `0x14c2818` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x14c2839` | code | the application switches its idler |
| `0x14c2dd0` | code | the run's loop, the pump's quit |
| `0x14c2e18` | code | the run's loop, the quit |
| `0x14c2e27` | code | Referenced while examining two faults a shown game found (2026-11-10); complete role not established |
| `0x14c723c` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x14d9000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x14dbb0b` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x14dd4ee` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x14e4570` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x14e5f93` | code | Referenced while examining the drawing thread tried (2026-11-09); complete role not established |
| `0x14e5fbe` | code | Referenced while examining the drawing thread tried (2026-11-09); complete role not established |
| `0x14e6170` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x14f135b` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x14fd930` | function | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x1500000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x153dd00` | function | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x153e260` | function | Referenced while examining the day's crossings read again (2026-11-09); complete role not established |
| `0x153e560` | function | a mesh object's destructor |
| `0x1540000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x1540f20` | code | Referenced while examining the drawing thread tried (2026-11-09); complete role not established |
| `0x154f697` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15509fb` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x1550c33` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x156b5c0` | function | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x156b873` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x158675a` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15978d4` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x1597909` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x1597977` | code | Referenced while examining a text drawn into its texture in a day (found 2026-11-11); complete role not established |
| `0x1597991` | code | Referenced while examining a text drawn into its texture in a day (found 2026-11-11); complete role not established |
| `0x1597998` | code | Referenced while examining a text drawn into its texture in a day (found 2026-11-11); complete role not established |
| `0x15979c5` | code | Referenced while examining a text drawn into its texture in a day (found 2026-11-11); complete role not established |
| `0x1597ca6` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x1597cf7` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x159840f` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x1598453` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x159b113` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x15ae089` | code | Referenced while examining an event's window, met half made (found 2026-11-12); complete role not established |
| `0x15ba25d` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15ba630` | leaf | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15bd4ac` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15bd91b` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x15e4f00` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x15f41b9` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x1640000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x169bd80` | function | Referenced while examining a frame drawn while the ai stage is waited for; complete role not established |
| `0x169e000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x169e034` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x169e05f` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x169e6d0` | function | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x16a32d0` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x16a381d` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x16bbe5f` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x16c2810` | function | Referenced while examining the game's locks, in one order (found 2026-11-11); complete role not established |
| `0x16c48b0` | function | Referenced while examining the game's locks, in one order (found 2026-11-11); complete role not established |
| `0x16d4e2f` | code | a scene forgets the renderer's vertex declaration |
| `0x16d4fcf` | code | Referenced while examining what the whole scenario set found (2026-11-09); complete role not established |
| `0x16d50e0` | function | a function of the graphics back end's table |
| `0x16d5430` | function | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x16d66be` | code | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x16d7aa0` | function | texture creation |
| `0x16d7e70` | function | a function of the graphics back end's table |
| `0x16d8120` | function | Referenced while examining the drawing on a thread of its own while the next day runs; complete role not established |
| `0x16d8d20` | leaf | a function of the graphics back end's table |
| `0x16d9750` | function | a function of the graphics back end's table |
| `0x16da460` | function | a function of the graphics back end's table |
| `0x16f7a20` | function | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x16fb000` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x17114e2` | code | Referenced while examining the crossings a late day reaches (found 2026-11-09); complete role not established |
| `0x1712e9c` | code | Referenced while examining frames late at speed 5 (measured 2026-11-10); complete role not established |
| `0x1746aa9` | code | Referenced while examining the war overview after the drawing (found 2026-11-11); complete role not established |
| `0x17a2e60` | function | Referenced while examining the graphics back end, the one way in (read 2026-11-11); complete role not established |
| `0x17a58a0` | function | Referenced while examining the graphics back end, the one way in (read 2026-11-11); complete role not established |
| `0x1b66538` | data | the game takes a slim lock |
| `0x1fd1310` | data | Referenced while examining a text drawn into its texture in a day (found 2026-11-11); complete role not established |
| `0x1fd1338` | data | Referenced while examining what the whole scenario set found (2026-11-09); complete role not established |
