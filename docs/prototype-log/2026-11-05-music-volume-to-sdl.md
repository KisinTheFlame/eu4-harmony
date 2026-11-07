# 2026-11-05 The music volume the mixer passes to SDL (tested by me)

A player found music at a low volume setting coming and going, sometimes silent. This run measured what the game's mixer (`0x16a6c20`) hands SDL for the music. A developer build with the call trace on `SDL_MixAudioFormat`'s body, `0x17458b0`; the game's `0x1735360` and the dynamic API's default `0x172e8a0` only jump to it through SDL's jump table at `0x1fb0000`, slot `+0x1b0`, which SDL fills at its start. Hidden starts, without sound: SDL's `dummy` driver still runs the mixer. Main menu only, no save loaded; `master_volume` and `music_volume` set in the player's `settings.txt` before each start.

| `master_volume` | `music_volume` | Loading screen | Main menu |
| --- | --- | --- | --- |
| 12 | 67.32 (the player's) | 10 | 8 |
| 100 | 100 | 128 | 96 |
| 100 | 10 | 13 | 10 |
| 12 | 3 | 1 | 1 |

- **The level** is the gain times 128, rounded, out of SDL's 128: the gain is master × music × the sound's own, all linear in amplitude, so a level is that fraction of the sample's amplitude. At the main menu a further 0.75 applies; whether it is the main theme's own volume or the menu's was not looked into.
- **Never 0 above 0**: master 12 with music 3 gives 0.46, which rounds to 0, yet SDL was given 1. A gain above zero plays at level 1 at least, about -42 dB.
- **The player's setting**: master 12 leaves the whole music slider levels 0 to 15; the player's music plays at level 8 to 10 out of 128, about -22 dB.
- **The tracks**: measured offline with ffmpeg's `ebur128`, the vanilla tracks' integrated loudness runs from -9.9 LUFS (`event_war_battleofbreitenfeld`) to -24.3 (`kingscourt`), and within a track the loudness range is up to 18.9 LU (`kingsinthenorth`, `maintheme`). No vanilla `music.asset` entry sets a `volume`; in the DLCs' music `.asset` files, 42 entries set one, from 0.35 to 0.9. A quiet track at level 8, against sound effects and ambient sound at 77 and 76, is the coming and going the player heard.
- **Not verified**: music in a running game and its war and peace tracks; fades between tracks, none of which these runs crossed; what was heard, since no sound was played; the 0.75 at the main menu.
- **Cleanup**: releasing the game put back the player's files, `settings.txt` with its volumes among them, and the game folder's framework files; `dev.toml` was deleted, and the copied logs were in the session's scratch folder.
