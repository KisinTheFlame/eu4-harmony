//! The C ABI between the EU4 Harmony core and extension DLLs. Extensions
//! never use these types directly; the Rust SDK wraps them.
//!
//! Every table starts with its own `size` so fields can only be appended;
//! each side uses only what the other's table has, and the versions change
//! only when something that exists breaks (ADR 0014).
//! No struct is passed by value: Win64 passes structs over 8 bytes by
//! reference, which would make the ABI depend on how temporaries are laid out.
//! Every host function returns a status code; results go through out-pointers.
//! List queries use the two-call pattern: with too small a buffer they return
//! [`BUFFER_TOO_SMALL`] and store the needed count in `len`. A command, which
//! cannot be run twice to ask for its output again, hands that to a
//! [`TextSink`] instead.
use std::ffi::c_void;

/// Value of `api_version` in the manifest that this ABI implements.
pub const API_VERSION: &str = "0.1";
/// Numeric form checked at runtime by both sides.
pub const ABI_VERSION: u32 = 1;

/// Exported by every extension DLL.
pub const ENTRY_SYMBOL: &str = "eu4h_extension_main";
/// Exported by every extension DLL: the `ABI_VERSION` it was built for, a u32.
pub const ABI_SYMBOL: &str = "eu4h_extension_abi";
/// `eu4h_extension_main(host, out) -> status`; [`OK`] means success.
pub type EntryFn = unsafe extern "C" fn(host: *const HostApi, out: *mut ExtensionVTable) -> i32;

// Status codes.
pub const OK: i32 = 0;
/// No campaign is running, or one is being loaded.
pub const NOT_IN_GAME: i32 = 1;
/// The game state cannot be read right now (for example visibility is not yet
/// refreshed for the new day). Retry on a later frame.
pub const NOT_READY: i32 = 2;
pub const NOT_FOUND: i32 = 3;
/// A command was refused; the reason is available from `last_error`.
pub const REJECTED: i32 = 4;
pub const BUFFER_TOO_SMALL: i32 = 5;
pub const INVALID_ARGUMENT: i32 = 6;
/// The capability is not available (unsupported state, or disabled after an
/// unexpected game-side failure).
pub const UNAVAILABLE: i32 = 7;
/// Called outside a callback or from another thread.
pub const WRONG_CONTEXT: i32 = 8;
pub const INTERNAL: i32 = 9;

/// Capabilities usable at any time and from any thread.
#[repr(C)]
pub struct HostApi {
    pub size: usize,
    pub abi_version: u32,
    /// Opaque, passed back to every host function.
    pub host: *mut c_void,
    /// Appends one line to this extension's log.
    pub log: unsafe extern "C" fn(host: *mut c_void, text: *const u8, len: usize),
}

/// Country identity. Stable within a session; compare for equality only.
pub type CountryId = u64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// Increases whenever a different campaign state is loaded or started.
    pub session: u64,
    /// Game date as a monotonic number; the same value throughout one day.
    pub date: i32,
    pub paused: u8,
    /// 1 when visibility (fog of war) reflects the current date.
    pub visibility_current: u8,
    pub _reserved: u16,
    pub player: CountryId,
}

pub const PROVINCE_LAND: u32 = 1;
pub const PROVINCE_LAKE: u32 = 2;
pub const PROVINCE_IMPASSABLE: u32 = 4;
/// Not covered by fog of war. Only set while `visibility_current` is 1.
pub const PROVINCE_VISIBLE: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Province {
    pub id: u32,
    pub flags: u32,
    /// 0 when unowned.
    pub owner: CountryId,
    /// 0 when unowned.
    pub controller: CountryId,
}

/// Neighbor reachable over land, possibly across a strait.
pub const NEIGHBOR_LAND: u32 = 1;
/// The land connection crosses a strait.
pub const NEIGHBOR_STRAIT: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Neighbor {
    pub to: u32,
    pub flags: u32,
}

pub const ARMY_LAND: u32 = 1;
pub const ARMY_IN_BATTLE: u32 = 2;
pub const ARMY_SIEGING: u32 = 4;
pub const ARMY_RETREATING: u32 = 8;
pub const ARMY_EXILED: u32 = 0x10;
pub const ARMY_MOVEMENT_LOCKED: u32 = 0x20;
/// Attached to another army.
pub const ARMY_ATTACHED: u32 = 0x40;
/// Other armies are attached to this one.
pub const ARMY_HAS_FOLLOWERS: u32 = 0x80;
/// Running a native task (for example hunting rebels) or otherwise not idle.
pub const ARMY_BUSY: u32 = 0x100;

pub const ROUTE_NONE: u32 = 0;
/// Set by the player or by the game itself.
pub const ROUTE_EXTERNAL: u32 = 1;
/// Set by the calling extension and not changed since.
pub const ROUTE_OWN: u32 = 2;
/// Set by another extension.
pub const ROUTE_OTHER_EXTENSION: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Army {
    /// Full 64-bit army ID; survives save and load.
    pub id: u64,
    pub owner: CountryId,
    pub province: u32,
    pub flags: u32,
    pub regiments: u32,
    /// Provinces left on the current route; 0 when not moving.
    pub route_len: u32,
    /// Last province of the current route; 0 when not moving.
    pub destination: u32,
    /// One of the `ROUTE_*` values.
    pub route_source: u32,
}

pub const CLICK_LEFT: u32 = 1;
pub const CLICK_RIGHT: u32 = 2;
pub const MODIFIER_SHIFT: u32 = 1;
pub const MODIFIER_CTRL: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapColor {
    pub province: u32,
    /// Bytes R, G, B, A.
    pub rgba: [u8; 4],
}

/// Where the map camera looks and how high it is. A map position is a pixel
/// of the province map (`map/provinces.bmp`): `x` from its left edge, `y`
/// from its bottom edge, as `map/positions.txt` writes them; a height is in
/// the same unit, above the map's base plane.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Camera {
    /// The map position at the middle of the view.
    pub x: u32,
    pub y: u32,
    pub height: u32,
    /// The province at that position; 0 when there is none.
    pub province: u32,
    /// The lowest and the highest the camera may be.
    pub min_height: u32,
    pub max_height: u32,
}

/// Receives text a host function hands back (UTF-8, `len` bytes at `text`),
/// with the `user` pointer the caller passed it. It is called on the game
/// thread, before the host function returns; the text is valid only for the
/// call. It must not unwind (no panic or exception may leave it), and it may
/// call host functions (ADR 0019).
pub type TextSink = unsafe extern "C" fn(user: *mut c_void, text: *const u8, len: usize);

/// Kinds of regiment and ship, as [`GameApi::recruit`] takes them, in the
/// game's own order (the order of `recruit` in a province's snapshot).
pub const UNIT_INFANTRY: u32 = 0;
pub const UNIT_CAVALRY: u32 = 1;
pub const UNIT_ARTILLERY: u32 = 2;
pub const UNIT_HEAVY_SHIP: u32 = 3;
pub const UNIT_LIGHT_SHIP: u32 = 4;
pub const UNIT_GALLEY: u32 = 5;
pub const UNIT_TRANSPORT: u32 = 6;

/// Capabilities valid only for the duration of one callback, on the game thread.
#[repr(C)]
pub struct GameApi {
    pub size: usize,
    pub host: *mut c_void,
    /// Copies the message of the last failed call into `buf` (UTF-8) and
    /// returns its full length.
    pub last_error: unsafe extern "C" fn(host: *mut c_void, buf: *mut u8, cap: usize) -> usize,
    pub session: unsafe extern "C" fn(host: *mut c_void, out: *mut Session) -> i32,
    /// Three-letter tag of a country (for example `CAS`).
    pub country_tag: unsafe extern "C" fn(host: *mut c_void, country: CountryId, out: *mut [u8; 3]) -> i32,
    /// Whether the two countries are at war, or either is a rebel faction.
    pub countries_hostile: unsafe extern "C" fn(host: *mut c_void, a: CountryId, b: CountryId, out: *mut u8) -> i32,
    /// Every province, indexed by ID (index 0 is a placeholder with no flags).
    pub provinces: unsafe extern "C" fn(host: *mut c_void, buf: *mut Province, cap: usize, len: *mut usize) -> i32,
    /// Geographic land neighbors of a province.
    pub province_neighbors:
        unsafe extern "C" fn(host: *mut c_void, province: u32, buf: *mut Neighbor, cap: usize, len: *mut usize) -> i32,
    /// The player's own army.
    pub army: unsafe extern "C" fn(host: *mut c_void, id: u64, out: *mut Army) -> i32,
    /// All armies and fleets of the player.
    pub own_armies: unsafe extern "C" fn(host: *mut c_void, buf: *mut Army, cap: usize, len: *mut usize) -> i32,
    /// Land armies in a province the player can see. [`NOT_READY`] while
    /// visibility is not current; an empty list for provinces under fog.
    pub province_armies:
        unsafe extern "C" fn(host: *mut c_void, province: u32, buf: *mut Army, cap: usize, len: *mut usize) -> i32,
    /// Provinces on the current route of an own army, in order.
    pub army_route: unsafe extern "C" fn(host: *mut c_void, id: u64, buf: *mut u32, cap: usize, len: *mut usize) -> i32,
    /// Whether an own land army may currently enter a province (military
    /// access and terrain; forts are not considered).
    pub army_can_enter: unsafe extern "C" fn(host: *mut c_void, id: u64, province: u32, out: *mut u8) -> i32,
    /// Whether the strait between two neighboring provinces is open to the army.
    pub army_strait_open: unsafe extern "C" fn(host: *mut c_void, id: u64, from: u32, to: u32, out: *mut u8) -> i32,
    /// Whether an own land army may attack a visible army in `province`.
    pub army_can_attack:
        unsafe extern "C" fn(host: *mut c_void, id: u64, target: u64, province: u32, out: *mut u8) -> i32,
    /// The route the game itself would choose for an own land army; empty when
    /// `goal` is the army's province. [`NOT_FOUND`] when there is no route.
    pub find_route:
        unsafe extern "C" fn(host: *mut c_void, id: u64, goal: u32, buf: *mut u32, cap: usize, len: *mut usize) -> i32,
    /// Orders an own land army along the game's route to `goal`. Accepted
    /// means the route is installed, not that the army has arrived.
    pub army_move: unsafe extern "C" fn(host: *mut c_void, id: u64, goal: u32) -> i32,
    pub army_stop: unsafe extern "C" fn(host: *mut c_void, id: u64) -> i32,
    /// The own land army shown in the open single-army panel.
    pub ui_selected_army: unsafe extern "C" fn(host: *mut c_void, out: *mut u64) -> i32,
    pub ui_set_visible: unsafe extern "C" fn(host: *mut c_void, name: *const u8, name_len: usize, visible: u8) -> i32,
    pub ui_set_enabled: unsafe extern "C" fn(host: *mut c_void, name: *const u8, name_len: usize, enabled: u8) -> i32,
    pub ui_set_text: unsafe extern "C" fn(
        host: *mut c_void,
        name: *const u8,
        name_len: usize,
        text: *const u8,
        text_len: usize,
    ) -> i32,
    /// Tooltip of a button; an empty text removes it.
    pub ui_set_tooltip: unsafe extern "C" fn(
        host: *mut c_void,
        name: *const u8,
        name_len: usize,
        text: *const u8,
        text_len: usize,
    ) -> i32,
    /// Colors provinces on the map, replacing this extension's previous
    /// coloring. Another extension painting later takes over the map.
    pub map_paint: unsafe extern "C" fn(host: *mut c_void, colors: *const MapColor, count: usize) -> i32,
    /// Removes this extension's coloring and restores the previous map mode.
    pub map_clear: unsafe extern "C" fn(host: *mut c_void) -> i32,
    /// This extension's data for the current save (UTF-8 JSON; empty if none).
    pub save_data_get: unsafe extern "C" fn(host: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// Replaces this extension's data; written alongside the next save.
    pub save_data_set: unsafe extern "C" fn(host: *mut c_void, json: *const u8, len: usize) -> i32,
    /// The player language's Paradox name (UTF-8, for example `english` or
    /// `simp_chinese`): the language texts are shown in this session.
    pub language: unsafe extern "C" fn(host: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// This mod's localisation text for `key` (UTF-8, line breaks as `\n`)
    /// in the player language, or else in English. [`NOT_FOUND`] when the mod
    /// has neither.
    pub text: unsafe extern "C" fn(
        host: *mut c_void,
        key: *const u8,
        key_len: usize,
        buf: *mut u8,
        cap: usize,
        len: *mut usize,
    ) -> i32,
    /// Where the map camera looks and how high it is.
    pub camera: unsafe extern "C" fn(host: *mut c_void, out: *mut Camera) -> i32,
    /// Turns the camera to a province, at the position the game looks at it
    /// from, keeping the height; `out`, unless null, gets the camera
    /// afterwards. At once: the next frame shows it. [`REJECTED`] while the
    /// camera is locked.
    pub camera_look_at_province: unsafe extern "C" fn(host: *mut c_void, province: u32, out: *mut Camera) -> i32,
    /// Turns the camera to a map position, keeping the height; as
    /// `camera_look_at_province` otherwise. [`INVALID_ARGUMENT`] off the map.
    pub camera_look_at: unsafe extern "C" fn(host: *mut c_void, x: u32, y: u32, out: *mut Camera) -> i32,
    /// Raises or lowers the camera to `height`, looking at the same position;
    /// as `camera_look_at_province` otherwise. [`INVALID_ARGUMENT`] outside
    /// the camera's `min_height` to `max_height`.
    pub camera_zoom: unsafe extern "C" fn(host: *mut c_void, height: u32, out: *mut Camera) -> i32,
    /// A province's snapshot, as UTF-8 JSON (ADR 0018): `{"id": 151, "name":
    /// "Constantinople", "owner": <CountryId or null>, "controller": ...,
    /// "development": {"tax": 6.0, "production": 6.0, "manpower": 4.0},
    /// "terrain": "farmlands" or null, "fort": {"level": 2, "mothballed":
    /// false}, "buildings": ["fort_15th", ...], "recruit": [{"kind":
    /// "infantry", "unit_type": "eastern_medieval_infantry" or null,
    /// "allowed": false, "reasons": ["..."]}, ...]}`, `recruit` holding the
    /// kinds infantry, cavalry, artillery, heavy_ship, light_ship, galley and
    /// transport in that order; `abi/json/province.json` is a whole one.
    /// [`NOT_FOUND`] for a number that names no province.
    pub province: unsafe extern "C" fn(host: *mut c_void, province: u32, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// Runs a command in the game console as the player would type it
    /// (UTF-8, such as `cash 100`), and hands what it printed to `printed`,
    /// with `user`, once before returning [`OK`]; `printed` may be null.
    /// [`REJECTED`] when the game console refused the command or it failed:
    /// what it printed is then the `last_error` message. [`INVALID_ARGUMENT`]
    /// for an empty command or one with a `|` or a NUL, before the game sees
    /// it.
    pub console_run: unsafe extern "C" fn(
        host: *mut c_void,
        command: *const u8,
        command_len: usize,
        printed: Option<TextSink>,
        user: *mut c_void,
    ) -> i32,
    /// A country's snapshot, as UTF-8 JSON (ADR 0018): `{"id": <CountryId>,
    /// "treasury": 31.25, "manpower": 5400, "max_manpower": 8250,
    /// "stability": -1.0, "prestige": 12.345, "power": {"adm": 120, "dip":
    /// 95, "mil": 143}, "wars": [{"name": "...", "attackers": [<CountryId>,
    /// ...], "defenders": [...]}, ...], "relations": [{"kind": "ally",
    /// "country": <CountryId>}, ...]}`. The treasury is in ducats, stability
    /// from -3 to 3 and prestige from -100 to 100, each to the thousandth;
    /// manpower and its maximum in men; monarch points whole. `wars` holds
    /// every war going on that the country takes part in, its `name` as the
    /// game shows it and each side's leader first. `relations` holds its
    /// allies, its subjects, its overlord and the countries it rivals, in that
    /// order, `kind` being `ally`, `subject`, `overlord` or `rival`; a later
    /// core may add kinds. `abi/json/country.json` is a whole one.
    /// [`NOT_FOUND`] when there is no such country.
    pub country: unsafe extern "C" fn(host: *mut c_void, country: CountryId, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// The game speed, from 1 (slowest) to 5 (fastest). [`INTERNAL`] when the
    /// game holds a speed outside them.
    pub time_speed: unsafe extern "C" fn(host: *mut c_void, out: *mut u32) -> i32,
    /// Sets the game speed, from 1 to 5, as the player would; the game stays
    /// paused or running. [`INVALID_ARGUMENT`] outside 1 to 5.
    pub time_set_speed: unsafe extern "C" fn(host: *mut c_void, speed: u32) -> i32,
    /// Pauses the game with the player's own pause, as the player would, also
    /// when windows hold it paused already, so that it stays paused once they
    /// close; nothing when the player's pause holds it already. [`REJECTED`]
    /// when the game does not pause.
    pub time_pause: unsafe extern "C" fn(host: *mut c_void) -> i32,
    /// Resumes the game as the player would, also when windows hold it paused
    /// (they stay open); nothing when it runs. [`REJECTED`] when the game
    /// stays paused.
    pub time_resume: unsafe extern "C" fn(host: *mut c_void) -> i32,
    /// Recruits one regiment or ship of `kind` (one of the `UNIT_*` kinds)
    /// in a province for the player, through the game's own recruit command,
    /// as the recruit button in the province view does. Accepted means the
    /// game has the order: it carries it out at its next frame, and the
    /// regiment or ship then waits in the province's recruit queue. The game
    /// checks the order again then, and drops one that no longer passes with
    /// only a line in its own log: several orders in one callback are each
    /// checked against the same treasury, manpower and sailors, and those
    /// the first ones leave too little for are dropped at that frame.
    /// [`REJECTED`] when the game would not let the player recruit it there
    /// now, `last_error` giving the game's reasons, one a line;
    /// [`NOT_FOUND`] for a number that names no province;
    /// [`INVALID_ARGUMENT`] for a kind that is none of the `UNIT_*`.
    pub recruit: unsafe extern "C" fn(host: *mut c_void, province: u32, kind: u32) -> i32,
    /// The questions waiting for the player's answer, oldest first, as UTF-8
    /// JSON (ADR 0018): `[{"id": 7, "event": "flavor_tur.1", "title": "...",
    /// "description": "...", "deadline": <date>, "options": [{"index": 0,
    /// "text": "..."}, ...]}, ...]`, `deadline` the last day to answer (after
    /// it the game picks an option itself), `options` those the player may
    /// choose, `index` the option's number among all the event's;
    /// `abi/json/questions.json` is a whole one.
    pub questions: unsafe extern "C" fn(host: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// Answers a question with the option of that number, as the player does
    /// in its event window. [`NOT_FOUND`] for no such question,
    /// [`INVALID_ARGUMENT`] for no such option, [`REJECTED`] for one the
    /// player may not choose.
    pub question_answer: unsafe extern "C" fn(host: *mut c_void, question: u32, option: u32) -> i32,
    /// The offers waiting for the player's answer, as UTF-8 JSON (ADR 0018):
    /// `[{"id": 3, "from": <CountryId>, "date": <date>, "title": "...",
    /// "description": "..."}, ...]`, `date` when the offer was made, no more
    /// than two months ago (the game takes no answer to an older one);
    /// `abi/json/offers.json` is a whole one.
    pub offers: unsafe extern "C" fn(host: *mut c_void, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// Accepts (`accept` 1) or declines (0) an offer, as the player does in
    /// its window. [`NOT_FOUND`] for no such offer; [`REJECTED`] when the
    /// game no longer takes an answer to it (it was made more than two months
    /// ago, or what it offers is no longer possible), which the game then
    /// lets go, mostly declining it, as when the player opens it.
    pub offer_answer: unsafe extern "C" fn(host: *mut c_void, offer: u32, accept: u8) -> i32,
    /// The war goals the game offers the player for declaring war on
    /// `country`, as the declare war window lists them, as UTF-8 JSON (ADR
    /// 0018): `[{"casus_belli": "cb_claim", "war_goal": "take_claim",
    /// "province": 151, "target": null, "name": "..."}, ...]`. `casus_belli`
    /// is its casus belli's key, `none` for declaring war without one, which
    /// comes last when the game offers it; `war_goal` its type's key or null;
    /// `province` and `target` (a [`CountryId`]) what it is about, or null;
    /// `name` as the window shows it. `abi/json/war_goals.json` is a whole
    /// one. [`NOT_FOUND`] when there is no such country, [`INVALID_ARGUMENT`]
    /// for the player's own.
    pub war_goals: unsafe extern "C" fn(host: *mut c_void, country: CountryId, buf: *mut u8, cap: usize, len: *mut usize) -> i32,
    /// Declares war on `country` for the first war goal `war_goals` lists
    /// with the casus belli `casus_belli` (UTF-8: a key, or `none`) and, when
    /// given, the war goal type `war_goal` (empty for any), the `province`
    /// (0 for any) and the `target` (0 for any), as the player does through
    /// the diplomacy panel. It calls to arms, as ticking their call boxes in
    /// the declare war window does, with no land promised: with `call`
    /// [`CALL_ALL`] each ally the window would let the player call; with
    /// [`CALL_NONE`] none; with [`CALL_LISTED`] the `allies_len` allies at
    /// `allies`, each one the window must let the player call.
    /// `allies_called` says which it called. A costly declaration (no casus
    /// belli, a truce broken) is sent without the window's confirmation, and
    /// the game takes its costs. [`NOT_FOUND`] when there is no such country;
    /// [`INVALID_ARGUMENT`] for the player's own, an empty `casus_belli`, an
    /// unknown `call`, or a country listed to call that is not among the
    /// allies the window lists; [`REJECTED`] when the game offers no such
    /// war goal, does not let the player declare war now, or would not let
    /// the player call an ally listed, the game's reasons, one a line, as
    /// the `last_error` message: nothing is sent then. [`OK`] once the game
    /// has the declaration: it carries it out at its next frame; `*ticket`,
    /// unless `ticket` is null, names it for `diplomacy_answer` and
    /// `allies_called`.
    pub declare_war: unsafe extern "C" fn(
        host: *mut c_void,
        country: CountryId,
        casus_belli: *const u8,
        casus_belli_len: usize,
        war_goal: *const u8,
        war_goal_len: usize,
        province: u32,
        target: CountryId,
        call: u32,
        allies: *const CountryId,
        allies_len: usize,
        ticket: *mut u64,
    ) -> i32,
    /// Offers `country`, which the player is at war with, a white peace, as
    /// `declare_war` sends a declaration: [`NOT_FOUND`] when there is no such
    /// country, [`INVALID_ARGUMENT`] for the player's own, [`REJECTED`] with
    /// the game's reasons, [`OK`] with a ticket.
    pub offer_peace: unsafe extern "C" fn(host: *mut c_void, country: CountryId, ticket: *mut u64) -> i32,
    /// Proposes an alliance to `country`, as `declare_war` sends a
    /// declaration: [`NOT_FOUND`] when there is no such country,
    /// [`INVALID_ARGUMENT`] for the player's own, [`REJECTED`] with the
    /// game's reasons, [`OK`] with a ticket.
    pub propose_alliance: unsafe extern "C" fn(host: *mut c_void, country: CountryId, ticket: *mut u64) -> i32,
    /// What came of a diplomatic action sent, by its ticket: `*out` is
    /// [`DIPLOMACY_PENDING`] until the game has carried it out and the other
    /// country has answered, which in a single player game is a frame or two
    /// after it is sent, then [`DIPLOMACY_ACCEPTED`], [`DIPLOMACY_DECLINED`]
    /// or, for a declaration of war once the war began,
    /// [`DIPLOMACY_DECLARED`]; a later core may add values. A proposal the
    /// game drops as it carries it out, its own check failing then, stays
    /// [`DIPLOMACY_PENDING`]: the game says nothing of it. [`NOT_FOUND`] for
    /// a ticket no longer kept (the latest 256 are), [`NOT_IN_GAME`] once
    /// the game it was sent in has ended, [`REJECTED`] for a declaration the
    /// game carried out without a war beginning.
    pub diplomacy_answer: unsafe extern "C" fn(host: *mut c_void, ticket: u64, out: *mut u32) -> i32,
    /// The allies a declaration of war called to arms, by its ticket, as the
    /// list protocol writes them; none for a proposal. [`NOT_FOUND`] and
    /// [`NOT_IN_GAME`] as `diplomacy_answer`.
    pub allies_called: unsafe extern "C" fn(host: *mut c_void, ticket: u64, buf: *mut CountryId, cap: usize, len: *mut usize) -> i32,
}

/// Which allies a declaration of war calls to arms (`GameApi::declare_war`).
pub const CALL_ALL: u32 = 0;
pub const CALL_NONE: u32 = 1;
pub const CALL_LISTED: u32 = 2;

/// What came of a diplomatic action (`GameApi::diplomacy_answer`).
pub const DIPLOMACY_PENDING: u32 = 0;
pub const DIPLOMACY_ACCEPTED: u32 = 1;
pub const DIPLOMACY_DECLINED: u32 = 2;
pub const DIPLOMACY_DECLARED: u32 = 3;

/// Callbacks an extension registers from its entry point. Every callback runs
/// on the game thread at a safe point and returns [`OK`]; any other value makes
/// the core stop calling the extension.
#[repr(C)]
pub struct ExtensionVTable {
    pub size: usize,
    pub user: *mut c_void,
    /// A campaign was loaded or started; previous game state no longer applies.
    pub on_session: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi) -> i32>,
    /// Every UI frame while a campaign runs, paused or not.
    pub on_frame: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi) -> i32>,
    /// Once per game day, after the day's update and visibility refresh.
    pub on_day: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi, date: i32) -> i32>,
    /// A button this extension's mod declared was clicked. `name` is the
    /// element name as written in the mod's fragment.
    pub on_button: Option<
        unsafe extern "C" fn(
            user: *mut c_void,
            game: *const GameApi,
            name: *const u8,
            name_len: usize,
            click: u32,
            modifiers: u32,
        ) -> i32,
    >,
    /// The route of an own army was changed by something other than an
    /// extension, usually the player.
    pub on_army_ordered: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi, army: u64) -> i32>,
    /// An own army lost a battle.
    pub on_army_defeated: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi, army: u64) -> i32>,
    /// Another extension took over the map coloring.
    pub on_map_lost: Option<unsafe extern "C" fn(user: *mut c_void, game: *const GameApi) -> i32>,
}

// The layout extensions are built against, checked as the crate compiles: a
// field may only be appended, never moved or resized.

/// Asserts that the fields of a table of pointer-sized slots sit in this
/// order, one slot each, and that the table ends after the last.
macro_rules! slots {
    ($table:ty: $($field:ident),+ $(,)?) => {
        const _: () = {
            let mut slot = 0;
            $(
                assert!(std::mem::offset_of!($table, $field) == slot * 8, concat!(stringify!($table), "::", stringify!($field), " moved"));
                slot += 1;
            )+
            assert!(std::mem::size_of::<$table>() == slot * 8, concat!(stringify!($table), " changed size"));
        };
    };
}

/// Asserts each field's offset and the struct's size.
macro_rules! layout {
    ($ty:ty, $size:literal: $($field:ident @ $offset:literal),+ $(,)?) => {
        const _: () = {
            $(assert!(std::mem::offset_of!($ty, $field) == $offset, concat!(stringify!($ty), "::", stringify!($field), " moved"));)+
            assert!(std::mem::size_of::<$ty>() == $size, concat!(stringify!($ty), " changed size"));
        };
    };
}

layout!(HostApi, 32: size @ 0, abi_version @ 8, host @ 16, log @ 24);
layout!(Session, 24: session @ 0, date @ 8, paused @ 12, visibility_current @ 13, _reserved @ 14, player @ 16);
layout!(Province, 24: id @ 0, flags @ 4, owner @ 8, controller @ 16);
layout!(Neighbor, 8: to @ 0, flags @ 4);
layout!(Army, 40: id @ 0, owner @ 8, province @ 16, flags @ 20, regiments @ 24, route_len @ 28, destination @ 32, route_source @ 36);
layout!(MapColor, 8: province @ 0, rgba @ 4);
layout!(Camera, 24: x @ 0, y @ 4, height @ 8, province @ 12, min_height @ 16, max_height @ 20);
slots!(GameApi:
    size, host, last_error, session, country_tag, countries_hostile, provinces, province_neighbors, army, own_armies,
    province_armies, army_route, army_can_enter, army_strait_open, army_can_attack, find_route, army_move, army_stop,
    ui_selected_army, ui_set_visible, ui_set_enabled, ui_set_text, ui_set_tooltip, map_paint, map_clear, save_data_get,
    save_data_set, language, text,
    camera, camera_look_at_province, camera_look_at, camera_zoom,
    province,
    console_run,
    country,
    time_speed, time_set_speed, time_pause, time_resume,
    recruit,
    questions, question_answer, offers, offer_answer,
    war_goals, declare_war, offer_peace, propose_alliance, diplomacy_answer, allies_called,
);
slots!(ExtensionVTable: size, user, on_session, on_frame, on_day, on_button, on_army_ordered, on_army_defeated, on_map_lost);
