# Presentation


**The graphics back end** (`pdx_gfx`).

- **The function table.** The back end is not a class: it is a table of 81 function pointers at `0x1fd10f8`–`0x1fd1378`. Its contents in `.data` are the DX9 functions. `0x16c1480` rewrites every slot when the command line names a renderer, and only `-opengl` gives OpenGL.
- **The device.** It is made with `Direct3DCreate9Ex` and `CreateDeviceEx` (`0x16d8ed0`), or plain Direct3D 9.
- **The interface over it** is in the style of Direct3D 10 (inferred from what its slots set): blend, depth and rasterizer state objects, input layouts, and shader constants kept in memory.
- **Shaders.** They are the `gfx\FX\*.shader` files, built through HLSL (`0x16fa460`, compiled with `D3DXCompileShader` and cached in `shadercache\dx9\`) or GLSL.
- **Present** is the slot at `0x1fd1300`, `0x16d4db0`.

| Slot | DX9 function | What |
| --- | --- | --- |
| `0x1fd12d8` | `0x16d8ed0` | Make the device |
| `0x1fd12e0` | `0x16d8e20` | Is the device lost |
| `0x1fd1308`, `0x1fd12f8`, `0x1fd12f0` | `0x16d4bb0`, `0x16d4a40`, `0x16d4b20` | Before, during and after a device reset |
| `0x1fd1290`, `0x1fd1298` | `0x16d4e00`, `0x16d4e50` | BeginScene, EndScene |
| `0x1fd1168`, `0x1fd1208` | `0x16d4f00`, `0x16d4e90` | Draw indexed, draw |
| `0x1fd11f0` | `0x16d8560` | Bind a texture ([font-textures.md](font-textures.md)) |
| `0x1fd1320`, `0x1fd1330` | `0x16d7de0`, `0x16d80f0` | Make and destroy a texture |
| `0x1fd1360` | `0x16d8ca0` | Set the render target |
| `0x1fd1300` | `0x16d4db0` | Present |

**The graphics object** `[0x23494b8]` (`graphics.cpp`, made by `0x14d9a20`).

- It holds:
  - the renderer `+0x88`;
  - the sprite and font types `+0xe8`;
  - the 2D scene;
  - the texture manager `+0x480`.
- Every frame begins with its device check `0x14df050`. A lost device skips the frame. A device that needs a reset is reset between invalidating (`0x14ddc70`) and restoring (`0x14ddd50`) every sprite type and listener.

**The in-game frame.** `0x8137c0` runs:

1. input (`0x830fd0`);
2. the commands and time (above);
3. the engine's services and the GUI's update (`0x14c5810`);
4. drawing (`0x8183b0`), in this order:
   1. the map mode's colors, when they changed;
   2. the device check;
   3. the map's off-screen passes;
   4. BeginScene and Clear;
   5. **the 3D map** (`0x10a59a0`): the map's layers, the map names and the 3D scene, once for each copy of the camera where the map wraps;
   6. the sky and post-processing (inferred: bloom);
   7. **the GUI** (`0x14dd010`, `0x14dd540`);
   8. debug and status text;
   9. EndScene;
   10. Present.

The main menu's frame draws the same way, without units and overlays. The mouse cursor is a Windows cursor (`gfx\cursors\`, `SetCursor`; inferred: it is not drawn into the frame). The framework UI is drawn in the Present hook, after the game's EndScene ([safe-points.md](../day/safe-points.md)).

**The map.** The graphical map (`eu3graphicalmap.cpp`) is the application's `+0x360` (the in-game frame reads `[[idler+0x330]+0x360]`), so the main menu shows the same map behind its pages. Its renderer, the "pdxmap", is at its `+0x58`.

- **The pdxmap's layers**, each found by the effects it loads:

  | Offset | Layer |
  | --- | --- |
  | `+0x18` | Terrain and water |
  | `+0xe8` | Trees |
  | `+0x1160`, `+0x11e8` | Rivers (two of the same class) |
  | `+0x1270` | Borders |
  | `+0x1398` | Straits |
  | `+0x13e8` | Lakes |
  | `+0x1440` | Sky |

- **Map modes.** There are 85 (0–0x54).
  - A change goes through the idler's `+0xc8` (`0x821ff0`) to `0x10a8c30`.
  - That function fills a two-layer province color texture: the colors (`0x12f0b40`, through `0x12fd9b0`) and the stripes over them (`0x12fdcb0`) ([map-modes.md](../provinces/map-modes.md)).
- **Picking** (`0x833360`): the screen point becomes a map position (`0x10a3e50`), and the province ID grid gives the province. The camera is in [camera.md](camera.md).

**The GUI** (`clausewitzlib\graphics`).

- **Types and windows.** A `.gui` file is read into types: `windowType`, `guiButtonType`, `instantTextBoxType` and so on, made by the factory `0x14eac80` by token. A `.gfx` file is read into sprite and font types. A window is made from a type by name ([ui-injection.md](gui-loading.md)).
- **The GUI manager** `[0x23494f0]` (vtable `0x1d58108`) holds the types, makes windows and sends input to them (`0x14e9a70`, by the event's kind). A handled event is marked so that later handlers skip it.
- **Draw and click order.** Visible windows are drawn from one ordered table, which hit testing walks too (inferred: so what is drawn on top is clicked first).
- **Views.** A view is a C++ class in `eu4\source\graphics`:
  - it makes its root window;
  - it finds its children by name;
  - it binds callbacks to them;
  - it refreshes them every frame ([notice-window.md](window-construction.md), [interface-bindings.md](interface-bindings.md)).

  The fourteen country-view tabs are made by `0x81ecf0`.

**Input.**

- SDL's events are queued on the current idler (in a game at its `+0x1208`).
- `0x830fd0` offers each event, in order, to: a modal window, the GUI manager, the graphical map, the idler itself, the camera, then the hotkey listeners.
- The hotkeys are `[0x23494d8]` (`hotkeymanager.cpp`), with the player's bindings in `shortcuts.txt`.
- Keyboard focus and text: [text-input.md](text-input.md).

**Sound.**

- SDL's audio: its own thread mixes, and AudioUpdateThread refills the buffers.
- Sounds and music are declared in `.asset` files; sounds are `.wav` and music is `.ogg`.
- `eu4sound.cpp` and `musicmanager.cpp` are the game's side.
- The mixer `0x16a6c20` takes each playing sound's gain as master × its category (`+0x978` times `+0x97c` or `+0x980` of the audio object) × the sound's own, rounds it times 128 into SDL's volume, at least 1 for a gain above zero, and calls `SDL_MixAudioFormat` for it (`0x1735360`, which jumps through SDL's jump table at `0x1fb0000`; the body is `0x17458b0`). Measured levels: [The music volume the mixer passes to SDL](../../research/mechanics-evidence.md).
- The volumes come from the settings at `[[0x23494e8]+0x28]` (also `[0x242bde8]`): master `+0xb4`, a factor `+0xb8` (100 in every run so far), effects `+0xbc`, music `+0xc0`, ambient `+0xc4`, in percent. The application frame pushes `+0x978` = master × factor, `+0x97c` = effects and `+0x980` = music, each × 0.01, when they changed (`0x14c2730`). Before that runs, as the main theme is first played, the music's gain is master × music × 0.0001 (`0x202296`). The map's ambient sound takes ambient × master × 0.0001 into its terrain gains (`0x6e80ee`). The game asks SDL for 44.1 kHz, `AUDIO_S16LSB`, two channels and 1024 frames, allowing no change (`0x16a68ed`).
