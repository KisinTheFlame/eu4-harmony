//! An in-memory stand-in for the game, for unit tests of extensions.
//!
//! It stores data and records what the extension did; it does not simulate
//! game rules. Tests set up provinces, armies and routes, call the extension's
//! events with [`FakeGame::game`], and inspect [`FakeGame::commands`], the UI
//! state and the map. Orders have no effect unless the test applies them.
//! [`FakeGame::load_localisation`] gives it the mod's real texts.
use crate::{backend::Backend, game::*};
use eu4_harmony_loc::Language;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Move(ArmyId, ProvinceId),
    Stop(ArmyId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    pub visible: Option<bool>,
    pub enabled: Option<bool>,
    pub text: Option<String>,
    pub tooltip: Option<String>,
}

#[derive(Default)]
pub struct FakeGame {
    pub session: Session,
    pub tags: HashMap<CountryId, String>,
    /// Unordered pairs of hostile countries.
    pub hostile: BTreeSet<(CountryId, CountryId)>,
    /// Indexed by province ID; index 0 is the placeholder.
    pub provinces: Vec<Province>,
    pub neighbors: BTreeMap<ProvinceId, Vec<Neighbor>>,
    pub own_armies: Vec<Army>,
    /// Visible armies per province, own and foreign.
    pub province_armies: BTreeMap<ProvinceId, Vec<Army>>,
    pub routes_of_armies: BTreeMap<ArmyId, Vec<ProvinceId>>,
    /// Provinces each army may not enter.
    pub closed: BTreeSet<(ArmyId, ProvinceId)>,
    pub closed_straits: BTreeSet<(ProvinceId, ProvinceId)>,
    /// Targets each army may not attack.
    pub protected: BTreeSet<(ArmyId, ArmyId)>,
    /// Routes `find_route` answers with; missing entries mean "no route".
    pub routes: BTreeMap<(ArmyId, ProvinceId), Vec<ProvinceId>>,
    /// Refuse every order with this message.
    pub reject_orders: Option<String>,
    pub selected: Option<ArmyId>,
    pub elements: BTreeMap<String, Element>,
    pub map: Option<Vec<MapColor>>,
    pub save_data: String,
    pub commands: Vec<Command>,
    /// The player language's Paradox name; English when empty.
    pub language: String,
    /// What `Game::text` answers, by key; see [`FakeGame::load_localisation`].
    pub texts: BTreeMap<String, String>,
}

fn not_found(what: impl Into<String>) -> Error {
    Error::new(Status::NotFound, what)
}

impl FakeGame {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn game(&mut self) -> Game<'_> {
        Game::new(self)
    }

    pub fn element(&self, name: &str) -> Element {
        self.elements.get(name).cloned().unwrap_or_default()
    }

    /// Sets the player language (a Paradox name such as `simp_chinese`) and
    /// fills [`texts`](Self::texts) from the mod at `mod_dir`, the way the
    /// core reads its `eu4-harmony/localisation/` files. Fails on files the
    /// core would leave out.
    pub fn load_localisation(&mut self, mod_dir: impl AsRef<Path>, language: &str) -> std::result::Result<(), String> {
        let language = Language::from_id(language).ok_or_else(|| format!("unknown language {language}"))?;
        let (catalog, problems) = eu4_harmony_loc::load_dir(&mod_dir.as_ref().join("eu4-harmony").join("localisation"));
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        self.language = language.id().to_string();
        self.texts = catalog.keys().into_iter().filter_map(|k| Some((k.to_string(), catalog.text(language, k)?))).collect();
        Ok(())
    }

    pub fn set_hostile(&mut self, a: CountryId, b: CountryId) {
        self.hostile.insert((a.min(b), a.max(b)));
    }

    /// Adds a land link both ways.
    pub fn link(&mut self, a: ProvinceId, b: ProvinceId) {
        self.neighbors.entry(a).or_default().push(Neighbor { to: b, strait: false });
        self.neighbors.entry(b).or_default().push(Neighbor { to: a, strait: false });
    }

    fn own(&self, id: ArmyId) -> Result<&Army> {
        self.own_armies.iter().find(|a| a.id == id).ok_or_else(|| not_found(format!("no own army {}", id.0)))
    }
}

impl Backend for FakeGame {
    fn session(&mut self) -> Result<Session> {
        Ok(self.session)
    }
    fn country_tag(&mut self, country: CountryId) -> Result<String> {
        self.tags.get(&country).cloned().ok_or_else(|| not_found("unknown country"))
    }
    fn countries_hostile(&mut self, a: CountryId, b: CountryId) -> Result<bool> {
        Ok(a != b && self.hostile.contains(&(a.min(b), a.max(b))))
    }
    fn provinces(&mut self) -> Result<Vec<Province>> {
        Ok(self.provinces.clone())
    }
    fn province_neighbors(&mut self, province: ProvinceId) -> Result<Vec<Neighbor>> {
        Ok(self.neighbors.get(&province).cloned().unwrap_or_default())
    }
    fn army(&mut self, id: ArmyId) -> Result<Army> {
        self.own(id).copied()
    }
    fn own_armies(&mut self) -> Result<Vec<Army>> {
        Ok(self.own_armies.clone())
    }
    fn province_armies(&mut self, province: ProvinceId) -> Result<Vec<Army>> {
        if !self.session.visibility_current {
            return Err(Error::new(Status::NotReady, "visibility is not current yet"));
        }
        let visible = self.provinces.get(province as usize).is_some_and(|p| p.visible);
        Ok(if visible { self.province_armies.get(&province).cloned().unwrap_or_default() } else { Vec::new() })
    }
    fn army_route(&mut self, id: ArmyId) -> Result<Vec<ProvinceId>> {
        self.own(id)?;
        Ok(self.routes_of_armies.get(&id).cloned().unwrap_or_default())
    }
    fn army_can_enter(&mut self, id: ArmyId, province: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(!self.closed.contains(&(id, province)))
    }
    fn army_strait_open(&mut self, id: ArmyId, from: ProvinceId, to: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(!self.closed_straits.contains(&(from, to)))
    }
    fn army_can_attack(&mut self, id: ArmyId, target: ArmyId, _province: ProvinceId) -> Result<bool> {
        self.own(id)?;
        Ok(id != target && !self.protected.contains(&(id, target)))
    }
    fn find_route(&mut self, id: ArmyId, goal: ProvinceId) -> Result<Vec<ProvinceId>> {
        let army = *self.own(id)?;
        if goal == army.province {
            return Ok(Vec::new());
        }
        self.routes.get(&(id, goal)).cloned().ok_or_else(|| not_found(format!("no route to {goal}")))
    }
    fn army_move(&mut self, id: ArmyId, goal: ProvinceId) -> Result<()> {
        self.own(id)?;
        if let Some(reason) = &self.reject_orders {
            return Err(Error::new(Status::Rejected, reason.clone()));
        }
        self.commands.push(Command::Move(id, goal));
        Ok(())
    }
    fn army_stop(&mut self, id: ArmyId) -> Result<()> {
        self.own(id)?;
        if let Some(reason) = &self.reject_orders {
            return Err(Error::new(Status::Rejected, reason.clone()));
        }
        self.commands.push(Command::Stop(id));
        Ok(())
    }
    fn ui_selected_army(&mut self) -> Result<ArmyId> {
        self.selected.ok_or_else(|| not_found("no army selected"))
    }
    fn ui_set_visible(&mut self, element: &str, visible: bool) -> Result<()> {
        self.elements.entry(element.into()).or_default().visible = Some(visible);
        Ok(())
    }
    fn ui_set_enabled(&mut self, element: &str, enabled: bool) -> Result<()> {
        self.elements.entry(element.into()).or_default().enabled = Some(enabled);
        Ok(())
    }
    fn ui_set_text(&mut self, element: &str, text: &str) -> Result<()> {
        self.elements.entry(element.into()).or_default().text = Some(text.into());
        Ok(())
    }
    fn ui_set_tooltip(&mut self, element: &str, text: &str) -> Result<()> {
        self.elements.entry(element.into()).or_default().tooltip = Some(text.into());
        Ok(())
    }
    fn map_paint(&mut self, colors: &[MapColor]) -> Result<()> {
        self.map = Some(colors.to_vec());
        Ok(())
    }
    fn map_clear(&mut self) -> Result<()> {
        self.map = None;
        Ok(())
    }
    fn save_data_get(&mut self) -> Result<String> {
        Ok(self.save_data.clone())
    }
    fn save_data_set(&mut self, json: &str) -> Result<()> {
        self.save_data = json.into();
        Ok(())
    }
    fn language(&mut self) -> Result<String> {
        Ok(if self.language.is_empty() { "english".into() } else { self.language.clone() })
    }
    fn text(&mut self, key: &str) -> Result<String> {
        self.texts.get(key).cloned().ok_or_else(|| not_found(format!("no localisation text for {key}")))
    }
}
