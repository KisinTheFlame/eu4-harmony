# Music volume plan

Status: decided with the owner (2026-11-05); V1 to V3 built in one pull request. Terms are in [GLOSSARY.md](../GLOSSARY.md) (Volume curve, Loudness normalization, Music leveling, Framework settings). The decision is [ADR the-framework-sets-the-sound-volumes](adr/the-framework-sets-the-sound-volumes.md).

## Goal and scope

- **Goal**: music a player turns down stays audible and even. A player found it, at a low volume, coming and going and sometimes silent; the run that measured why is in [prototype-log.md](prototype-log.md), 2026-11-05, "The music volume the mixer passes to SDL".
- **In scope**: the four volume sliders of the game's settings, which go by a volume curve, the square of their position; the music, mixed by the framework at the game's gain as a fraction, with loudness normalization and music leveling as framework settings.
- **Out of scope**: the game's own settings window and its sliders, which stay as they are; sound effects and ambient sound beyond their volume curve; which track the game picks and when (`songs.txt`); the main menu's own 0.75 on its music, and whatever else the game does to a voice's volume; a capability for mods to play music, a plan of its own if one is wanted.

## Why the music comes and goes

From the game's code and the run ([research/architecture.md](research/architecture.md), "Sound"):

- The mixer gives each voice a gain: master × music × the voice's own for music, all linear in the sliders' percentages, and rounds it times 128 into SDL's volume. The player's master 12 and music 67 put the music at 8 to 10 of 128, about -22 dB; with master at 12, the whole music slider is levels 0 to 15.
- The vanilla tracks' integrated loudness runs from -9.9 to -24.3 LUFS, and within a track up to 19 LU between quiet and loud passages. At -22 dB, the quiet tracks and passages fall under the sound effects and ambient sound, which the player had at 77 and 76.

## Behavior

### The volume curve (V1)

- The master, music, effects and ambient sliders of the game's settings give the square of their position as the gain: 100 is full volume, 50 is -12 dB, 10 is -40 dB, 1 is -80 dB, 0 is silence. Loudness goes about as the amplitude to the power 0.6 (half as loud is about -10 dB), so each step of the slider sounds about as large. Vanilla gives the position itself (50 is -6 dB), so that most of the slider sounds alike and its last tenth falls off to silence.
- A curve even in decibels, -40 dB at 1 to 0 dB, was built first and replaced before merging: 50 was -20 dB, a quarter as loud, and with the master and music volumes multiplied, both at 50 were -40 dB, much quieter than players' saved volumes were in vanilla.
- It is always on, with no setting to go back to vanilla's (decided by the owner): a player's saved volumes sound quieter than before below 100, and are set again once.
- Where the game combines them, it still does: the music's gain is the master's times the music's, so their decibels add (master 50 and music 50 are -24 dB), and the map's ambient sound takes the master volume into its own gain, as in vanilla. What else the mixer multiplies a sound effect or an ambient voice by was not followed.

### Music mixed by the framework (V2)

- Every voice that streams, which is the music, is mixed by the framework instead of SDL: at the game's gain, as a fraction rather than one of SDL's 128 steps, added into the game's mix and held within the samples' range as SDL does. Every other voice is mixed by SDL as before.
- **Loudness normalization**, a framework setting, on by default: each track is brought to -12.8 LUFS, the median of the vanilla tracks, raised or lowered by at most 12 dB. A limiter holds the raised samples below full scale.
- A track's loudness is measured as it plays (ITU-R BS.1770, gated as EBU R 128 does) and kept in `eu4-harmony/music-loudness.txt` in the user folder, so that the next time it plays, in this game or a later one, it is brought to the target from its first second. A hearing is kept once a minute of it is measured, and replaces what was kept when it is 20 s longer; while a track plays longer than what was kept of it, it follows what this hearing measured. A track is recognized by its decoded samples, its first second after any silence, so that DLC music and mods' music are recognized too, whatever their files are called.
- A track heard for the first time plays as it is for 3 s; then a loud one is lowered toward its measured loudness at 2 dB a second, and a quiet one is raised so only once a minute of it is measured. A quiet opening is no measure of a track: the main theme is -35.6 LUFS over its first 20 s, -13.7 over a minute and -11.4 over 240 s.
- **Music leveling**, a framework setting, off by default: quiet passages are raised and loud ones lowered, by two thirds of their distance from the target in short-term loudness (the last 3 s), at most 12 dB, following at 3 dB a second. Silence is not raised.

### The settings (V3)

- The settings window gets a page "Sound" (声音), last, with two toggles: "Play every track equally loud" (所有曲目一样响), on, and "Even out quiet and loud passages" (拉近轻响段落（动态压缩）), off, and below them what the sliders and the two do.
- Both take effect when saved, in a game going on too: what a track is raised by follows its new wanted gain at the same pace.

## How it fits together

`sound`, a module of the core:

- **The volume curve**: `sound::gain(percent)`, and three jump points where the game turns the sliders' percentages into gains. The application frame pushes the master, music and effects gains to the audio every frame (`0x14c2768`), but only once the main menu runs; before the main theme first plays, the game gives the music master × music on its own (`0x202296`); the map's ambient sound takes the ambient and master volumes over 100 (`0x6e80f5`). Each stub calls Rust with the percentages the game loaded, and puts the curve's gains where the game's would be.
- **The mix**: two jump points on the mixer's calls of `SDL_MixAudioFormat` (`0x16a744a` for every voice, `0x16a75df` for the rest of a stream after its next buffer). The stub passes the call's arguments, the voice (rbx) and its gain (xmm6) to `sound::mix`, which mixes a streaming voice itself and calls SDL for every other. The game asks SDL for 44.1 kHz, 16 bits, stereo and allows no change, which a point keeps, so every voice comes in that format.
- **Pure parts**, unit-tested without the game: `sound::loudness`, the meter (the K-weighting, the blocks, the gates, the short-term window); `sound::music`, a track's normalization, leveling and limiter, and its fingerprint; `sound::cache`, the file of what was measured.
- **State**: one lock, taken on SDL's audio thread only while a music voice is mixed: each voice's track by the voice, each sound's fingerprint by the sound, and the cache. A track's measurement is kept from a minute on, and when it is 20 s longer than what was kept, and the file written on a thread of its own, since the audio thread must not wait for the disk; it writes only a log line when a track is recognized or measured.
- **The settings**: `music_loudness` and `music_leveling` in the framework settings, taken by `sound::choose` at startup and when saved, as two atomics the audio thread reads.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against the game's files;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- a change players notice is in README.md and README.zh-CN.md.

### V1 The volume curve

`sound::gain` and the three points. Unit tests: the curve's points; the effects gain given for the game's hundredth after. Tried: SDL's volume for a sound effect at a slider position, against the curve.

### V2 The music mixed by the framework

The two mix points, the meter, the track, the fingerprint and the cache. Unit tests: the K-weighting against BS.1770's coefficients at 48 kHz; EBU Tech 3341's tone at -23 LUFS; the gates; mixing with nothing on as SDL does; a known quiet track raised to the target and held below full scale; a first hearing following its estimate; the fingerprint after silence; the cache's file. Tried: the music recognized, measured and kept; recognized again at the next start; its gain the curve's.

### V3 The settings

The two settings, their page and texts in the five languages of the framework's own texts. Unit tests: the settings' file and that both take effect when saved.

## Order

One pull request, with the measuring run that led to it: V1, V2, V3.
