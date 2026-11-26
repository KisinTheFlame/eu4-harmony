//! Responsive mouse-wheel map zoom and frame-rate-independent camera glide
//! (docs/game/interface/camera.md).
//!
//! Vanilla EU4's desktop mouse-wheel path (`0x109e750` -> `0x109ec30` ->
//! `0x109f6d0`) has four coupled defects that make zooming feel sluggish and
//! unpredictable:
//!
//! 1. `0x109e7bb` overwrites the frame's wheel delta (`mov [r14], eax`) instead
//!    of accumulating it, discarding all extra notches within the same frame.
//! 2. `0x109ec30` steps linearly in world height and scales the step by
//!    `clamp(|cur_y - goal_y| / 50, 1, 5)`, making low altitudes overly
//!    sensitive, high altitudes barely move on the first notches, and zoom
//!    distance depend on smoothing lag and frame rate.
//! 3. `0x109ec30` locks its cursor raycast target for 0.3 s (`+0x764`) and
//!    flies along a 3D line toward it rather than anchoring the hovered map
//!    point under the cursor, causing pointer drift and lateral movement even
//!    at minimum height.
//! 4. `0x109f6d0` glides with a slow Euler step (`6.5 * dt` by default),
//!    leaving a half-second tail and varying across frame rates.
//!
//! This module fixes all four by default:
//! - Accumulates all wheel events in a frame (`0x109e7bb`) and clears SDL's
//!   fractional wheel remainder (`0x23349e8`) on direction reversal.
//! - Replaces `0x109ec30` with a multiplicative height curve scaled by the
//!   game's `zoom_speed` setting, using `0x109d540` to keep the hovered map
//!   point pinned under the cursor on zoom-in (when `centered_zoom_in` is on)
//!   while keeping zoom-out strictly vertical as vanilla does.
//! - Wraps `0x109f6d0` to apply exact exponential smoothing
//!   (`1 - exp(-k * dt)`), with a snappy convergence rate while a wheel zoom
//!   is settling and vanilla's rate (made frame-rate-independent) for other
//!   camera moves such as menu sweeps and province jumps.
use crate::base::called_by_game;
use crate::drawing::SETTINGS;
use crate::native::memory::Memory;
use crate::native::objects::camera::camera as fields;
use crate::native::objects::campaign::globals::MAP;
use crate::native::objects::gui::globals::SCALE as GRAPHICS;
use crate::native::points::{Function, Point};
use crate::base::settings::{DEFAULT_ZOOM_SMOOTHING, DEFAULT_ZOOM_STEP, ZOOM_SMOOTHING_BOUNDS, ZOOM_STEP_BOUNDS};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Inside `0x109e750`: `mov [r14], eax` stores the wheel delta into the
/// frame's camera-input struct, overwriting any earlier wheel event in the
/// same frame. Changing `89` to `01` (`add [r14], eax`) accumulates every
/// notch in the frame.
const WHEEL_EVENT_STORE: usize = 0x109e7bb;
/// The camera's desktop mouse-wheel zoom (`rcx` camera, `xmm1` notches as f32,
/// `r8` cursor screen position `[i32; 2]`).
const WHEEL_ZOOM: usize = 0x109ec30;
/// The camera's per-frame glide toward its goal (`rcx` camera, `xmm1` dt).
const CAMERA_GLIDE: usize = 0x109f6d0;
/// Positions the camera's goal at `new_height` so that `world_pt` projects to
/// `cursor` (`rcx` camera, `rdx` `&[f32; 3]`, `r8` `&[f32; 2]`, `xmm3` height,
/// stack `u8` immediate).
const FOCUS_AT_CURSOR_RVA: usize = 0x109d540;
/// Clamps both the current and goal camera positions onto the map (`rcx` camera).
const CLAMP_ON_MAP_RVA: usize = 0x109d0e0;
/// Lowest allowed camera height at a position (`rcx` camera, `rdx` `&[f32; 3]`,
/// `r8` map) -> `f32`.
const MIN_GROUND_HEIGHT_RVA: usize = 0x109cd00;
/// Builds a world-space ray from NDC (`rcx` view at `camera + 0x30`, `rdx`
/// `&[f32; 3]` ndc, `r8` `&mut [f32; 3]` out dir, `r9` `&mut [f32; 3]` out origin).
const SCREEN_RAY_RVA: usize = 0x1581ff0;

/// Camera fields on `CEU4Settings` (`[SETTINGS]`).
const SETTINGS_ZOOM_SPEED: usize = 0xd0;
const SETTINGS_CAMERA_SPEED: usize = 0x278;
const SETTINGS_CAMERA_TILT: usize = 0x27c;
const SETTINGS_CENTERED_ZOOM_IN: usize = 0x280;

/// Viewport size (`[i32; 2]` width, height) on `[GRAPHICS]`.
const GRAPHICS_VIEWPORT: usize = 0x488;

/// Map ready flag (`u8`) on `[MAP]`.
const MAP_READY: usize = 0x30;

/// Camera fields beyond `objects::camera::camera`.
const CAMERA_VIEW: usize = 0x30;
const CAMERA_MOVING: usize = 0x6f4;
const CAMERA_SPEED_MULT: usize = 0x774;

/// SDL 2.0.4's signed 16-bit vertical wheel delta accumulator (`0x173ac20`).
const SDL_WHEEL_ACCUM: usize = 0x23349e8;

/// Vanilla's glide scale constant in `0x109f6d0` (`[0x1dc1530]`).
const VANILLA_GLIDE_SCALE: f32 = 0.13;
/// Vanilla's tilt multiplier (`[0x1dc1558]`).
const CAMERA_TILT_SCALE: f32 = 0.3;
/// Sea-level plane height used for cursor raycasting (matches `0x10a3e50` and
/// `0x109cd00`).
const MAP_PLANE_Y: f32 = 19.0;
/// Maximum cursor drift (in pixels per axis) that still reuses the current
/// zoom-in world anchor across rapid consecutive wheel ticks.
const ANCHOR_CURSOR_TOLERANCE: i32 = 2;
/// Maximum goal displacement (in map units per axis) before treating the
/// camera as moved by something other than our zoom (panning or `look_at`).
const GOAL_MATCH_TOLERANCE: f32 = 0.5;

static ZOOM_SMOOTHING: AtomicU32 = AtomicU32::new(DEFAULT_ZOOM_SMOOTHING);
static ZOOM_STEP: AtomicU32 = AtomicU32::new(DEFAULT_ZOOM_STEP);

/// Updates the wheel zoom's smoothing (`0..=100`) and per-notch step
/// (`105..=200` hundredths of the height ratio) from framework settings.
pub fn configure(smoothing: u32, step: u32) {
    let smoothing = smoothing.clamp(*ZOOM_SMOOTHING_BOUNDS.start(), *ZOOM_SMOOTHING_BOUNDS.end());
    let step = step.clamp(*ZOOM_STEP_BOUNDS.start(), *ZOOM_STEP_BOUNDS.end());
    ZOOM_SMOOTHING.store(smoothing, Ordering::Relaxed);
    ZOOM_STEP.store(step, Ordering::Relaxed);
}

fn current_smoothing() -> u32 {
    ZOOM_SMOOTHING.load(Ordering::Relaxed)
}

fn current_step() -> u32 {
    ZOOM_STEP.load(Ordering::Relaxed)
}

/// Exponential glide rate (1/s) while a wheel zoom is settling for a
/// `0..=100` smoothing setting (`0` snaps in one frame; `1..=100` maps
/// linearly to a `2 ms..=200 ms` time constant, so `50%` is `100 ms`).
pub fn zoom_glide_rate(smoothing: u32) -> f32 {
    let s = smoothing.clamp(*ZOOM_SMOOTHING_BOUNDS.start(), *ZOOM_SMOOTHING_BOUNDS.end());
    if s == 0 {
        f32::INFINITY
    } else {
        500.0 / s as f32
    }
}

/// Multiplicative height ratio per wheel notch for a `105..=200` step setting.
pub fn zoom_step_ratio(step: u32) -> f32 {
    step.clamp(*ZOOM_STEP_BOUNDS.start(), *ZOOM_STEP_BOUNDS.end()) as f32 / 100.0
}

type WheelZoom = unsafe extern "C" fn(usize, f32, *const [i32; 2]);
type CameraGlide = unsafe extern "C" fn(usize, f32);
type FocusAtCursor = unsafe extern "C" fn(usize, *const [f32; 3], *const [f32; 2], f32, u8);
type ClampOnMap = unsafe extern "C" fn(usize);
type MinGroundHeight = unsafe extern "C" fn(usize, *const [f32; 3], usize) -> f32;
type ScreenRay = unsafe extern "C" fn(usize, *const [f32; 3], *mut [f32; 3], *mut [f32; 3]);

static ORIGINAL_WHEEL_ZOOM: Function<WheelZoom> = Function::new();
static ORIGINAL_GLIDE: Function<CameraGlide> = Function::new();
static FOCUS_AT_CURSOR: Function<FocusAtCursor> = Function::new();
static CLAMP_ON_MAP: Function<ClampOnMap> = Function::new();
static MIN_GROUND_HEIGHT: Function<MinGroundHeight> = Function::new();
static SCREEN_RAY: Function<ScreenRay> = Function::new();

pub fn points() -> Vec<Point> {
    vec![
        // mov [r14], eax -> add [r14], eax: accumulate every wheel notch in a frame.
        Point::write(WHEEL_EVENT_STORE, "41 89 06", "accumulate mouse wheel events in a frame", "41 01 06"),
        Point::replace(WHEEL_ZOOM, "4c 8b dc 49 89 5b 10 57 48 81 ec e0 00 00 00", "camera wheel zoom", wheel_zoom as WheelZoom, &ORIGINAL_WHEEL_ZOOM),
        Point::replace(CAMERA_GLIDE, "48 8b c4 48 89 70 10 48 89 78 18 4c 89 60 20", "camera glide", glide as CameraGlide, &ORIGINAL_GLIDE),
        Point::keep(FOCUS_AT_CURSOR_RVA, "48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18", "camera focus at screen cursor").recorded(FOCUS_AT_CURSOR.slot()),
        Point::keep(CLAMP_ON_MAP_RVA, "48 89 5c 24 08 57 48 83 ec 20 48 8b 1d e7 2d 2a 01", "clamp camera on map").recorded(CLAMP_ON_MAP.slot()),
        Point::keep(MIN_GROUND_HEIGHT_RVA, "48 83 ec 28 f3 0f 10 42 08 4d 8b c8", "minimum camera height above ground").recorded(MIN_GROUND_HEIGHT.slot()),
        Point::keep(SCREEN_RAY_RVA, "48 89 5c 24 08 57 48 83 ec 60 48 8d 99 80 01 00 00", "camera view ray from screen point").recorded(SCREEN_RAY.slot()),
        // Field and global checks inside the camera functions and SDL's WndProc.
        Point::keep(0x109ee1a, "48 8b 05 c7 cf 38 01 f3 0f 59 88 d0 00 00 00", "settings zoom_speed").uses(&[SETTINGS, SETTINGS_ZOOM_SPEED]),
        Point::keep(0x109ee4f, "80 b8 80 02 00 00 00", "settings centered_zoom_in").uses(&[SETTINGS_CENTERED_ZOOM_IN]),
        Point::keep(0x109eead, "f3 0f 10 80 7c 02 00 00", "settings camera_tilt").uses(&[SETTINGS_CAMERA_TILT]),
        Point::keep(0x109f881, "f3 0f 5c 80 78 02 00 00", "settings camera_speed").uses(&[SETTINGS_CAMERA_SPEED]),
        Point::keep(0x109f89f, "f3 0f 59 87 74 07 00 00", "camera speed multiplier").uses(&[CAMERA_SPEED_MULT]),
        Point::keep(0x109fab9, "c6 87 f4 06 00 00 01", "camera moving flag").uses(&[CAMERA_MOVING]),
        Point::keep(0x109d939, "48 8b 05 78 bb 2a 01", "graphics context for camera zoom").uses(&[GRAPHICS]),
        Point::keep(0x109d95e, "48 8b 88 88 04 00 00", "graphics viewport size").uses(&[GRAPHICS_VIEWPORT]),
        Point::keep(0x109d992, "48 8d 4b 30", "camera view offset").uses(&[CAMERA_VIEW]),
        Point::keep(0x109d8dd, "41 80 78 30 00", "map ready flag").uses(&[MAP_READY]),
        Point::keep(0x173b76a, "66 03 05 77 92 bf 00 66 89 05 70 92 bf 00", "SDL vertical wheel accumulator").uses(&[SDL_WHEEL_ACCUM]),
    ]
}

/// A pinned map point under the mouse cursor across consecutive zoom-in ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    cursor: [i32; 2],
    world_pt: [f32; 3],
}

/// Tracks active wheel-zoom smoothing and cursor anchoring for one camera.
#[derive(Default)]
struct ZoomState {
    camera: usize,
    zoom_active: bool,
    last_goal: [f32; 3],
    anchor: Option<Anchor>,
}

impl ZoomState {
    fn clear(&mut self) {
        self.zoom_active = false;
        self.anchor = None;
    }

    fn matches_goal(&self, camera: usize, goal: [f32; 3]) -> bool {
        self.zoom_active
            && self.camera == camera
            && (goal[0] - self.last_goal[0]).abs() <= GOAL_MATCH_TOLERANCE
            && (goal[1] - self.last_goal[1]).abs() <= GOAL_MATCH_TOLERANCE
            && (goal[2] - self.last_goal[2]).abs() <= GOAL_MATCH_TOLERANCE
    }

    /// Reuses the cached zoom-in world anchor when the camera's goal has not
    /// been moved externally and the cursor stayed on the same spot.
    fn cached_anchor(&self, camera: usize, goal: [f32; 3], cursor: [i32; 2]) -> Option<[f32; 3]> {
        if !self.matches_goal(camera, goal) {
            return None;
        }
        let anchor = self.anchor?;
        let steady = (cursor[0] - anchor.cursor[0]).abs() <= ANCHOR_CURSOR_TOLERANCE && (cursor[1] - anchor.cursor[1]).abs() <= ANCHOR_CURSOR_TOLERANCE;
        steady.then_some(anchor.world_pt)
    }

    fn record_zoom(&mut self, camera: usize, goal: [f32; 3], anchor: Option<Anchor>) {
        self.camera = camera;
        self.zoom_active = true;
        self.last_goal = goal;
        self.anchor = anchor;
    }

    /// Whether the upcoming glide belongs to our active wheel zoom. Clears
    /// the zoom state if another move changed `goal`.
    fn check_glide(&mut self, camera: usize, goal: [f32; 3]) -> bool {
        if self.matches_goal(camera, goal) {
            true
        } else {
            if self.camera == camera {
                self.clear();
            }
            false
        }
    }

    /// Updates tracked state after `0x109f6d0` runs: keeps `last_goal` in sync
    /// with any map-edge clamp applied by the glide, and clears `zoom_active`
    /// once the camera settles.
    fn after_glide(&mut self, camera: usize, dt: f32, moving: bool, goal: [f32; 3]) {
        if !self.zoom_active || self.camera != camera {
            return;
        }
        if dt > 0.0 && !moving {
            self.clear();
        } else {
            self.last_goal = goal;
        }
    }
}

static STATE: Mutex<ZoomState> = Mutex::new(ZoomState { camera: 0, zoom_active: false, last_goal: [0.0; 3], anchor: None });

fn state() -> std::sync::MutexGuard<'static, ZoomState> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Clears SDL 2.0.4's fractional vertical wheel accumulator when the player
/// reverses scroll direction, removing the deadzone on precision touchpads
/// and high-resolution wheels.
pub fn before_sdl_wheel(delta: i16) {
    if delta == 0 {
        return;
    }
    let Some(m) = crate::native::memory() else { return };
    let at = m.at(SDL_WHEEL_ACCUM);
    if let Ok(raw) = m.u16(at) {
        let accum = raw as i16;
        if should_reset_wheel_accum(accum, delta) {
            let _ = m.write(at, &0i16.to_le_bytes());
        }
    }
}

fn should_reset_wheel_accum(accum: i16, delta: i16) -> bool {
    (accum > 0 && delta < 0) || (accum < 0 && delta > 0)
}

/// Target camera height after `notches` wheel steps (positive zooms in,
/// negative zooms out), using a multiplicative scale driven by `notch_ratio`
/// and the game's `zoom_speed` setting.
pub fn target_height(current_goal_y: f32, notches: f32, zoom_speed: f32, notch_ratio: f32, min_height: f32, max_height: f32) -> f32 {
    if !current_goal_y.is_finite() || !notches.is_finite() || notches == 0.0 || !notch_ratio.is_finite() || notch_ratio <= 1.0 {
        return current_goal_y;
    }
    let low = min_height.max(1.0);
    let high = max_height.max(low);
    let speed = if zoom_speed.is_finite() { (zoom_speed / 50.0).clamp(0.2, 2.5) } else { 1.0 };
    let step_log = notch_ratio.ln() * speed;
    let clamped_start = current_goal_y.clamp(low, high);
    (clamped_start * (-step_log * notches).exp()).clamp(low, high)
}

/// Converts a frame time `dt` into the argument `0x109f6d0` needs so that its
/// linear `k_vanilla * dt` equals the exact exponential factor
/// `1 - exp(-k_desired * dt)` (or `1.0` when `k_desired` is infinite).
pub fn effective_glide_dt(dt: f32, k_vanilla: f32, k_desired: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 || !k_vanilla.is_finite() || k_vanilla <= 1e-6 || k_desired.is_nan() || k_desired <= 0.0 {
        return dt;
    }
    if k_desired.is_infinite() {
        // `0x109f6d0` clamps `k_vanilla * dt` to `1.0`; `2.0 / k_vanilla`
        // guarantees it reaches `1.0` without float-rounding shortfall.
        return 2.0 / k_vanilla;
    }
    let alpha = 1.0 - (-k_desired * dt).exp();
    alpha / k_vanilla
}

/// Intersects a world ray with the horizontal map plane `y = plane_y`.
fn ray_plane_hit(origin: [f32; 3], dir: [f32; 3], plane_y: f32) -> Option<[f32; 3]> {
    if !dir[1].is_finite() || dir[1] >= -1e-4 {
        return None;
    }
    let t = (plane_y - origin[1]) / dir[1];
    if !t.is_finite() || t <= 0.0 || t > 20_000.0 {
        return None;
    }
    let hit = [origin[0] + t * dir[0], plane_y, origin[2] + t * dir[2]];
    hit.iter().all(|v| v.is_finite()).then_some(hit)
}

fn read_vec3(m: &Memory, at: usize) -> Option<[f32; 3]> {
    Some([f32::from_bits(m.u32(at).ok()?), f32::from_bits(m.u32(at + 4).ok()?), f32::from_bits(m.u32(at + 8).ok()?)])
}

fn write_vec3(m: &Memory, at: usize, v: [f32; 3]) {
    let mut bytes = [0u8; 12];
    bytes[0..4].copy_from_slice(&v[0].to_le_bytes());
    bytes[4..8].copy_from_slice(&v[1].to_le_bytes());
    bytes[8..12].copy_from_slice(&v[2].to_le_bytes());
    let _ = m.write(at, &bytes);
}

fn raycast_cursor(m: &Memory, camera: usize, cursor: [i32; 2]) -> Option<[f32; 3]> {
    let graphics = m.ptr(m.at(GRAPHICS)).ok()?;
    if graphics == 0 {
        return None;
    }
    let width = m.i32(graphics + GRAPHICS_VIEWPORT).ok()?;
    let height = m.i32(graphics + GRAPHICS_VIEWPORT + 4).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }
    let ndc = [(2.0 * cursor[0] as f32) / width as f32 - 1.0, 1.0 - (2.0 * cursor[1] as f32) / height as f32, 0.0];
    let mut dir = [0.0f32; 3];
    let mut origin = [0.0f32; 3];
    unsafe { SCREEN_RAY.get()(camera + CAMERA_VIEW, &ndc, &mut dir, &mut origin) };
    ray_plane_hit(origin, dir, MAP_PLANE_Y)
}

#[called_by_game]
unsafe extern "C" fn wheel_zoom(camera: usize, notches: f32, cursor: *const [i32; 2]) {
    crate::base::guarded("camera wheel zoom", || {
        if camera == 0 || cursor.is_null() || !notches.is_finite() || notches == 0.0 {
            return;
        }
        let m = crate::native::hooked_memory();
        let Ok(map) = m.ptr(m.at(MAP)) else { return };
        if map == 0 || m.u8(map + MAP_READY).unwrap_or(0) == 0 {
            return;
        }
        let Ok(settings) = m.ptr(m.at(SETTINGS)) else { return };
        if settings == 0 {
            return;
        }
        let Some(goal_pos) = read_vec3(&m, camera + fields::GOAL_POSITION) else { return };
        let Ok(max_h_i32) = m.i32(camera + fields::MAX_HEIGHT) else { return };
        let zoom_speed = m.u32(settings + SETTINGS_ZOOM_SPEED).map(f32::from_bits).unwrap_or(50.0);
        let centered_zoom_in = m.u8(settings + SETTINGS_CENTERED_ZOOM_IN).unwrap_or(1) != 0;
        let camera_tilt = m.u32(settings + SETTINGS_CAMERA_TILT).map(f32::from_bits).unwrap_or(100.0);

        let min_h = unsafe { MIN_GROUND_HEIGHT.get()(camera, &goal_pos, map) };
        let new_height = target_height(goal_pos[1], notches, zoom_speed, zoom_step_ratio(current_step()), min_h, max_h_i32 as f32);
        if (new_height - goal_pos[1]).abs() < 1e-4 {
            return;
        }

        let cursor_px = unsafe { *cursor };
        let mut used_anchor = None;
        if notches > 0.0 && centered_zoom_in {
            let world_pt = {
                let cached = state().cached_anchor(camera, goal_pos, cursor_px);
                cached.or_else(|| raycast_cursor(&m, camera, cursor_px))
            };
            if let Some(world_pt) = world_pt {
                let cursor_f32 = [cursor_px[0] as f32, cursor_px[1] as f32];
                unsafe { FOCUS_AT_CURSOR.get()(camera, &world_pt, &cursor_f32, new_height, 0) };
                used_anchor = Some(Anchor { cursor: cursor_px, world_pt });
            }
        }

        if used_anchor.is_none() {
            let next_pos = [goal_pos[0], new_height, goal_pos[2]];
            let next_look = [goal_pos[0], 0.0, goal_pos[2] + camera_tilt * CAMERA_TILT_SCALE];
            write_vec3(&m, camera + fields::GOAL_POSITION, next_pos);
            write_vec3(&m, camera + fields::GOAL_LOOK_AT, next_look);
            unsafe { CLAMP_ON_MAP.get()(camera) };
        }

        // Entering the nation-selection lobby leaves `+0x774` slowly ramping from
        // the main-menu flyover speed (~0.1) back toward 1.0 at 0.1/s. Once the
        // player zooms, restore full responsiveness immediately.
        if m.u32(camera + CAMERA_SPEED_MULT).map(f32::from_bits).is_ok_and(|mult| mult < 1.0) {
            let _ = m.write(camera + CAMERA_SPEED_MULT, &1.0f32.to_le_bytes());
        }

        if let Some(clamped_goal) = read_vec3(&m, camera + fields::GOAL_POSITION) {
            state().record_zoom(camera, clamped_goal, used_anchor);
        }
    });
}

fn prepare_glide_dt(camera: usize, dt: f32) -> f32 {
    if camera == 0 || !dt.is_finite() || dt <= 0.0 {
        return dt;
    }
    let m = crate::native::hooked_memory();
    let Ok(settings) = m.ptr(m.at(SETTINGS)) else { return dt };
    if settings == 0 {
        return dt;
    }
    let Some(goal) = read_vec3(&m, camera + fields::GOAL_POSITION) else { return dt };
    let zoom_active = state().check_glide(camera, goal);
    let camera_speed = m.u32(settings + SETTINGS_CAMERA_SPEED).map(f32::from_bits).unwrap_or(50.0);
    let speed_mult = m.u32(camera + CAMERA_SPEED_MULT).map(f32::from_bits).unwrap_or(1.0);
    let k_vanilla = (100.0 - camera_speed) * speed_mult * VANILLA_GLIDE_SCALE;
    let k_desired = if zoom_active { zoom_glide_rate(current_smoothing()) } else { k_vanilla };
    effective_glide_dt(dt, k_vanilla, k_desired)
}

fn finish_glide(camera: usize, dt: f32) {
    if camera == 0 {
        return;
    }
    let m = crate::native::hooked_memory();
    let moving = m.u8(camera + CAMERA_MOVING).unwrap_or(0) != 0;
    if let Some(goal) = read_vec3(&m, camera + fields::GOAL_POSITION) {
        state().after_glide(camera, dt, moving, goal);
    }
}

#[called_by_game]
unsafe extern "C" fn glide(camera: usize, dt: f32) {
    let effective_dt = crate::base::guarded("camera glide", || prepare_glide_dt(camera, dt));
    unsafe { ORIGINAL_GLIDE.get()(camera, effective_dt) };
    crate::base::guarded("camera glide after", || finish_glide(camera, dt));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplicative_height_is_associative_invertible_and_clamped() {
        let start = 1000.0;
        let ratio = zoom_step_ratio(DEFAULT_ZOOM_STEP);
        // Two single notches equal one double-notch frame.
        let one = target_height(start, 1.0, 50.0, ratio, 50.0, 3000.0);
        let two_steps = target_height(one, 1.0, 50.0, ratio, 50.0, 3000.0);
        let one_batch = target_height(start, 2.0, 50.0, ratio, 50.0, 3000.0);
        assert!((two_steps - one_batch).abs() < 1e-3, "{two_steps} vs {one_batch}");
        // Zooming in then out by the same count returns to the exact height.
        let back = target_height(one_batch, -2.0, 50.0, ratio, 50.0, 3000.0);
        assert!((back - start).abs() < 1e-3, "{back} vs {start}");
        // Respects bounds and does not move past min/max.
        assert_eq!(target_height(50.0, 1.0, 50.0, ratio, 50.0, 3000.0), 50.0);
        assert_eq!(target_height(3000.0, -1.0, 50.0, ratio, 50.0, 3000.0), 3000.0);
        // Larger zoom step and faster zoom_speed both move farther per notch.
        let fine = target_height(1000.0, 1.0, 50.0, zoom_step_ratio(110), 50.0, 3000.0);
        let wide = target_height(1000.0, 1.0, 50.0, zoom_step_ratio(180), 50.0, 3000.0);
        assert!(wide < one && one < fine);
        let slow = target_height(1000.0, 1.0, 25.0, ratio, 50.0, 3000.0);
        let fast = target_height(1000.0, 1.0, 100.0, ratio, 50.0, 3000.0);
        assert!(fast < one && one < slow);
        // Zero and non-finite inputs leave height unchanged.
        assert_eq!(target_height(500.0, 0.0, 50.0, ratio, 50.0, 3000.0), 500.0);
        assert_eq!(target_height(500.0, f32::NAN, 50.0, ratio, 50.0, 3000.0), 500.0);
    }

    #[test]
    fn exponential_glide_is_frame_rate_independent_and_respects_smoothing() {
        let k_vanilla = 6.5;
        let k_default = zoom_glide_rate(DEFAULT_ZOOM_SMOOTHING);
        // One 100 ms step equals ten 10 ms steps.
        let mut x_one = 1000.0f32;
        let dt_big = effective_glide_dt(0.1, k_vanilla, k_default);
        x_one += (0.0 - x_one) * (k_vanilla * dt_big).clamp(0.0, 1.0);

        let mut x_ten = 1000.0f32;
        for _ in 0..10 {
            let dt_small = effective_glide_dt(0.01, k_vanilla, k_default);
            x_ten += (0.0 - x_ten) * (k_vanilla * dt_small).clamp(0.0, 1.0);
        }
        assert!((x_one - x_ten).abs() < 1e-2, "{x_one} vs {x_ten}");
        // Smoothing is monotonic: 0% snaps in 1 frame, higher % lowers the rate.
        let dt_off = effective_glide_dt(0.016, k_vanilla, zoom_glide_rate(0));
        assert_eq!((k_vanilla * dt_off).clamp(0.0, 1.0), 1.0);
        // Even with a lowered lobby `speed_mult` (e.g. 0.1 -> k_vanilla = 0.65),
        // the resulting per-frame factor still matches full-speed smoothing.
        let k_low = k_vanilla * 0.1;
        let alpha_full = (k_vanilla * effective_glide_dt(0.016, k_vanilla, k_default)).clamp(0.0, 1.0);
        let alpha_low = (k_low * effective_glide_dt(0.016, k_low, k_default)).clamp(0.0, 1.0);
        assert!((alpha_full - alpha_low).abs() < 1e-5, "{alpha_full} vs {alpha_low}");
        assert!(zoom_glide_rate(25) > k_default);
        assert!(k_default > zoom_glide_rate(100));
        // Zero dt and zero vanilla rate pass through untouched.
        assert_eq!(effective_glide_dt(0.0, k_vanilla, k_default), 0.0);
        assert_eq!(effective_glide_dt(0.016, 0.0, k_default), 0.016);
    }

    #[test]
    fn ray_plane_intersection_hits_ground_and_rejects_sky() {
        let hit = ray_plane_hit([100.0, 119.0, 200.0], [0.0, -1.0, 0.5], 19.0).unwrap();
        assert!((hit[0] - 100.0).abs() < 1e-4);
        assert!((hit[1] - 19.0).abs() < 1e-4);
        assert!((hit[2] - 250.0).abs() < 1e-4);
        assert!(ray_plane_hit([100.0, 119.0, 200.0], [0.0, 0.1, 1.0], 19.0).is_none());
        assert!(ray_plane_hit([100.0, 119.0, 200.0], [0.0, 0.0, 1.0], 19.0).is_none());
    }

    #[test]
    fn zoom_state_reuses_steady_anchor_and_clears_on_move_or_settle() {
        let mut s = ZoomState::default();
        let cam = 0x1000;
        let goal = [500.0, 800.0, 600.0];
        let anchor = Anchor { cursor: [960, 540], world_pt: [500.0, 19.0, 630.0] };
        s.record_zoom(cam, goal, Some(anchor));
        // Steady cursor reuses anchor; moving cursor > 2 px invalidates it.
        assert_eq!(s.cached_anchor(cam, goal, [962, 539]), Some(anchor.world_pt));
        assert_eq!(s.cached_anchor(cam, goal, [965, 540]), None);
        // Active glide with matching goal stays boosted.
        assert!(s.check_glide(cam, goal));
        // Zero-dt main-thread glide does not clear zoom_active even when not moving.
        s.after_glide(cam, 0.0, false, goal);
        assert!(s.zoom_active);
        // Settling on a real frame clears zoom_active and anchor.
        s.after_glide(cam, 0.016, false, goal);
        assert!(!s.zoom_active && s.anchor.is_none());
        // External camera jump clears zoom_active immediately.
        s.record_zoom(cam, goal, Some(anchor));
        assert!(!s.check_glide(cam, [900.0, 800.0, 600.0]));
        assert!(!s.zoom_active);
    }

    #[test]
    fn wheel_accumulator_resets_only_on_direction_reversal() {
        assert!(should_reset_wheel_accum(40, -40));
        assert!(should_reset_wheel_accum(-40, 40));
        assert!(!should_reset_wheel_accum(40, 40));
        assert!(!should_reset_wheel_accum(0, -40));
        assert!(!should_reset_wheel_accum(-40, 0));
    }
}
