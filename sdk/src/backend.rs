//! What a [`Game`](crate::Game) talks to: the real capability table, or the
//! fake in [`testing`](crate::testing).
use crate::{abi, game::*};

/// The room a snapshot's JSON gets in the first call: several times a
/// province's.
const SNAPSHOT_ROOM: usize = 16 * 1024;

/// The capabilities behind a `Game`.
pub trait Backend {
    fn session(&mut self) -> Result<Session>;
    fn country_tag(&mut self, country: CountryId) -> Result<String>;
    fn countries_hostile(&mut self, a: CountryId, b: CountryId) -> Result<bool>;
    fn provinces(&mut self) -> Result<Vec<Province>>;
    fn province_neighbors(&mut self, province: ProvinceId) -> Result<Vec<Neighbor>>;
    fn army(&mut self, id: ArmyId) -> Result<Army>;
    fn own_armies(&mut self) -> Result<Vec<Army>>;
    fn province_armies(&mut self, province: ProvinceId) -> Result<Vec<Army>>;
    fn army_route(&mut self, id: ArmyId) -> Result<Vec<ProvinceId>>;
    fn army_can_enter(&mut self, id: ArmyId, province: ProvinceId) -> Result<bool>;
    fn army_strait_open(&mut self, id: ArmyId, from: ProvinceId, to: ProvinceId) -> Result<bool>;
    fn army_can_attack(&mut self, id: ArmyId, target: ArmyId, province: ProvinceId) -> Result<bool>;
    /// `NotFound` when there is no route.
    fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Vec<ProvinceId>>;
    fn army_move(&mut self, id: ArmyId, goal: ProvinceId) -> Result<()>;
    fn army_stop(&mut self, id: ArmyId) -> Result<()>;
    /// `NotFound` when no own land army is shown.
    fn ui_selected_army(&mut self) -> Result<ArmyId>;
    fn ui_set_visible(&mut self, element: &str, visible: bool) -> Result<()>;
    fn ui_set_enabled(&mut self, element: &str, enabled: bool) -> Result<()>;
    fn ui_set_text(&mut self, element: &str, text: &str) -> Result<()>;
    fn ui_set_tooltip(&mut self, element: &str, text: &str) -> Result<()>;
    fn map_paint(&mut self, colors: &[MapColor]) -> Result<()>;
    fn map_clear(&mut self) -> Result<()>;
    /// JSON text; empty when there is none.
    fn save_data_get(&mut self) -> Result<String>;
    fn save_data_set(&mut self, json: &str) -> Result<()>;
    /// The player language's Paradox name.
    fn language(&mut self) -> Result<String>;
    /// This mod's text for `key`; `NotFound` when it has none.
    fn text(&mut self, key: &str) -> Result<String>;
    fn camera(&mut self) -> Result<Camera>;
    fn camera_look_at_province(&mut self, province: ProvinceId) -> Result<Camera>;
    fn camera_look_at(&mut self, x: u32, y: u32) -> Result<Camera>;
    fn camera_zoom(&mut self, height: u32) -> Result<Camera>;
    /// `NotFound` for a number that names no province.
    fn province(&mut self, province: ProvinceId) -> Result<ProvinceSnapshot>;
    /// What the game console command printed; `Rejected`, with what it
    /// printed, when the game console refused it or it failed.
    fn console_run(&mut self, command: &str) -> Result<String>;
    /// `NotFound` when there is no such country.
    fn country(&mut self, country: CountryId) -> Result<Country>;
}

/// The core's capability table for the current callback.
pub(crate) struct RealBackend {
    api: *const abi::GameApi,
}

impl RealBackend {
    pub(crate) fn new(api: *const abi::GameApi) -> Self {
        Self { api }
    }

    fn api(&self) -> &abi::GameApi {
        unsafe { &*self.api }
    }

    fn check(&self, code: i32) -> Result<()> {
        if code == abi::OK {
            return Ok(());
        }
        let api = self.api();
        let len = unsafe { (api.last_error)(api.host, std::ptr::null_mut(), 0) };
        let mut buf = vec![0u8; len];
        unsafe { (api.last_error)(api.host, buf.as_mut_ptr(), buf.len()) };
        Err(Error::new(Status::from_code(code), String::from_utf8_lossy(&buf).into_owned()))
    }

    /// Two-call list protocol.
    fn list<T: Copy + Default>(&self, call: impl Fn(*mut T, usize, *mut usize) -> i32) -> Result<Vec<T>> {
        self.list_from(0, call)
    }

    /// Two-call list protocol, with room for `room` items in the first call:
    /// a list that fits takes one call.
    fn list_from<T: Copy + Default>(&self, room: usize, call: impl Fn(*mut T, usize, *mut usize) -> i32) -> Result<Vec<T>> {
        let mut len = 0usize;
        let mut buf: Vec<T> = vec![T::default(); room];
        for _ in 0..4 {
            let code = call(buf.as_mut_ptr(), buf.len(), &mut len);
            if code == abi::BUFFER_TOO_SMALL {
                buf = vec![T::default(); len];
                continue;
            }
            self.check(code)?;
            buf.truncate(len);
            return Ok(buf);
        }
        Err(Error::new(Status::Internal, "list kept growing"))
    }

    /// A snapshot's JSON read into `T` (ADR 0018). The first call has room
    /// for [`SNAPSHOT_ROOM`] bytes, so the core builds a snapshot that fits
    /// once; a larger one it builds again for the second call.
    fn snapshot<T: serde::de::DeserializeOwned>(&self, what: &str, call: impl Fn(*mut u8, usize, *mut usize) -> i32) -> Result<T> {
        let bytes = self.list_from(SNAPSHOT_ROOM, call)?;
        serde_json::from_slice(&bytes).map_err(|e| Error::new(Status::Internal, format!("{what} snapshot: {e}")))
    }

    fn flag(&self, call: impl FnOnce(*mut u8) -> i32) -> Result<bool> {
        let mut out = 0u8;
        self.check(call(&mut out))?;
        Ok(out != 0)
    }

    /// Fails when the core's table ends before `end`, the offset just past a
    /// field appended after the first release of the table.
    fn require(&self, end: usize) -> Result<()> {
        if self.api().size < end {
            return Err(Error::new(Status::Unavailable, "the installed EU4 Harmony is older than this SDK"));
        }
        Ok(())
    }

    fn utf8(&self, bytes: Vec<u8>) -> Result<String> {
        String::from_utf8(bytes).map_err(|e| Error::new(Status::Internal, e.to_string()))
    }
}

/// Offset just past a function-pointer field of the game table.
macro_rules! end_of {
    ($field:ident) => {
        std::mem::offset_of!(abi::GameApi, $field) + std::mem::size_of::<usize>()
    };
}

impl Backend for RealBackend {
    fn session(&mut self) -> Result<Session> {
        let api = self.api();
        let mut out = abi::Session::default();
        self.check(unsafe { (api.session)(api.host, &mut out) })?;
        Ok(out.into())
    }
    fn country_tag(&mut self, country: CountryId) -> Result<String> {
        let api = self.api();
        let mut out = [0u8; 3];
        self.check(unsafe { (api.country_tag)(api.host, country.0, &mut out) })?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
    fn countries_hostile(&mut self, a: CountryId, b: CountryId) -> Result<bool> {
        let api = self.api();
        self.flag(|o| unsafe { (api.countries_hostile)(api.host, a.0, b.0, o) })
    }
    fn provinces(&mut self) -> Result<Vec<Province>> {
        let api = self.api();
        Ok(self.list(|b, c, l| unsafe { (api.provinces)(api.host, b, c, l) })?.into_iter().map(Province::from).collect())
    }
    fn province_neighbors(&mut self, province: ProvinceId) -> Result<Vec<Neighbor>> {
        let api = self.api();
        Ok(self.list(|b, c, l| unsafe { (api.province_neighbors)(api.host, province, b, c, l) })?.into_iter().map(Neighbor::from).collect())
    }
    fn army(&mut self, id: ArmyId) -> Result<Army> {
        let api = self.api();
        let mut out = abi::Army::default();
        self.check(unsafe { (api.army)(api.host, id.0, &mut out) })?;
        Ok(out.into())
    }
    fn own_armies(&mut self) -> Result<Vec<Army>> {
        let api = self.api();
        Ok(self.list(|b, c, l| unsafe { (api.own_armies)(api.host, b, c, l) })?.into_iter().map(Army::from).collect())
    }
    fn province_armies(&mut self, province: ProvinceId) -> Result<Vec<Army>> {
        let api = self.api();
        Ok(self.list(|b, c, l| unsafe { (api.province_armies)(api.host, province, b, c, l) })?.into_iter().map(Army::from).collect())
    }
    fn army_route(&mut self, id: ArmyId) -> Result<Vec<ProvinceId>> {
        let api = self.api();
        self.list(|b, c, l| unsafe { (api.army_route)(api.host, id.0, b, c, l) })
    }
    fn army_can_enter(&mut self, id: ArmyId, province: ProvinceId) -> Result<bool> {
        let api = self.api();
        self.flag(|o| unsafe { (api.army_can_enter)(api.host, id.0, province, o) })
    }
    fn army_strait_open(&mut self, id: ArmyId, from: ProvinceId, to: ProvinceId) -> Result<bool> {
        let api = self.api();
        self.flag(|o| unsafe { (api.army_strait_open)(api.host, id.0, from, to, o) })
    }
    fn army_can_attack(&mut self, id: ArmyId, target: ArmyId, province: ProvinceId) -> Result<bool> {
        let api = self.api();
        self.flag(|o| unsafe { (api.army_can_attack)(api.host, id.0, target.0, province, o) })
    }
    fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Vec<ProvinceId>> {
        let api = self.api();
        self.list(|b, c, l| unsafe { (api.find_route)(api.host, id.0, goal, b, c, l) })
    }
    fn army_move(&mut self, id: ArmyId, goal: ProvinceId) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.army_move)(api.host, id.0, goal) })
    }
    fn army_stop(&mut self, id: ArmyId) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.army_stop)(api.host, id.0) })
    }
    fn ui_selected_army(&mut self) -> Result<ArmyId> {
        let api = self.api();
        let mut out = 0u64;
        self.check(unsafe { (api.ui_selected_army)(api.host, &mut out) })?;
        Ok(ArmyId(out))
    }
    fn ui_set_visible(&mut self, element: &str, visible: bool) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.ui_set_visible)(api.host, element.as_ptr(), element.len(), visible as u8) })
    }
    fn ui_set_enabled(&mut self, element: &str, enabled: bool) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.ui_set_enabled)(api.host, element.as_ptr(), element.len(), enabled as u8) })
    }
    fn ui_set_text(&mut self, element: &str, text: &str) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.ui_set_text)(api.host, element.as_ptr(), element.len(), text.as_ptr(), text.len()) })
    }
    fn ui_set_tooltip(&mut self, element: &str, text: &str) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.ui_set_tooltip)(api.host, element.as_ptr(), element.len(), text.as_ptr(), text.len()) })
    }
    fn map_paint(&mut self, colors: &[MapColor]) -> Result<()> {
        let api = self.api();
        let raw: Vec<abi::MapColor> = colors.iter().map(|c| abi::MapColor { province: c.province, rgba: c.rgba }).collect();
        self.check(unsafe { (api.map_paint)(api.host, raw.as_ptr(), raw.len()) })
    }
    fn map_clear(&mut self) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.map_clear)(api.host) })
    }
    fn save_data_get(&mut self) -> Result<String> {
        let api = self.api();
        let bytes = self.list(|b, c, l| unsafe { (api.save_data_get)(api.host, b, c, l) })?;
        self.utf8(bytes)
    }
    fn save_data_set(&mut self, json: &str) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.save_data_set)(api.host, json.as_ptr(), json.len()) })
    }
    fn language(&mut self) -> Result<String> {
        self.require(end_of!(language))?;
        let api = self.api();
        let bytes = self.list(|b, c, l| unsafe { (api.language)(api.host, b, c, l) })?;
        self.utf8(bytes)
    }
    fn text(&mut self, key: &str) -> Result<String> {
        self.require(end_of!(text))?;
        let api = self.api();
        let bytes = self.list(|b, c, l| unsafe { (api.text)(api.host, key.as_ptr(), key.len(), b, c, l) })?;
        self.utf8(bytes)
    }
    fn camera(&mut self) -> Result<Camera> {
        self.require(end_of!(camera))?;
        let api = self.api();
        let mut out = abi::Camera::default();
        self.check(unsafe { (api.camera)(api.host, &mut out) })?;
        Ok(out.into())
    }
    fn camera_look_at_province(&mut self, province: ProvinceId) -> Result<Camera> {
        self.require(end_of!(camera_look_at_province))?;
        let api = self.api();
        let mut out = abi::Camera::default();
        self.check(unsafe { (api.camera_look_at_province)(api.host, province, &mut out) })?;
        Ok(out.into())
    }
    fn camera_look_at(&mut self, x: u32, y: u32) -> Result<Camera> {
        self.require(end_of!(camera_look_at))?;
        let api = self.api();
        let mut out = abi::Camera::default();
        self.check(unsafe { (api.camera_look_at)(api.host, x, y, &mut out) })?;
        Ok(out.into())
    }
    fn camera_zoom(&mut self, height: u32) -> Result<Camera> {
        self.require(end_of!(camera_zoom))?;
        let api = self.api();
        let mut out = abi::Camera::default();
        self.check(unsafe { (api.camera_zoom)(api.host, height, &mut out) })?;
        Ok(out.into())
    }
    fn province(&mut self, province: ProvinceId) -> Result<ProvinceSnapshot> {
        self.require(end_of!(province))?;
        let api = self.api();
        let json: ProvinceJson = self.snapshot("province", |b, c, l| unsafe { (api.province)(api.host, province, b, c, l) })?;
        Ok(json.into())
    }
    fn console_run(&mut self, command: &str) -> Result<String> {
        self.require(end_of!(console_run))?;
        unsafe extern "C" fn keep(user: *mut std::ffi::c_void, text: *const u8, len: usize) {
            if len > 0 && !text.is_null() {
                (*(user as *mut Vec<u8>)).extend_from_slice(std::slice::from_raw_parts(text, len));
            }
        }
        let api = self.api();
        let mut printed: Vec<u8> = Vec::new();
        let user = &mut printed as *mut Vec<u8> as *mut std::ffi::c_void;
        self.check(unsafe { (api.console_run)(api.host, command.as_ptr(), command.len(), Some(keep), user) })?;
        self.utf8(printed)
    }
    fn country(&mut self, country: CountryId) -> Result<Country> {
        self.require(end_of!(country))?;
        let api = self.api();
        let mut numbers = abi::Country::default();
        let out: *mut abi::Country = &mut numbers;
        let links = self.list(|b, c, l| unsafe { (api.country)(api.host, country.0, out, b, c, l) })?;
        Ok(Country::from_abi(numbers, &links))
    }
}

/// The SDK's side of the ABI, against a table built here as the core builds
/// its own: what the fake game skips.
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;

    type H = *mut c_void;
    // What the tests do not call.
    unsafe extern "C" fn no_flag_2(_: H, _: u64, _: u32, _: *mut u8) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_flag_3(_: H, _: u64, _: u32, _: u32, _: *mut u8) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_attack(_: H, _: u64, _: u64, _: u32, _: *mut u8) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_tag(_: H, _: u64, _: *mut [u8; 3]) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_hostile(_: H, _: u64, _: u64, _: *mut u8) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_neighbors(_: H, _: u32, _: *mut abi::Neighbor, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_armies(_: H, _: *mut abi::Army, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_province_armies(_: H, _: u32, _: *mut abi::Army, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_route(_: H, _: u64, _: *mut u32, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_find(_: H, _: u64, _: u32, _: *mut u32, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_move(_: H, _: u64, _: u32) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_stop(_: H, _: u64) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_selected(_: H, _: *mut u64) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_flag_ui(_: H, _: *const u8, _: usize, _: u8) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_text_ui(_: H, _: *const u8, _: usize, _: *const u8, _: usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_paint(_: H, _: *const abi::MapColor, _: usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_clear(_: H) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_bytes(_: H, _: *mut u8, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_set(_: H, _: *const u8, _: usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_text(_: H, _: *const u8, _: usize, _: *mut u8, _: usize, _: *mut usize) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_camera_province(_: H, _: u32, _: *mut abi::Camera) -> i32 { abi::INTERNAL }
    unsafe extern "C" fn no_camera_position(_: H, _: u32, _: u32, _: *mut abi::Camera) -> i32 { abi::INTERNAL }

    /// Hands back what `cash 100` prints; refuses anything else.
    unsafe extern "C" fn console_run(_: H, command: *const u8, len: usize, printed: Option<abi::TextSink>, user: H) -> i32 {
        if std::slice::from_raw_parts(command, len) != b"cash 100" {
            return abi::REJECTED;
        }
        let text = "Gold changed by 100 ✓";
        if let Some(printed) = printed {
            printed(user, text.as_ptr(), text.len());
        }
        abi::OK
    }

    unsafe extern "C" fn last_error(_: H, buf: *mut u8, cap: usize) -> usize {
        let message = b"no such army";
        if !buf.is_null() {
            std::ptr::copy_nonoverlapping(message.as_ptr(), buf, cap.min(message.len()));
        }
        message.len()
    }
    unsafe extern "C" fn session(_: H, out: *mut abi::Session) -> i32 {
        *out = abi::Session { session: 3, date: 1444, paused: 1, visibility_current: 0, _reserved: 0, player: 77 };
        abi::OK
    }
    unsafe extern "C" fn provinces(_: H, buf: *mut abi::Province, cap: usize, len: *mut usize) -> i32 {
        let all = [abi::Province::default(), abi::Province { id: 1, flags: abi::PROVINCE_LAND | abi::PROVINCE_VISIBLE, owner: 77, controller: 0 }];
        *len = all.len();
        if cap < all.len() {
            return abi::BUFFER_TOO_SMALL;
        }
        std::ptr::copy_nonoverlapping(all.as_ptr(), buf, all.len());
        abi::OK
    }
    unsafe extern "C" fn army(_: H, _: u64, _: *mut abi::Army) -> i32 {
        abi::NOT_FOUND
    }
    unsafe extern "C" fn camera(_: H, out: *mut abi::Camera) -> i32 {
        *out = abi::Camera { x: 2842, y: 1494, height: 800, province: 183, min_height: 50, max_height: 3000 };
        abi::OK
    }
    unsafe extern "C" fn language(_: H, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
        let name = b"simp_chinese";
        *len = name.len();
        if cap < name.len() {
            return abi::BUFFER_TOO_SMALL;
        }
        std::ptr::copy_nonoverlapping(name.as_ptr(), buf, name.len());
        abi::OK
    }
    /// Country 77 allied with 5, rivalling 6, attacking 6 with 5 in war 1,
    /// defending against 8 in war 2; a link of a kind from a later core.
    unsafe extern "C" fn country(_: H, id: u64, out: *mut abi::Country, buf: *mut abi::CountryLink, cap: usize, len: *mut usize) -> i32 {
        if id != 77 {
            return abi::NOT_FOUND;
        }
        *out = abi::Country { tag: 77, treasury: 125_500, manpower: 22_600, max_manpower: 30_000, stability: -1_000, prestige: 12_345, power: [50, 60, 70] };
        let link = |kind, war, country| abi::CountryLink { kind, war, country };
        let links = [
            link(abi::LINK_ALLY, 0, 5),
            link(abi::LINK_RIVAL, 0, 6),
            link(99, 0, 9),
            link(abi::LINK_ATTACKER, 1, 77),
            link(abi::LINK_ATTACKER, 1, 5),
            link(abi::LINK_DEFENDER, 1, 6),
            link(abi::LINK_ATTACKER, 2, 8),
            link(abi::LINK_DEFENDER, 2, 77),
        ];
        *len = links.len();
        if cap < links.len() {
            return abi::BUFFER_TOO_SMALL;
        }
        std::ptr::copy_nonoverlapping(links.as_ptr(), buf, links.len());
        abi::OK
    }

    /// Constantinople, as the core's test checks that the core writes it
    /// (ADR 0018).
    const PROVINCE_JSON: &str = include_str!("../../abi/json/province.json");

    unsafe extern "C" fn province(_: H, id: u32, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
        if id != 151 {
            return abi::NOT_FOUND;
        }
        let json = PROVINCE_JSON.as_bytes();
        *len = json.len();
        if cap < json.len() {
            return abi::BUFFER_TOO_SMALL;
        }
        std::ptr::copy_nonoverlapping(json.as_ptr(), buf, json.len());
        abi::OK
    }

    fn table() -> abi::GameApi {
        abi::GameApi {
            size: std::mem::size_of::<abi::GameApi>(),
            host: std::ptr::null_mut(),
            last_error,
            session,
            country_tag: no_tag,
            countries_hostile: no_hostile,
            provinces,
            province_neighbors: no_neighbors,
            army,
            own_armies: no_armies,
            province_armies: no_province_armies,
            army_route: no_route,
            army_can_enter: no_flag_2,
            army_strait_open: no_flag_3,
            army_can_attack: no_attack,
            find_route: no_find,
            army_move: no_move,
            army_stop: no_stop,
            ui_selected_army: no_selected,
            ui_set_visible: no_flag_ui,
            ui_set_enabled: no_flag_ui,
            ui_set_text: no_text_ui,
            ui_set_tooltip: no_text_ui,
            map_paint: no_paint,
            map_clear: no_clear,
            save_data_get: no_bytes,
            save_data_set: no_set,
            language,
            text: no_text,
            camera,
            camera_look_at_province: no_camera_province,
            camera_look_at: no_camera_position,
            camera_zoom: no_camera_province,
            province,
            console_run,
            country,
        }
    }

    #[test]
    fn a_country_comes_with_its_relations_and_wars() {
        let api = table();
        let mut backend = RealBackend::new(&api);
        let c = backend.country(CountryId(77)).unwrap();
        assert_eq!((c.id, c.treasury, c.manpower, c.max_manpower, c.stability, c.prestige), (CountryId(77), 125.5, 22_600, 30_000, -1.0, 12.345));
        assert_eq!(c.power, Power { adm: 50, dip: 60, mil: 70 });
        let relation = |kind, id| Relation { kind, country: CountryId(id) };
        assert_eq!(c.relations, [relation(RelationKind::Ally, 5), relation(RelationKind::Rival, 6)]);
        let ids = |ids: &[u64]| ids.iter().map(|&i| CountryId(i)).collect::<Vec<_>>();
        assert_eq!(c.wars, [War { attackers: ids(&[77, 5]), defenders: ids(&[6]) }, War { attackers: ids(&[8]), defenders: ids(&[77]) }]);
        assert!(c.has_relation(RelationKind::Ally, CountryId(5)) && !c.has_relation(RelationKind::Ally, CountryId(6)));
        assert!(c.at_war_with(CountryId(6)) && c.at_war_with(CountryId(8)) && !c.at_war_with(CountryId(5)));
        assert_eq!(backend.country(CountryId(3)).unwrap_err().status, Status::NotFound);
    }

    #[test]
    fn values_and_lists_come_through_the_table() {
        let api = table();
        let mut backend = RealBackend::new(&api);
        let session = backend.session().unwrap();
        assert_eq!((session.id, session.date, session.paused, session.visibility_current, session.player), (3, 1444, true, false, CountryId(77)));
        // A list asks twice: once for its length, once with room for it.
        let provinces = backend.provinces().unwrap();
        assert_eq!(provinces.len(), 2);
        assert_eq!((provinces[1].id, provinces[1].land, provinces[1].visible, provinces[1].owner, provinces[1].controller), (1, true, true, Some(CountryId(77)), None));
        assert_eq!(backend.language().unwrap(), "simp_chinese");
        let camera = backend.camera().unwrap();
        assert_eq!((camera.x, camera.y, camera.height, camera.province), (2842, 1494, 800, Some(183)));
    }

    #[test]
    fn a_province_snapshot_comes_as_json() {
        let api = table();
        let mut backend = RealBackend::new(&api);
        let p = backend.province(151).unwrap();
        let byz = Some(CountryId(72057735777769794));
        assert_eq!((p.id, p.name.as_str(), p.owner, p.controller, p.terrain.as_deref()), (151, "君士坦丁堡", byz, byz, Some("farmlands")));
        assert_eq!((p.development.tax, p.development.production, p.development.manpower), (10.0, 9.5, 6.0));
        assert_eq!((p.fort.level, p.fort.mothballed), (2, false));
        assert_eq!(p.buildings, ["fort_15th", "marketplace"]);
        // Every kind's name reads, in the core's order.
        let recruit: Vec<_> = p.recruit.iter().map(|r| (r.kind, r.unit_type.as_deref(), r.allowed, r.reasons.len())).collect();
        assert_eq!(
            recruit,
            [
                (UnitKind::Infantry, Some("eastern_medieval_infantry"), false, 1),
                (UnitKind::Cavalry, Some("eastern_knights"), false, 1),
                (UnitKind::Artillery, None, false, 0),
                (UnitKind::HeavyShip, Some("early_carrack"), false, 0),
                (UnitKind::LightShip, Some("early_war_galley"), false, 0),
                (UnitKind::Galley, Some("galley"), false, 0),
                (UnitKind::Transport, Some("cog"), false, 0),
            ]
        );
        assert_eq!(p.recruit[0].reasons, ["君士坦丁堡不是我们的领土"]);
        assert_eq!(backend.province(1).unwrap_err().status, Status::NotFound);
    }

    static BUILT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// `province`, counting how often the core would build a snapshot.
    unsafe extern "C" fn counted_province(h: H, id: u32, buf: *mut u8, cap: usize, len: *mut usize) -> i32 {
        BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        province(h, id, buf, cap, len)
    }

    /// A snapshot that fits the first call's room is built once.
    #[test]
    fn a_snapshot_takes_one_call() {
        let mut api = table();
        api.province = counted_province;
        assert!(PROVINCE_JSON.len() < SNAPSHOT_ROOM);
        RealBackend::new(&api).province(151).unwrap();
        assert_eq!(BUILT.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// A newer core may add members and values: the snapshot still reads,
    /// a new kind as unknown (ADR 0018).
    #[test]
    fn a_newer_core_s_snapshot_still_reads() {
        let mut json: serde_json::Value = serde_json::from_str(PROVINCE_JSON).unwrap();
        json["religion"] = "orthodox".into();
        json["recruit"][2]["kind"] = "marines".into();
        json["recruit"][2]["cost"] = 10.0.into();
        let p: ProvinceSnapshot = serde_json::from_value::<ProvinceJson>(json).unwrap().into();
        let kinds: Vec<_> = p.recruit.iter().map(|r| r.kind).collect();
        assert_eq!(kinds[1..4], [UnitKind::Cavalry, UnitKind::Unknown, UnitKind::HeavyShip]);
        assert_eq!((p.name.as_str(), p.recruit.len()), ("君士坦丁堡", 7));
    }

    #[test]
    fn a_failure_brings_the_core_s_message() {
        let api = table();
        let err = RealBackend::new(&api).army(ArmyId(5)).unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (Status::NotFound, "no such army"));
    }

    /// A core from before a capability was added: its table ends before it.
    #[test]
    fn a_capability_the_core_lacks_is_unavailable() {
        let mut api = table();
        api.size = std::mem::offset_of!(abi::GameApi, language);
        let mut backend = RealBackend::new(&api);
        assert_eq!(backend.language().unwrap_err().status, Status::Unavailable);
        assert_eq!(backend.text("KEY").unwrap_err().status, Status::Unavailable);
        assert!(backend.session().is_ok());
        api.size = std::mem::offset_of!(abi::GameApi, province);
        let mut backend = RealBackend::new(&api);
        assert_eq!(backend.province(151).unwrap_err().status, Status::Unavailable);
        assert!(backend.language().is_ok());
        api.size = std::mem::offset_of!(abi::GameApi, country);
        assert_eq!(RealBackend::new(&api).country(CountryId(77)).unwrap_err().status, Status::Unavailable);
    }

    #[test]
    fn a_core_from_before_the_camera_has_none() {
        let mut api = table();
        api.size = std::mem::offset_of!(abi::GameApi, camera);
        let mut backend = RealBackend::new(&api);
        assert_eq!(backend.camera().unwrap_err().status, Status::Unavailable);
        assert_eq!(backend.camera_look_at_province(183).unwrap_err().status, Status::Unavailable);
        assert_eq!(backend.camera_look_at(2842, 1494).unwrap_err().status, Status::Unavailable);
        assert_eq!(backend.camera_zoom(800).unwrap_err().status, Status::Unavailable);
        assert!(backend.language().is_ok());
    }

    #[test]
    fn a_console_command_hands_back_what_it_printed() {
        let api = table();
        let mut backend = RealBackend::new(&api);
        assert_eq!(backend.console_run("cash 100").unwrap(), "Gold changed by 100 ✓");
        // A refusal's message comes from `last_error`, as every failure's.
        let err = backend.console_run("cahs 100").unwrap_err();
        assert_eq!((err.status, err.message.as_str()), (Status::Rejected, "no such army"));
    }

    #[test]
    fn a_core_from_before_the_game_console_has_none() {
        let mut api = table();
        api.size = std::mem::offset_of!(abi::GameApi, console_run);
        let mut backend = RealBackend::new(&api);
        assert_eq!(backend.console_run("cash 100").unwrap_err().status, Status::Unavailable);
        assert!(backend.camera().is_ok());
    }
}
