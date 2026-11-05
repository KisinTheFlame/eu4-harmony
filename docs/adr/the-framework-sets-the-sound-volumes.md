# The framework sets the sound volumes

The framework decides how loud the game's sound is: the volume sliders of the game's settings are a volume curve in decibels, always, and the music is mixed by the framework rather than SDL, at the game's gain as a fraction, with loudness normalization (on by default) and music leveling (off by default) as framework settings ([music-volume.md](../music-volume.md)).

Vanilla makes every slider linear in amplitude and multiplies them, then rounds the gain into SDL's 128 steps. A player who turns the master volume down leaves the music a few steps, about -22 dB at the player's own settings, and the vanilla tracks differ by 14 dB in loudness and by up to 19 LU within a track ([prototype-log.md](../prototype-log.md), 2026-11-05). Quiet tracks and passages then fall under the sound effects. Mixing the music itself lets the framework give it any gain, and measure what it plays.

## Considered Options

- **Only the curve**: a smaller change, but the tracks would still differ by 14 dB, which is what the player heard come and go.
- **The curve as a framework setting, vanilla's by default**: no player's volumes would change unasked, but a setting few find would leave most players with the steep slider. The owner chose the curve for everyone.
- **Loudness from a table made from the game's files**: exact from a track's first second, but the DLC music is in archives and mods add their own, so the table would always lag. Measuring as a track plays covers every track, and the cache makes it exact from the second hearing on.
- **Volume steps scaled by SDL**: the framework could hand SDL a recomputed volume, but 128 steps are coarse below about 8, where a step is a decibel or more.

## Consequences

- Every player's saved volumes below 100 sound quieter than before, once, until set again; there is no going back to vanilla's curve.
- The framework holds more dependency points in the game's sound (the volume push, the main theme's first gain, the ambient volume, the mixer's two calls and the device it asks for), and keeps `SDL_MixAudioFormat` callable; a game build whose audio differs does not start (ADR framework-succeeds-or-fails-as-a-whole).
- The framework runs on SDL's audio thread for the first time, for each chunk of music: a lock held for the chunk, and of the disk only a log line when a track is recognized or measured; the cache is written on a thread of its own.
- The user folder gains `eu4-harmony/music-loudness.txt`, one line a track heard.
