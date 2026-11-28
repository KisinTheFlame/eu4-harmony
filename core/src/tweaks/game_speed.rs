//! Configurable game speeds 1 to 5 (docs/game/day/time.md,
//! docs/game/day/frame-scheduling.md).
//!
//! Vanilla EU4 hardcodes five speed levels (`world+0x1e08`, indices `0..=4`)
//! across three places:
//!
//! 1. The daily step threshold table at `0x1c33e30` (`2.0, 1.0, 0.5, 0.2, 0.0`
//!    seconds per day), read in `0x757ad0`. Once the frame-time accumulator
//!    (`world+0x1df8`) reaches the threshold, vanilla resets it to `0` rather
//!    than subtracting the threshold, which quantizes fast finite rates down
//!    to integer frame multiples (e.g. 40 and 50 days/s both collapse to 30
//!    days/s at 60 FPS).
//! 2. The mid-day frame check interval table at `0x1c33e18` (`0.02 s` for
//!    speeds 1–4, `0.05 s` for speed 5), read in `0x769ea0`.
//! 3. Two speed-index checks in unit map-model movement (`0x1447cf0`, at
//!    `0x1447dff` and `0x1447f98`) that enable smooth inter-province grace and
//!    intra-province glide for speeds 1–4 (`< 4`) and skip them on speed 5.
//!
//! This module lets the player set each of the five speeds independently in
//! days per second (or unlimited), subtracts one threshold from the
//! accumulator while clamping any excess to one threshold (preventing burst
//! catch-up after a stall while eliminating frame-rate quantization), and
//! drives both unit-movement smoothing and the mid-day frame interval from the
//! configured pace (`<= 10` days/s stays smooth / `20 ms`; `> 10` days/s or
//! unlimited switches to instant unit jumps / `50 ms`).
use crate::base::called_by_game;
use crate::base::settings::GameSpeed;
use crate::native::objects::campaign::world;
use crate::native::points::Point;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

/// World frame-time accumulator (`f32` seconds since last posted `DailyTick`).
const WORLD_ACCUM: usize = 0x1df8;

/// Vanilla daily-step threshold table (`[2.0, 1.0, 0.5, 0.2, 0.0]`).
const VANILLA_STEP_TABLE: usize = 0x1c33e30;
/// Vanilla mid-day frame interval table (`[0.02, 0.02, 0.02, 0.02, 0.05]`).
const VANILLA_MID_DAY_TABLE: usize = 0x1c33e18;

/// Highest finite pace (in days per second) that keeps unit movement smoothing
/// and the `20 ms` mid-day frame interval; faster finite rates and unlimited
/// switch to speed-5 behavior (skip smoothing, `50 ms` mid-day interval).
pub const SMOOTHING_MAX_DAYS_PER_SEC: f32 = 10.0;
pub const MID_DAY_SMOOTH_SECONDS: f32 = 0.02;
pub const MID_DAY_FAST_SECONDS: f32 = 0.05;

/// Inside `0x757ad0`: loads `world+0x1e08` and fetches the day threshold into
/// `xmm0` before `comiss xmm0, xmm2` at `0x757b1c`.
const STEP_THRESHOLD: usize = 0x757b09;
const STEP_COMPARE: usize = 0x757b1c;
/// Inside `0x757ad0`: `mov [rbp+0x1df8], esi` resets the accumulator right
/// before allocating and posting `DailyTick`.
const STEP_RESET: usize = 0x757c83;
const STEP_POST: usize = 0x757c89;
/// Inside `0x769ea0`: `mulss xmm6, [r13+rax*4+0x1c33e18]` scales the mid-day
/// frame budget by the speed's interval factor.
const MID_DAY_INTERVAL: usize = 0x769f61;
const MID_DAY_NEXT: usize = 0x769f6b;
/// Inside `0x1447cf0`: `cmp dword ptr [rax+0x1e08], 3; jg 0x1447ebd` gates
/// the 6-day inter-province movement grace period.
const UNIT_SMOOTH_GRACE: usize = 0x1447dff;
const UNIT_SMOOTH_GRACE_KEEP: usize = 0x1447e0c;
const UNIT_SMOOTH_GRACE_SKIP: usize = 0x1447ebd;
/// Inside `0x1447cf0`: `cmp dword ptr [r9+0x1e08], 4` selects `0.0` (smooth)
/// or `xmm9` (`5.0`, instant) as the intra-province progress floor in `xmm1`.
const UNIT_SMOOTH_FLOOR: usize = 0x1447f98;
const UNIT_SMOOTH_FLOOR_NEXT: usize = 0x1447fab;

/// Per-speed minimum day interval in seconds (`f32::to_bits()`), indexed `0..=4`.
static DAY_INTERVAL_BITS: [AtomicU32; 5] = [
    AtomicU32::new(2.0f32.to_bits()),
    AtomicU32::new(1.0f32.to_bits()),
    AtomicU32::new(0.5f32.to_bits()),
    AtomicU32::new(0.2f32.to_bits()),
    AtomicU32::new(0.0f32.to_bits()),
];

/// Per-speed mid-day frame interval in seconds (`f32::to_bits()`), indexed `0..=4`.
static MID_DAY_INTERVAL_BITS: [AtomicU32; 5] = [
    AtomicU32::new(MID_DAY_SMOOTH_SECONDS.to_bits()),
    AtomicU32::new(MID_DAY_SMOOTH_SECONDS.to_bits()),
    AtomicU32::new(MID_DAY_SMOOTH_SECONDS.to_bits()),
    AtomicU32::new(MID_DAY_SMOOTH_SECONDS.to_bits()),
    AtomicU32::new(MID_DAY_FAST_SECONDS.to_bits()),
];

/// Per-speed unit movement smoothing flag (`1` smooth, `0` instant), indexed `0..=4`.
static UNIT_SMOOTHING: [AtomicU8; 5] = [AtomicU8::new(1), AtomicU8::new(1), AtomicU8::new(1), AtomicU8::new(1), AtomicU8::new(0)];

/// Minimum day interval in seconds for `speed` (`0.0` for unlimited).
pub fn day_interval(speed: GameSpeed) -> f32 {
    match speed.rate() {
        Some(rate) if rate.is_finite() && rate > 0.0 => 1.0 / rate,
        _ => 0.0,
    }
}

/// Whether `speed` uses fast-mode behavior (skip unit movement smoothing and
/// use the `50 ms` mid-day frame interval).
pub fn is_fast_speed(speed: GameSpeed) -> bool {
    match speed.rate() {
        Some(rate) if rate.is_finite() && rate > 0.0 => rate > SMOOTHING_MAX_DAYS_PER_SEC,
        _ => true,
    }
}

/// Next accumulator value after posting a day at `threshold` seconds/day.
/// Keeps sub-frame remainder to avoid frame-rate quantization while clamping
/// the carry to at most one `threshold` so a stall never triggers a burst of
/// catch-up days. Unlimited (`threshold <= 0.0`) always resets to `0.0`.
pub fn next_accumulator(accum: f32, threshold: f32) -> f32 {
    if !accum.is_finite() || !threshold.is_finite() || threshold <= 0.0 {
        return 0.0;
    }
    (accum - threshold).clamp(0.0, threshold)
}

/// Updates all runtime speed tables from framework settings.
pub fn configure(speeds: [GameSpeed; 5]) {
    for (i, speed) in speeds.into_iter().enumerate() {
        let interval = day_interval(speed);
        let fast = is_fast_speed(speed);
        let mid_day = if fast { MID_DAY_FAST_SECONDS } else { MID_DAY_SMOOTH_SECONDS };
        DAY_INTERVAL_BITS[i].store(interval.to_bits(), Ordering::Relaxed);
        MID_DAY_INTERVAL_BITS[i].store(mid_day.to_bits(), Ordering::Relaxed);
        UNIT_SMOOTHING[i].store(u8::from(!fast), Ordering::Relaxed);
    }
}

fn day_interval_at(index: i32) -> f32 {
    let idx = if (0..=4).contains(&index) { index as usize } else { 0 };
    f32::from_bits(DAY_INTERVAL_BITS[idx].load(Ordering::Relaxed))
}

#[called_by_game]
extern "C" fn advance_accumulator(world_ptr: usize) {
    crate::base::guarded("game speed accumulator", || {
        if world_ptr == 0 {
            return;
        }
        let m = crate::native::hooked_memory();
        let speed = m.i32(world_ptr + world::SPEED).unwrap_or(0);
        let accum = m.u32(world_ptr + WORLD_ACCUM).map(f32::from_bits).unwrap_or(0.0);
        let next = next_accumulator(accum, day_interval_at(speed));
        let _ = m.write(world_ptr + WORLD_ACCUM, &next.to_le_bytes());
    });
}

resume!(STEP_THRESHOLD_RESUME: STEP_COMPARE_AT = STEP_COMPARE);

#[unsafe(naked)]
extern "C" fn step_threshold_stub() {
    std::arch::naked_asm!(
        "movsxd rax, dword ptr [rcx + {speed}]",
        "cmp rax, 4",
        "jbe 2f",
        "xor eax, eax",
        "2:",
        "lea rcx, [rip + {intervals}]",
        "movss xmm0, dword ptr [rcx + rax*4]",
        "jmp qword ptr [rip + {compare}]",
        speed = const world::SPEED,
        intervals = sym DAY_INTERVAL_BITS,
        compare = sym STEP_COMPARE_AT,
    );
}

resume!(STEP_RESET_RESUME: STEP_POST_AT = STEP_POST);

#[unsafe(naked)]
extern "C" fn step_reset_stub() {
    std::arch::naked_asm!(
        call_rust!("mov rcx, [rbp]", call advance_accumulator),
        "jmp qword ptr [rip + {post}]",
        advance_accumulator = sym advance_accumulator,
        post = sym STEP_POST_AT,
    );
}

resume!(MID_DAY_RESUME: MID_DAY_NEXT_AT = MID_DAY_NEXT);

#[unsafe(naked)]
extern "C" fn mid_day_interval_stub() {
    std::arch::naked_asm!(
        "cmp rax, 4",
        "jbe 2f",
        "xor eax, eax",
        "2:",
        "lea rcx, [rip + {intervals}]",
        "mulss xmm6, dword ptr [rcx + rax*4]",
        "jmp qword ptr [rip + {next}]",
        intervals = sym MID_DAY_INTERVAL_BITS,
        next = sym MID_DAY_NEXT_AT,
    );
}

resume!(UNIT_SMOOTH_GRACE_RESUME: UNIT_GRACE_KEEP_AT = UNIT_SMOOTH_GRACE_KEEP, UNIT_GRACE_SKIP_AT = UNIT_SMOOTH_GRACE_SKIP);

#[unsafe(naked)]
extern "C" fn unit_smooth_grace_stub() {
    std::arch::naked_asm!(
        "push rax",
        "push rcx",
        "movsxd rax, dword ptr [rax + {speed}]",
        "cmp rax, 4",
        "jbe 2f",
        "xor eax, eax",
        "2:",
        "lea rcx, [rip + {smoothing}]",
        "cmp byte ptr [rcx + rax], 0",
        "pop rcx",
        "pop rax",
        "je 3f",
        "jmp qword ptr [rip + {keep}]",
        "3:",
        "jmp qword ptr [rip + {skip}]",
        speed = const world::SPEED,
        smoothing = sym UNIT_SMOOTHING,
        keep = sym UNIT_GRACE_KEEP_AT,
        skip = sym UNIT_GRACE_SKIP_AT,
    );
}

resume!(UNIT_SMOOTH_FLOOR_RESUME: UNIT_FLOOR_NEXT_AT = UNIT_SMOOTH_FLOOR_NEXT);

#[unsafe(naked)]
extern "C" fn unit_smooth_floor_stub() {
    std::arch::naked_asm!(
        "push rax",
        "push rcx",
        "movsxd rax, dword ptr [r9 + {speed}]",
        "cmp rax, 4",
        "jbe 2f",
        "xor eax, eax",
        "2:",
        "lea rcx, [rip + {smoothing}]",
        "cmp byte ptr [rcx + rax], 0",
        "pop rcx",
        "pop rax",
        "jne 3f",
        "movaps xmm1, xmm9",
        "jmp qword ptr [rip + {next}]",
        "3:",
        "xorps xmm1, xmm1",
        "jmp qword ptr [rip + {next}]",
        speed = const world::SPEED,
        smoothing = sym UNIT_SMOOTHING,
        next = sym UNIT_FLOOR_NEXT_AT,
    );
}

pub fn points() -> Vec<Point> {
    vec![
        Point::jump(
            STEP_THRESHOLD,
            "48 63 81 08 1e 00 00 48 8d 0d 19 c3 4d 01 f3 0f 10 04 81",
            "game speed daily step threshold",
            step_threshold_stub as *const () as usize,
            &STEP_THRESHOLD_RESUME,
        )
        .uses(&[world::SPEED, VANILLA_STEP_TABLE]),
        Point::keep(STEP_COMPARE, "0f 2f c2 0f 87 fd 01 00 00", "game speed daily step compare"),
        Point::jump(STEP_RESET, "89 b5 f8 1d 00 00", "game speed accumulator reset", step_reset_stub as *const () as usize, &STEP_RESET_RESUME)
            .uses(&[WORLD_ACCUM]),
        Point::keep(STEP_POST, "49 8b cc 49 8b 04 24", "game speed daily tick dispatch"),
        Point::keep(0x769f4busize, "48 63 86 08 1e 00 00", "mid-day frame check reads game speed").uses(&[world::SPEED]),
        Point::jump(
            MID_DAY_INTERVAL,
            "f3 41 0f 59 b4 85 18 3e c3 01",
            "mid-day frame interval by speed",
            mid_day_interval_stub as *const () as usize,
            &MID_DAY_RESUME,
        )
        .uses(&[VANILLA_MID_DAY_TABLE]),
        Point::keep(MID_DAY_NEXT, "48 8b 0d 36 f5 bd 01", "mid-day frame clock read"),
        Point::jump(
            UNIT_SMOOTH_GRACE,
            "83 b8 08 1e 00 00 03 0f 8f b1 00 00 00",
            "unit movement grace period speed check",
            unit_smooth_grace_stub as *const () as usize,
            &UNIT_SMOOTH_GRACE_RESUME,
        )
        .uses(&[world::SPEED]),
        Point::keep(UNIT_SMOOTH_GRACE_KEEP, "4c 39 b6 c0 00 00 00", "unit movement grace period kept"),
        Point::keep(UNIT_SMOOTH_GRACE_SKIP, "45 32 db 45 85 ff", "unit movement grace period skipped"),
        Point::jump(
            UNIT_SMOOTH_FLOOR,
            "41 83 b9 08 1e 00 00 04 7c 06 41 0f 28 c9 eb 03 0f 57 c9",
            "unit movement progress floor speed check",
            unit_smooth_floor_stub as *const () as usize,
            &UNIT_SMOOTH_FLOOR_RESUME,
        )
        .uses(&[world::SPEED]),
        Point::keep(UNIT_SMOOTH_FLOOR_NEXT, "0f 2f c8 76 0a", "unit movement progress floor compare"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::settings::DEFAULT_GAME_SPEEDS;

    #[test]
    fn vanilla_defaults_match_game_tables_and_fast_threshold() {
        let intervals = DEFAULT_GAME_SPEEDS.map(day_interval);
        assert_eq!(intervals, [2.0, 1.0, 0.5, 0.2, 0.0]);
        assert_eq!(DEFAULT_GAME_SPEEDS.map(is_fast_speed), [false, false, false, false, true]);
        // Boundary at 10 days/s: <= 10 stays smooth, > 10 switches to fast mode.
        assert!(!is_fast_speed(GameSpeed::days_per_second(10.0)));
        assert!(is_fast_speed(GameSpeed::days_per_second(10.1)));
        assert!(is_fast_speed(GameSpeed::days_per_second(12.0)));
        assert!(is_fast_speed(GameSpeed::Unlimited));
    }

    #[test]
    fn accumulator_carry_hits_exact_target_rate_at_60_fps_without_burst_catchup() {
        let dt = 1.0 / 60.0;
        for target_rate in [0.5f32, 1.0, 2.0, 5.0, 20.0, 40.0, 50.0, 60.0] {
            let threshold = 1.0 / target_rate;
            let mut accum = 0.0f32;
            let mut days = 0u32;
            // Simulate 10 seconds at 60 FPS (600 frames).
            for _ in 0..600 {
                accum += dt;
                if accum >= threshold {
                    days += 1;
                    accum = next_accumulator(accum, threshold);
                }
            }
            let expected = (target_rate * 10.0).round() as i32;
            assert!((days as i32 - expected).abs() <= 1, "rate {target_rate}: got {days} days in 10s, expected ~{expected}");
        }
        // Stall carry is clamped to at most one threshold (at most one extra
        // immediate follow-up day, never an unbounded backlog).
        assert_eq!(next_accumulator(5.0, 0.2), 0.2);
        // Unlimited always resets to zero.
        assert_eq!(next_accumulator(0.5, 0.0), 0.0);
        assert_eq!(next_accumulator(f32::NAN, 0.2), 0.0);
    }

    #[test]
    fn configure_updates_all_three_runtime_tables() {
        let custom = [
            GameSpeed::days_per_second(0.25),
            GameSpeed::days_per_second(10.0),
            GameSpeed::days_per_second(12.0),
            GameSpeed::Unlimited,
            GameSpeed::days_per_second(8.0),
        ];
        configure(custom);
        assert_eq!(DAY_INTERVAL_BITS.each_ref().map(|a| f32::from_bits(a.load(Ordering::Relaxed))), [4.0, 0.1, 1.0 / 12.0, 0.0, 0.125]);
        assert_eq!(MID_DAY_INTERVAL_BITS.each_ref().map(|a| f32::from_bits(a.load(Ordering::Relaxed))), [0.02, 0.02, 0.05, 0.05, 0.02]);
        assert_eq!(UNIT_SMOOTHING.each_ref().map(|a| a.load(Ordering::Relaxed)), [1, 1, 0, 0, 1]);
        // Restore defaults for other tests in the same process.
        configure(DEFAULT_GAME_SPEEDS);
    }
}
