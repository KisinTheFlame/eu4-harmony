//! The settings window's language and fonts page: the player language, and
//! for each font role the system font families offered and the weights of
//! the one chosen. What the page offers depends on the fonts installed, which
//! it reaches through [`FontSystem`], so that it is built, changed and read
//! back the same without DirectWrite.
//!
//! A font the player leaves as it showed stays as it was: chosen, or
//! following the language. The page shows each role's face in the weight
//! that draws it, so a default asking for a weight its family lacks still
//! reads back as left alone.
use super::widgets::{Widget, Window};
use super::{page_note, setting_widgets};
use crate::settings::{Page, Role, Settings};
use crate::unicode::system_font::{self, Face, SystemFonts, REGULAR};
use eu4_harmony_loc::Language;

/// What the page asks of the system's fonts.
pub trait FontSystem {
    /// The weights `family` comes in, lightest first, each with its English
    /// name; none when it is not installed.
    fn weights(&self, family: &str) -> Vec<(u16, String)>;

    /// Installed families that have every character of `text`, as (English
    /// name, name in `language`), sorted by the latter.
    fn families_with(&self, text: &str, language: Language) -> Vec<(String, String)>;

    /// Whether `family` is installed.
    fn has_family(&self, family: &str) -> bool {
        !self.weights(family).is_empty()
    }

    /// `face` as it is drawn: in the weight of its family that draws it;
    /// None when the family is not installed.
    fn drawn(&self, face: &Face) -> Option<Face> {
        let have: Vec<u16> = self.weights(&face.family).into_iter().map(|(w, _)| w).collect();
        system_font::drawn_in(&have, face)
    }
}

impl FontSystem for SystemFonts {
    fn weights(&self, family: &str) -> Vec<(u16, String)> {
        SystemFonts::weights(self, family)
    }

    fn families_with(&self, text: &str, language: Language) -> Vec<(String, String)> {
        SystemFonts::families_with(self, text, language)
    }

    fn has_family(&self, family: &str) -> bool {
        SystemFonts::has_family(self, family)
    }
}

/// The system fonts when they could be reached; with none, each role offers
/// the face it shows, and that face's weight.
impl<F: FontSystem> FontSystem for Option<F> {
    fn weights(&self, family: &str) -> Vec<(u16, String)> {
        self.as_ref().map(|f| f.weights(family)).unwrap_or_default()
    }

    fn families_with(&self, text: &str, language: Language) -> Vec<(String, String)> {
        self.as_ref().map(|f| f.families_with(text, language)).unwrap_or_default()
    }

    fn has_family(&self, family: &str) -> bool {
        self.as_ref().is_some_and(|f| f.has_family(family))
    }
}

/// A font role's drop-down lists on the page: its family's and its weight's.
fn ids(role: Role) -> (&'static str, &'static str) {
    match role {
        Role::Body => ("body", "body_weight"),
        Role::Heading => ("heading", "heading_weight"),
        Role::Map => ("map", "map_weight"),
        Role::Console => ("console", "console_weight"),
    }
}

/// The keys of the words over a font role's drop-down lists.
fn labels(role: Role) -> (&'static str, &'static str) {
    match role {
        Role::Body => ("EU4HARMONY_SETTINGS_BODY_FONT", "EU4HARMONY_SETTINGS_BODY_WEIGHT"),
        Role::Heading => ("EU4HARMONY_SETTINGS_HEADING_FONT", "EU4HARMONY_SETTINGS_HEADING_WEIGHT"),
        Role::Map => ("EU4HARMONY_SETTINGS_MAP_FONT", "EU4HARMONY_SETTINGS_MAP_WEIGHT"),
        Role::Console => ("EU4HARMONY_SETTINGS_CONSOLE_FONT", "EU4HARMONY_SETTINGS_CONSOLE_WEIGHT"),
    }
}

/// What the page offers for one font role.
struct Lists {
    /// The families offered, by English name, in their drop-down list's order.
    families: Vec<String>,
    /// The same, as the player language names them.
    names: Vec<String>,
    /// The weights offered for the family selected, in their list's order.
    weights: Vec<u16>,
    /// The face the role showed when the page was made, in the weight that
    /// draws it.
    shown: Face,
}

/// The language and fonts page of one opening of the settings window.
pub struct FontsPage<F> {
    fonts: F,
    language: Language,
    /// By role, in [`Role::ALL`]'s order.
    roles: Vec<Lists>,
}

fn at(role: Role) -> usize {
    Role::ALL.iter().position(|&r| r == role).expect("every role is in Role::ALL")
}

impl<F: FontSystem> FontsPage<F> {
    /// The page showing `next`, the settings the next start runs with, as
    /// `fonts` draw them.
    pub fn new(fonts: F, next: &Settings) -> Self {
        let language = next.language();
        let shown: Vec<Face> = Role::ALL
            .into_iter()
            .map(|role| {
                let face = next.face(role, |f| fonts.has_family(f));
                fonts.drawn(&face).unwrap_or(face)
            })
            .collect();
        // The installed families that draw `text`, by English name and as
        // shown in the language, and those chosen or shown that do not.
        let offered = |text: &str, roles: &[Role]| {
            let (mut families, mut names): (Vec<String>, Vec<String>) = fonts.families_with(text, language).into_iter().unzip();
            for family in roles.iter().flat_map(|&role| next.chosen(role).into_iter().chain([&shown[at(role)]])).map(|face| &face.family) {
                if !families.contains(family) {
                    families.push(family.clone());
                    names.push(family.clone());
                }
            }
            (families, names)
        };
        // Fonts that write the language's name; the game console's needs
        // only Latin letters, as other fonts draw the rest.
        let written = offered(language.native_name(), &[Role::Body, Role::Heading, Role::Map]);
        let latin = offered("A", &[Role::Console]);
        let roles = Role::ALL
            .into_iter()
            .zip(shown)
            .map(|(role, shown)| {
                let (families, names) = if role == Role::Console { latin.clone() } else { written.clone() };
                let (weights, ..) = weight_options(&fonts, &shown);
                Lists { families, names, weights, shown }
            })
            .collect();
        Self { fonts, language, roles }
    }

    fn lists(&self, role: Role) -> &Lists {
        &self.roles[at(role)]
    }

    /// The page's widgets, showing `next` as [`FontsPage::new`] was given it.
    pub fn widgets(&self, next: &Settings, words: &dyn Fn(&str) -> String) -> Vec<Widget> {
        let mut out = vec![
            Widget::Label(words("EU4HARMONY_SETTINGS_LANGUAGE")),
            Widget::Dropdown {
                id: "language",
                options: Language::ALL.iter().map(|l| l.native_name().to_string()).collect(),
                selected: Language::ALL.iter().position(|&l| l == self.language).unwrap_or(0),
            },
        ];
        for role in [Role::Body, Role::Heading, Role::Map] {
            out.extend(self.role_widgets(role, words));
        }
        // The page's own settings, the map outline, follow the map's font.
        out.extend(setting_widgets(Page::LanguageFonts, next, words));
        out.extend(self.role_widgets(Role::Console, words));
        out.extend(page_note(Page::LanguageFonts, words));
        out
    }

    fn role_widgets(&self, role: Role, words: &dyn Fn(&str) -> String) -> [Widget; 4] {
        let ((family_id, weight_id), (family_label, weight_label)) = (ids(role), labels(role));
        let lists = self.lists(role);
        let (_, names, selected) = weight_options(&self.fonts, &lists.shown);
        [
            Widget::Label(words(family_label)),
            Widget::Dropdown {
                id: family_id,
                options: lists.names.clone(),
                selected: lists.families.iter().position(|f| *f == lists.shown.family).unwrap_or(0),
            },
            Widget::Label(words(weight_label)),
            Widget::Dropdown { id: weight_id, options: names, selected },
        ]
    }

    /// The player chose the option at `index` in the drop-down list `id` of
    /// `window`, the window showing the page. A family's makes its weight
    /// list the new family's, keeping the weight chosen when the family has
    /// it, else the one below.
    pub fn chose(&mut self, window: &mut Window, id: &str, index: usize) {
        let Some(role) = Role::ALL.into_iter().find(|&role| ids(role).0 == id) else { return };
        let (_, weight_id) = ids(role);
        let lists = self.lists(role);
        let Some(family) = lists.families.get(index).cloned() else { return };
        let chosen = window.selected(weight_id).and_then(|i| lists.weights.get(i).copied()).unwrap_or(REGULAR);
        let (weights, names, selected) = weight_options(&self.fonts, &Face::new(family, chosen));
        window.set_options(weight_id, names, selected);
        self.roles[at(role)].weights = weights;
    }

    /// Takes into `chosen`, the settings the page was made from, the
    /// language and the fonts `window` shows. A font left as it showed stays
    /// as it was in `chosen`.
    pub fn read_back(&self, window: &Window, chosen: &mut Settings) {
        if let Some(&language) = window.selected("language").and_then(|i| Language::ALL.get(i)) {
            chosen.language = Some(language);
        }
        for role in Role::ALL {
            let ((family_id, weight_id), lists) = (ids(role), self.lists(role));
            let family = window.selected(family_id).and_then(|i| lists.families.get(i)).cloned().unwrap_or_else(|| lists.shown.family.clone());
            let weight = window.selected(weight_id).and_then(|i| lists.weights.get(i).copied()).unwrap_or(lists.shown.weight);
            let face = Face::new(family, weight);
            if face != lists.shown {
                *chosen.chosen_mut(role) = Some(face);
            }
        }
    }
}

/// The weights offered for `face`'s family, their names, and which of them
/// draws `face`. A family not installed offers the weight chosen for it.
fn weight_options(fonts: &impl FontSystem, face: &Face) -> (Vec<u16>, Vec<String>, usize) {
    let offered = fonts.weights(&face.family);
    if offered.is_empty() {
        return (vec![face.weight], vec![system_font::weight_name(face.weight).to_string()], 0);
    }
    let drawn = fonts.drawn(face).map_or(face.weight, |d| d.weight);
    let selected = offered.iter().position(|(w, _)| *w == drawn).unwrap_or(0);
    let (weights, names) = offered.into_iter().unzip();
    (weights, names, selected)
}

/// A fixed table of installed fonts, for tests.
#[cfg(test)]
pub struct Table(pub Vec<(&'static str, &'static [u16], Writes)>);

/// Which text a font in a [`Table`] has the characters of.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Writes {
    Latin,
    Every,
}

#[cfg(test)]
impl FontSystem for Table {
    fn weights(&self, family: &str) -> Vec<(u16, String)> {
        self.0.iter().filter(|(f, ..)| *f == family).flat_map(|(_, weights, _)| weights.iter()).map(|&w| (w, system_font::weight_name(w).to_string())).collect()
    }

    fn families_with(&self, text: &str, _: Language) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> =
            self.0.iter().filter(|(.., writes)| *writes == Writes::Every || text.is_ascii()).map(|(f, ..)| (f.to_string(), f.to_string())).collect();
        out.sort();
        out
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Fonts as a test lays them out. Segoe UI lacks 600, the weight of the
    /// Latin languages' headings.
    pub fn installed() -> Table {
        use Writes::*;
        Table(vec![
            ("Segoe UI", &[300, 400, 700], Every),
            ("Georgia", &[400, 700], Latin),
            ("Consolas", &[400, 700], Latin),
            ("Cascadia Mono", &[200, 300, 400, 600, 700], Latin),
            ("Microsoft YaHei", &[300, 400, 700], Every),
            ("KaiTi", &[400], Every),
            ("Microsoft JhengHei", &[300, 400, 700], Every),
            ("Yu Gothic UI", &[300, 400, 500, 600, 700], Every),
            ("Malgun Gothic", &[300, 400, 700], Every),
            ("Heavy Sans", &[800, 900], Every),
        ])
    }

    fn keys(key: &str) -> String {
        key.to_string()
    }

    /// The page, and a window showing it alone.
    fn opened(next: &Settings) -> (FontsPage<Table>, Window) {
        let page = FontsPage::new(installed(), next);
        let window = Window::new("", page.widgets(next, &keys), 600.0);
        (page, window)
    }

    /// The options of the drop-down list `id`.
    fn options(window: &Window, id: &str) -> Vec<String> {
        window
            .body
            .iter()
            .find_map(|widget| match widget {
                Widget::Dropdown { id: i, options, .. } if *i == id => Some(options.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Selects the option `name` of the drop-down list `id`.
    fn select(window: &mut Window, id: &str, name: &str) -> usize {
        let options = options(window, id);
        let index = options.iter().position(|o| o == name).unwrap_or_else(|| panic!("{id} offers no {name}: {options:?}"));
        window.set_options(id, options, index);
        index
    }

    #[test]
    fn a_family_chosen_keeps_its_weight_or_takes_the_one_below() {
        let english = Settings { language: Some(Language::English), ..Settings::default() };
        let (mut page, mut window) = opened(&english);
        // Bold Segoe UI, then a family that has bold: still bold.
        select(&mut window, "body_weight", "Bold");
        let georgia = select(&mut window, "body", "Georgia");
        page.chose(&mut window, "body", georgia);
        assert_eq!(options(&window, "body_weight"), ["Regular", "Bold"]);
        assert_eq!(window.selected("body_weight"), Some(1));
        // KaiTi has only regular, the one below bold.
        let kaiti = select(&mut window, "body", "KaiTi");
        page.chose(&mut window, "body", kaiti);
        assert_eq!((options(&window, "body_weight"), window.selected("body_weight")), (vec!["Regular".to_string()], Some(0)));
        // From regular to a family with nothing so light: its lightest.
        let heavy = select(&mut window, "body", "Heavy Sans");
        page.chose(&mut window, "body", heavy);
        assert_eq!((options(&window, "body_weight"), window.selected("body_weight")), (vec!["ExtraBold".to_string(), "Black".to_string()], Some(0)));
        // What the window then shows is what is saved.
        let mut chosen = english.clone();
        page.read_back(&window, &mut chosen);
        assert_eq!(chosen.body_font, Some(Face::new("Heavy Sans", 800)));
        assert_eq!((chosen.heading_font, chosen.map_font, chosen.console_font), (None, None, None));
        // Another list's choice changes no weight list.
        page.chose(&mut window, "language", 0);
        assert_eq!(options(&window, "body_weight"), ["ExtraBold", "Black"]);
    }

    #[test]
    fn a_heading_left_lighter_than_its_default_follows_the_language() {
        // English headings ask for Segoe UI 600, which the table lacks: the
        // page shows its regular.
        let english = Settings { language: Some(Language::English), ..Settings::default() };
        let (page, window) = opened(&english);
        assert_eq!(options(&window, "heading_weight")[window.selected("heading_weight").unwrap()], "Regular");
        let mut chosen = english.clone();
        page.read_back(&window, &mut chosen);
        assert_eq!(chosen, english);
    }

    #[test]
    fn a_font_offered_is_one_that_writes_the_language() {
        let chinese = Settings { language: Some(Language::SimpChinese), ..Settings::default() };
        let (_, window) = opened(&chinese);
        // Latin fonts are not offered for Chinese text, but are for the
        // game console.
        assert!(!options(&window, "body").contains(&"Georgia".to_string()));
        assert!(options(&window, "console").contains(&"Georgia".to_string()));
        // A chosen font not installed is offered with the rest.
        let gone = Settings { map_font: Some(Face::new("Gone", 700)), ..chinese };
        let (_, window) = opened(&gone);
        assert!(options(&window, "map").contains(&"Gone".to_string()));
        assert_eq!(options(&window, "map")[window.selected("map").unwrap()], "KaiTi");
    }
}
