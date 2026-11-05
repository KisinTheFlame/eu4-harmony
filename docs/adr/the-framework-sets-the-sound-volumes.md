# The framework sets the sound volumes

The framework decides how loud the game's sound is: the volume sliders of the game's settings give the square of their position, always, and the music is mixed by the framework rather than SDL, at the game's gain as a fraction, with loudness normalization (on by default), by the loudness of each track's whole file, measured as it starts, and music leveling (off by default) as framework settings ([music-volume.md](../music-volume.md)).

Vanilla makes every slider linear in amplitude and multiplies them, then rounds the gain into SDL's 128 steps. A player who turns the master volume down leaves the music a few steps, about -22 dB at the player's own settings, and the vanilla tracks differ by 14 dB in loudness and by up to 19 LU within a track ([prototype-log.md](../prototype-log.md), 2026-11-05). Quiet tracks and passages then fall under the sound effects. Mixing the music itself lets the framework give it any gain, and measure what it plays.

## Considered Options

- **Only the curve**: a smaller change, but the tracks would still differ by 14 dB, which is what the player heard come and go.
- **The curve as a framework setting, vanilla's by default**: no player's volumes would change unasked, but a setting few find would leave most players with the steep slider. The owner chose the curve for everyone.
- **Loudness from a table made from the game's files**: the DLC music is in archives and mods add their own, so a table would always lag. Reading the file through the game's file system as the track starts covers every track the game can play.
- **Measuring as a track plays, kept for the next time**: no decoder and no file to find, but a first hearing is not right until it is over, and what a game ended early keeps is wrong: the main theme's quiet opening measured 24 LU below the track. Built first, and replaced before merging.
- **A curve even in decibels** (-40 dB at 1): each step the same ratio, but the middle of the slider far quieter than vanilla's (50 is -20 dB), and the master and music volumes multiplied make that -40 dB. The square gives about the same loudness to each step, and keeps the upper half close to vanilla's.
- **Volume steps scaled by SDL**: the framework could hand SDL a recomputed volume, but 128 steps are coarse below about 8, where a step is a decibel or more.

## Consequences

- Every player's saved volumes below 100 sound quieter than before (50 is -12 dB instead of -6), once, until set again; there is no going back to vanilla's curve.
- The framework holds more dependency points in the game's sound (the volume push, the main theme's first gain, the ambient volume, the mixer's two calls and the device it asks for), and keeps `SDL_MixAudioFormat` callable; a game build whose audio differs does not start (ADR framework-succeeds-or-fails-as-a-whole).
- The framework runs on SDL's audio thread for the first time, for each chunk of music: a lock held for the chunk, and of the disk only a log line when a track starts.
- The framework decodes Ogg Vorbis (`lewton`), a file at each track's start on a thread of its own, about half a second for the main theme; the sound's path is one more dependency point.
