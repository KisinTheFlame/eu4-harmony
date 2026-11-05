//! The game's sound volumes and its music (docs/music-volume.md; ADR
//! the-framework-sets-the-sound-volumes):
//!
//! - every volume slider goes by a volume curve (GLOSSARY), the square of its
//!   position, where the game multiplies by the slider's percentage: the master, music and
//!   effects volumes as the application frame pushes them to the audio, the
//!   ambient volume where the map's ambient sound is worked out;
//! - the music is mixed by the framework instead of SDL: at the game's gain,
//!   as a fraction rather than in SDL's 128 steps, with the loudness
//!   normalization and the music leveling (GLOSSARY) the player chose. As a
//!   track starts, its whole file is read through the game's file system and
//!   measured on a thread of its own; the track waits where it is until
//!   that is done, well within a second, and each file is measured once a
//!   game.
//!
//! The game mixes on SDL's audio thread (`0x16a6c20`), voice by voice, each
//! through `SDL_MixAudioFormat`; a voice whose sound streams is music, whose
//! gain is the voice's own volume times the master and music volumes.
mod analysis;
pub mod loudness;
pub mod music;

use crate::points::{Function, Point};
use music::{Choices, Track};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

/// The gain of a volume slider at `percent`: the square of its position, so
/// that each step sounds about as large a change (loudness goes about as the
/// amplitude to the power 0.6), where the game takes the position itself and
/// leaves the last tenth of the slider to fall off steeply. 50 is -12 dB, 10
/// is -40 dB, and 0 is silence.
pub fn gain(percent: f32) -> f32 {
    let position = percent.clamp(0.0, 100.0) / 100.0;
    position * position
}

/// The application frame pushes the volumes to the audio: the settings'
/// master volume in xmm0, a factor beside it (100 so far) in xmm1, the music
/// volume in xmm2 and the effects volume in xmm3, all in percent, which
/// these four instructions make the gains of the master (times the factor)
/// in xmm1 and of the music in xmm2. The effects volume is multiplied by 0.01
/// after them (`mulss xmm3, xmm6`).
const VOLUMES_PUSHED: usize = 0x14c2768;
const VOLUMES_PUSHED_NEXT: usize = 0x14c2778;
const VOLUMES_READ: usize = 0x14c2744;
const EFFECTS_SCALED: usize = 0x14c2778;
/// The settings, at `[[0x23494e8]+0x28]`, and their fields: master, factor,
/// effects, music, ambient.
const SETTINGS: usize = 0x28;
const MASTER: usize = 0xb4;
const FACTOR: usize = 0xb8;
const EFFECTS: usize = 0xbc;
const MUSIC: usize = 0xc0;
const AMBIENT: usize = 0xc4;
/// The map's ambient sound: the ambient volume over 100 times the master
/// volume over 100, into xmm3, from the settings at rax.
const AMBIENT_SCALED: usize = 0x6e80f5;
const AMBIENT_SCALED_NEXT: usize = 0x6e8119;
const HUNDRED: usize = 0x1dc18c8;
/// Before the main theme first plays, the music's gain is the master volume
/// times the music volume, each times 0.01, into xmm2 (and the master's in
/// xmm1), from the settings at rax; the volumes are pushed apart only later.
const THEME_GAIN: usize = 0x202296;
const THEME_GAIN_NEXT: usize = 0x2022ba;
const HUNDREDTH: usize = 0x1dc14ec;
/// The mixer's `SDL_MixAudioFormat(device, source, format, bytes, volume)`
/// for a voice at rbx, whose gain is in xmm6: the first for every voice, the
/// second for a streaming voice's rest after its next buffer.
const VOICE_MIXED: usize = 0x16a744a;
const VOICE_MIXED_NEXT: usize = 0x16a744f;
const STREAM_REST_MIXED: usize = 0x16a75df;
const STREAM_REST_MIXED_NEXT: usize = 0x16a75e4;
/// The path of a sound's file in the game's files, as text in the sound,
/// which loading a sound puts in its message when the file cannot be loaded
/// (`lea r8, [rdi+0x50]`); 128 bytes, up to the sound's volume at `+0xd0`.
const PATH_GIVEN: usize = 0x16a9f6d;
const PATH: usize = 0x50;
const PATH_LENGTH: usize = 0x80;
/// After the first call, the mixer moves the voice on by what it mixed:
/// `add [rbx+0x10], r14d`, the bytes it passed, to the voice's position in
/// its buffer.
const POSITION_MOVED: usize = 0x16a7497;
const POSITION: usize = 0x10;
/// Where the mixer tells a streaming voice: its sound `[voice+8]`, whose
/// kind `[sound+0xc]` is 1 for a stream.
const VOICE_KIND: usize = 0x16a7452;
const SOUND: usize = 0x8;
const KIND: usize = 0xc;
const STREAM: u32 = 1;
/// The gain in xmm6: the voice's volume `[voice+0x40]`, times the master and
/// music volumes for a stream.
const GAIN_FROM_VOICE: usize = 0x16a72ab;
const GAIN_FOR_STREAM: usize = 0x16a72d8;
const VOICE_VOLUME: usize = 0x40;
/// SDL's `SDL_MixAudioFormat`, through its jump table.
const SDL_MIX: usize = 0x1735360;
/// The audio the game asks SDL for, allowing no change: 44.1 kHz, signed
/// 16 bits, two channels; so every voice is mixed in that format.
const AUDIO_ASKED: usize = 0x16a68ed;
const NO_CHANGES_ALLOWED: usize = 0x16a6919;
const AUDIO_S16LSB: u32 = 0x8010;

crate::resume!(VOLUMES_RESUME: VOLUMES_AT = VOLUMES_PUSHED_NEXT);
crate::resume!(AMBIENT_RESUME: AMBIENT_AT = AMBIENT_SCALED_NEXT);
crate::resume!(THEME_RESUME: THEME_AT = THEME_GAIN_NEXT);
crate::resume!(VOICE_RESUME: VOICE_AT = VOICE_MIXED_NEXT);
crate::resume!(REST_RESUME: REST_AT = STREAM_REST_MIXED_NEXT);

type Mix = unsafe extern "C" fn(*mut i16, *const i16, u32, u32, i32);
static ORIGINAL_MIX: Function<Mix> = Function::new();

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(
            VOLUMES_READ,
            "48 8b 48 28 f3 0f 10 81 b4 00 00 00 f3 0f 10 89 b8 00 00 00 f3 0f 10 91 c0 00 00 00 f3 0f 10 99 bc 00 00 00",
            "volumes read",
        )
        .uses(&[SETTINGS, MASTER, FACTOR, MUSIC, EFFECTS]),
        Point::jump(VOLUMES_PUSHED, "f3 0f 59 c6 f3 0f 59 ce f3 0f 59 d6 f3 0f 59 c8", "volumes pushed", volumes_stub as *const () as usize, &VOLUMES_RESUME),
        Point::keep(EFFECTS_SCALED, "f3 0f 10 05 34 43 e6 00 f3 0f 59 de", "effects volume scaled"),
        Point::jump(
            AMBIENT_SCALED,
            "f3 0f 10 98 c4 00 00 00 f3 0f 5e 1d c3 97 6d 01 f3 0f 10 88 b4 00 00 00 f3 0f 5e 0d b3 97 6d 01 f3 0f 59 d9",
            "ambient volume scaled",
            ambient_stub as *const () as usize,
            &AMBIENT_RESUME,
        )
        .uses(&[AMBIENT, MASTER, HUNDRED]),
        Point::jump(
            THEME_GAIN,
            "f3 0f 10 88 b4 00 00 00 f3 0f 59 0d 46 f2 bb 01 f3 0f 10 90 c0 00 00 00 f3 0f 59 15 36 f2 bb 01 f3 0f 59 d1",
            "main theme gain",
            theme_stub as *const () as usize,
            &THEME_RESUME,
        )
        .uses(&[MASTER, MUSIC, HUNDREDTH]),
        // Both call SDL_MIX.
        Point::jump(VOICE_MIXED, "e8 11 df 08 00", "voice mixed", voice_stub as *const () as usize, &VOICE_RESUME),
        Point::jump(STREAM_REST_MIXED, "e8 7c dd 08 00", "stream rest mixed", rest_stub as *const () as usize, &REST_RESUME),
        Point::keep(VOICE_KIND, "48 8b 43 08 83 78 0c 01", "voice kind").uses(&[SOUND, KIND]),
        Point::keep(POSITION_MOVED, "44 01 73 10", "voice moved on").uses(&[POSITION]),
        Point::keep(PATH_GIVEN, "4c 8d 47 50", "sound path").uses(&[PATH]),
        Point::keep(GAIN_FROM_VOICE, "f3 0f 10 73 40", "voice gain").uses(&[VOICE_VOLUME]),
        Point::keep(GAIN_FOR_STREAM, "f3 41 0f 59 f4", "stream gain"),
        Point::keep(SDL_MIX, "48 ff 25 49 ae 87 00", "SDL mix audio").recorded(ORIGINAL_MIX.slot()),
        Point::keep(AUDIO_ASKED, "c7 45 d7 44 ac 00 00 b8 10 80 00 00 66 89 45 db c6 45 dd 02", "audio asked").uses(&[44100, AUDIO_S16LSB as usize]),
        Point::keep(NO_CHANGES_ALLOWED, "c7 44 24 20 00 00 00 00", "audio changes allowed"),
    ]
}

/// The gains of the master (times the factor beside it), music and effects
/// volumes, the last times 100 for the game's 0.01 after.
extern "C" fn volumes(master: f32, factor: f32, music: f32, effects: f32, out: *mut [f32; 3]) {
    unsafe { *out = [gain(master) * factor / 100.0, gain(music), gain(effects) * 100.0] };
}

#[unsafe(naked)]
extern "C" fn volumes_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(
            "lea rax, [rsp + 0x30]",
            "mov [rsp + 0x20], rax",
            call volumes,
            "movss xmm1, [rsp + 0x30]",
            "movss xmm2, [rsp + 0x34]",
            "movss xmm3, [rsp + 0x38]"
        ),
        "jmp qword ptr [rip + {next}]",
        volumes = sym volumes,
        next = sym VOLUMES_AT,
    );
}

/// The gain of the map's ambient sound, as the bits of an `f32`.
extern "C" fn ambient(ambient: f32, master: f32) -> u32 {
    (gain(ambient) * gain(master)).to_bits()
}

#[unsafe(naked)]
extern "C" fn ambient_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("movss xmm0, [rax + 0xc4]", "movss xmm1, [rax + 0xb4]", call ambient, "movd xmm3, dword ptr [rsp + 0x48]"),
        "jmp qword ptr [rip + {next}]",
        ambient = sym ambient,
        next = sym AMBIENT_AT,
    );
}

/// The gains of the master volume and of the music with it, before the
/// main theme first plays.
extern "C" fn theme(master: f32, music: f32, out: *mut [f32; 2]) {
    unsafe { *out = [gain(master), gain(master) * gain(music)] };
}

#[unsafe(naked)]
extern "C" fn theme_stub() {
    std::arch::naked_asm!(
        crate::call_rust!(
            "movss xmm0, [rax + 0xb4]",
            "movss xmm1, [rax + 0xc0]",
            "lea r8, [rsp + 0x30]",
            call theme,
            "movss xmm1, [rsp + 0x30]",
            "movss xmm2, [rsp + 0x34]"
        ),
        "jmp qword ptr [rip + {next}]",
        theme = sym theme,
        next = sym THEME_AT,
    );
}

/// Mixes one voice's `bytes` into the device's buffer: music by the
/// framework, every other voice by SDL as the game asked. A track whose
/// file is still being analyzed is held where it is, at the mixer's first
/// call, which moves it on by `bytes` after.
#[expect(clippy::too_many_arguments, reason = "the game's call's arguments, the voice and its gain, and which of the two calls it is")]
fn mix(device: *mut i16, source: *const i16, format: u32, bytes: u32, volume: i32, voice: usize, gain: f32, first: bool) {
    crate::guarded("mix a voice", || {
        let memory = crate::hooked_memory();
        let sound = memory.ptr(voice + SOUND).unwrap_or(0);
        let stream = sound != 0 && memory.u32(sound + KIND).is_ok_and(|kind| kind == STREAM);
        if stream && format == AUDIO_S16LSB {
            let samples = bytes as usize / 2;
            let (source, device) = unsafe { (std::slice::from_raw_parts(source, samples), std::slice::from_raw_parts_mut(device, samples)) };
            if mix_music(voice, sound, source, device, f64::from(gain), first) {
                let position = memory.i32(voice + POSITION).unwrap_or(0);
                let _ = memory.write(voice + POSITION, &(position - bytes as i32).to_le_bytes());
            }
        } else {
            unsafe { ORIGINAL_MIX.get()(device, source, format, bytes, volume) };
        }
    })
}

unsafe extern "C" fn mix_first(device: *mut i16, source: *const i16, format: u32, bytes: u32, volume: i32, voice: usize, gain: f32) {
    mix(device, source, format, bytes, volume, voice, gain, true);
}

unsafe extern "C" fn mix_rest(device: *mut i16, source: *const i16, format: u32, bytes: u32, volume: i32, voice: usize, gain: f32) {
    mix(device, source, format, bytes, volume, voice, gain, false);
}

/// Passes the game's arguments on to `$mix`, with the volume it stored for
/// the call at its `[rsp+0x20]`, the voice in rbx and its gain in xmm6.
macro_rules! mix_stub {
    ($name:ident, $mix:ident, $next:ident) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            std::arch::naked_asm!(
                crate::call_rust!(
                    "mov eax, [rbp + 0x28]",
                    "mov [rsp + 0x20], rax",
                    "mov [rsp + 0x28], rbx",
                    "movss [rsp + 0x30], xmm6",
                    call mix
                ),
                "jmp qword ptr [rip + {next}]",
                mix = sym $mix,
                next = sym $next,
            );
        }
    };
}
mix_stub!(voice_stub, mix_first, VOICE_AT);
mix_stub!(rest_stub, mix_rest, REST_AT);

static NORMALIZE: AtomicBool = AtomicBool::new(true);
static LEVEL: AtomicBool = AtomicBool::new(false);

/// The integrated loudness of a file's whole track: unset while it is
/// analyzed, None when it could not be.
type Analysis = Arc<OnceLock<Option<f64>>>;

/// A track playing in one of the mixer's voices.
struct Playing {
    sound: usize,
    track: Track,
    analysis: Analysis,
    /// The mixer calls it was held for, waiting for its analysis.
    held: u32,
    /// The mixer call it was last mixed in.
    last: u64,
}

#[derive(Default)]
struct State {
    playing: HashMap<usize, Playing>,
    /// Every file analyzed in this game, by its path in the game's files.
    analyses: HashMap<String, Analysis>,
    calls: u64,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);

/// A voice not mixed for this many calls (about 20 s) has stopped.
const STOPPED: u64 = 1000;
/// The most a track is held for its analysis, in mixer calls (about 2 s);
/// then it plays as it is until the analysis is in.
const HOLD: u32 = 86;

/// Mixes a streaming voice; whether it was held instead, at a first call
/// (`first`), because its file's analysis is not in yet.
fn mix_music(voice: usize, sound: usize, source: &[i16], device: &mut [i16], gain: f64, first: bool) -> bool {
    let choices = Choices { normalize: NORMALIZE.load(Ordering::Relaxed), level: LEVEL.load(Ordering::Relaxed) };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = &mut *state;
    state.calls += 1;
    let calls = state.calls;
    state.playing.retain(|_, p| calls - p.last < STOPPED);
    if state.playing.get(&voice).is_none_or(|p| p.sound != sound) {
        let path = path_of(sound);
        log!("music: {path} plays; the game's gain {gain:.4}");
        let analysis = state.analyses.entry(path.clone()).or_insert_with(|| analyze(path)).clone();
        state.playing.insert(voice, Playing { sound, track: Track::default(), analysis, held: 0, last: calls });
    }
    let Some(playing) = state.playing.get_mut(&voice) else { return false };
    playing.last = calls;
    if !playing.track.known() {
        match playing.analysis.get() {
            Some(Some(loudness)) => playing.track.know(*loudness),
            None if first && choices.normalize && playing.held < HOLD && !playing.track.started() => {
                playing.held += 1;
                return true;
            }
            _ => {}
        }
        if playing.held > 0 && !playing.track.started() {
            log!("music: held {} ms for its analysis", u64::from(playing.held) * 1024 * 1000 / 44100);
        }
    }
    playing.track.mix(source, device, gain, choices);
    false
}

/// The path of a sound's file in the game's files, as the sound keeps it.
fn path_of(sound: usize) -> String {
    let bytes = crate::hooked_memory().bytes(sound + PATH, PATH_LENGTH).unwrap_or_default();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Reads and measures the whole of the file at `path` on a thread of its
/// own, since the audio thread must wait neither for the disk nor for the
/// decoding.
fn analyze(path: String) -> Analysis {
    let cell = Analysis::default();
    let result = cell.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let loudness = crate::physfs::read(&path).map_err(|e| e.to_string()).and_then(|data| analysis::loudness(&data));
        let took = started.elapsed().as_secs_f64();
        match &loudness {
            Ok(Some(loudness)) => log!("music: {path} is {loudness:.1} LUFS, analyzed in {took:.2} s"),
            Ok(None) => log!("music: {path} is silent"),
            Err(e) => log!("music: {path} was not analyzed, and plays as it is: {e}"),
        }
        let _ = result.set(loudness.ok().flatten());
    });
    cell
}

/// Takes the player's choices, at startup and when the settings are saved.
pub fn choose(settings: &crate::settings::Settings) {
    NORMALIZE.store(settings.music_loudness, Ordering::Relaxed);
    LEVEL.store(settings.music_leveling, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_curve_is_the_square_of_the_position() {
        assert_eq!(gain(0.0), 0.0);
        assert_eq!(gain(-5.0), 0.0);
        assert_eq!(gain(100.0), 1.0);
        assert_eq!(gain(150.0), 1.0);
        let db = |p| 20.0 * gain(p).log10();
        assert!((db(50.0) - -12.04).abs() < 1e-2);
        assert!((db(10.0) - -40.0).abs() < 1e-4);
        assert!((db(1.0) - -80.0).abs() < 1e-3);
    }

    #[test]
    fn the_effects_gain_is_given_for_the_games_hundredth_after() {
        let mut out = [0.0; 3];
        volumes(50.0, 100.0, 100.0, 50.0, &mut out);
        assert!((out[0] - 0.25).abs() < 1e-6 && out[1] == 1.0);
        assert!((out[2] * 0.01 - 0.25).abs() < 1e-6);
        assert!((f32::from_bits(ambient(50.0, 50.0)) - 0.0625).abs() < 1e-6);
    }
}
