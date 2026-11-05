//! The game's sound volumes and its music (docs/music-volume.md; ADR
//! the-framework-sets-the-sound-volumes):
//!
//! - every volume slider is a volume curve (GLOSSARY) in decibels, where the
//!   game multiplies by the slider's percentage: the master, music and
//!   effects volumes as the application frame pushes them to the audio, the
//!   ambient volume where the map's ambient sound is worked out;
//! - the music is mixed by the framework instead of SDL: at the game's gain,
//!   as a fraction rather than in SDL's 128 steps, with the loudness
//!   normalization and the music leveling (GLOSSARY) the player chose.
//!
//! The game mixes on SDL's audio thread (`0x16a6c20`), voice by voice, each
//! through `SDL_MixAudioFormat`; a voice whose sound streams is music, whose
//! gain is the voice's own volume times the master and music volumes.
mod cache;
pub mod loudness;
pub mod music;

use crate::points::{Function, Point};
use cache::{Cache, Measured};
use music::{Choices, Fingerprint, Track};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// The quietest a volume above 0 is, in dB below full volume.
const CURVE_RANGE: f32 = 40.0;

/// The gain of a volume slider at `percent`: 0 is silence, and 1 to 100 go
/// from -40 dB to full volume evenly in decibels, where the game goes evenly
/// in amplitude.
pub fn gain(percent: f32) -> f32 {
    if percent <= 0.0 {
        0.0
    } else {
        10f32.powf(-CURVE_RANGE * (1.0 - percent.min(100.0) / 100.0) / 20.0)
    }
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
/// framework, every other voice by SDL as the game asked.
unsafe extern "C" fn mix(device: *mut i16, source: *const i16, format: u32, bytes: u32, volume: i32, voice: usize, gain: f32) {
    crate::guarded("mix a voice", || {
        let memory = crate::hooked_memory();
        let sound = memory.ptr(voice + SOUND).unwrap_or(0);
        let stream = sound != 0 && memory.u32(sound + KIND).is_ok_and(|kind| kind == STREAM);
        if stream && format == AUDIO_S16LSB {
            let samples = bytes as usize / 2;
            let (source, device) = unsafe { (std::slice::from_raw_parts(source, samples), std::slice::from_raw_parts_mut(device, samples)) };
            mix_music(voice, sound, source, device, f64::from(gain));
        } else {
            unsafe { ORIGINAL_MIX.get()(device, source, format, bytes, volume) };
        }
    })
}

/// Passes the game's arguments on to [`mix`], with the volume it stored for
/// the call at its `[rsp+0x20]`, the voice in rbx and its gain in xmm6.
macro_rules! mix_stub {
    ($name:ident, $next:ident) => {
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
                mix = sym mix,
                next = sym $next,
            );
        }
    };
}
mix_stub!(voice_stub, VOICE_AT);
mix_stub!(rest_stub, REST_AT);

static NORMALIZE: AtomicBool = AtomicBool::new(true);
static LEVEL: AtomicBool = AtomicBool::new(false);

/// A track playing in one of the mixer's voices.
struct Playing {
    sound: usize,
    track: Track,
    fingerprint: Fingerprint,
    /// The mixer call it was last mixed in.
    last: u64,
}

#[derive(Default)]
struct State {
    playing: HashMap<usize, Playing>,
    /// The fingerprint of each sound heard in this game, to know a track
    /// again from its first frame.
    fingerprints: HashMap<usize, u64>,
    cache: Cache,
    path: Option<PathBuf>,
    calls: u64,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);

/// A voice not mixed for this many calls (about 20 s) has stopped.
const STOPPED: u64 = 1000;

fn mix_music(voice: usize, sound: usize, source: &[i16], device: &mut [i16], gain: f64) {
    let choices = Choices { normalize: NORMALIZE.load(Ordering::Relaxed), level: LEVEL.load(Ordering::Relaxed) };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = &mut *state;
    state.calls += 1;
    let calls = state.calls;
    state.playing.retain(|_, p| calls - p.last < STOPPED);
    let playing = state.playing.entry(voice).or_insert_with(|| Playing { sound: 0, track: Track::default(), fingerprint: Fingerprint::default(), last: 0 });
    if playing.sound != sound {
        *playing = Playing { sound, track: Track::default(), fingerprint: Fingerprint::default(), last: calls };
        if let Some(measured) = state.fingerprints.get(&sound).and_then(|f| state.cache.get(*f)) {
            playing.track.know(measured.loudness);
        }
    }
    playing.last = calls;
    if playing.fingerprint.done().is_none() {
        if let Some(fingerprint) = playing.fingerprint.add(source) {
            state.fingerprints.insert(sound, fingerprint);
            match state.cache.get(fingerprint) {
                Some(measured) => {
                    playing.track.know(measured.loudness);
                    log!("music: track {fingerprint:016x} is known, {:.1} LUFS; the game's gain {gain:.4}", measured.loudness);
                }
                None => log!("music: track {fingerprint:016x} is heard for the first time; the game's gain {gain:.4}"),
            }
        }
    }
    playing.track.mix(source, device, gain, choices);
    let (Some(fingerprint), Some((loudness, seconds))) = (playing.fingerprint.done(), playing.track.measured()) else { return };
    if state.cache.offer(fingerprint, Measured { loudness, seconds }) {
        log!("music: track {fingerprint:016x} measured {loudness:.1} LUFS over {seconds:.0} s");
        if let Some(path) = state.path.clone() {
            let cache = state.cache.clone();
            // Off the audio thread, which must not wait for the disk.
            std::thread::spawn(move || {
                if let Err(e) = cache::write(&path, &cache) {
                    log!("music: {} not written: {e}", path.display());
                }
            });
        }
    }
}

/// Reads what earlier games measured, at startup, and takes the player's choices.
pub fn configure(user_dir: &Path, settings: &crate::settings::Settings) {
    let path = cache::path(user_dir);
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.cache = cache::read(&path);
    state.path = Some(path);
    drop(state);
    choose(settings);
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
    fn the_volume_curve_is_even_in_decibels() {
        assert_eq!(gain(0.0), 0.0);
        assert_eq!(gain(-5.0), 0.0);
        assert_eq!(gain(100.0), 1.0);
        assert_eq!(gain(150.0), 1.0);
        let db = |p| 20.0 * gain(p).log10();
        assert!((db(50.0) - -20.0).abs() < 1e-4);
        assert!((db(1.0) - -39.6).abs() < 1e-4);
        assert!((db(75.0) - db(50.0) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn the_effects_gain_is_given_for_the_games_hundredth_after() {
        let mut out = [0.0; 3];
        volumes(50.0, 100.0, 100.0, 50.0, &mut out);
        assert!((out[0] - 0.1).abs() < 1e-6 && out[1] == 1.0);
        assert!((out[2] * 0.01 - 0.1).abs() < 1e-6);
        assert!((f32::from_bits(ambient(50.0, 50.0)) - 0.01).abs() < 1e-6);
    }
}
