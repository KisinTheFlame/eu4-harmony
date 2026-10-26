//! What a [`Game`](crate::Game) talks to: the real capability table, or the
//! fake in [`testing`](crate::testing).
use crate::{abi, game::*};

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
        let mut len = 0usize;
        let mut buf: Vec<T> = Vec::new();
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

    fn flag(&self, call: impl FnOnce(*mut u8) -> i32) -> Result<bool> {
        let mut out = 0u8;
        self.check(call(&mut out))?;
        Ok(out != 0)
    }
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
        String::from_utf8(bytes).map_err(|e| Error::new(Status::Internal, e.to_string()))
    }
    fn save_data_set(&mut self, json: &str) -> Result<()> {
        let api = self.api();
        self.check(unsafe { (api.save_data_set)(api.host, json.as_ptr(), json.len()) })
    }
}
