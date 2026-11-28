//! The settings window's language and fonts page: the player language, and
//! for each font role the system font families offered, localized weights,
//! and a live preview box rendered in the selected font and preview language.
//!
//! Changing the language immediately refreshes all four font lists and preview
//! samples. Unchosen roles follow the new language's defaults; custom fonts
//! that cover the new language are preserved, while custom fonts lacking its
//! glyphs reset to the new language default.
use super::widgets::{Widget, Window};
use crate::base::settings::{self, Role, Settings};
use crate::text::unicode::system_font::{self, Face, SystemFonts, REGULAR};
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

/// A font role's widget IDs: (family dropdown, weight dropdown, preview box).
fn ids(role: Role) -> (&'static str, &'static str, &'static str) {
    match role {
        Role::Body => ("body", "body_weight", "preview_body"),
        Role::Heading => ("heading", "heading_weight", "preview_heading"),
        Role::Map => ("map", "map_weight", "preview_map"),
        Role::Console => ("console", "console_weight", "preview_console"),
    }
}

/// The keys of the words over a font role's drop-down lists and preview sample.
fn labels(role: Role) -> (&'static str, &'static str, &'static str, f32) {
    match role {
        Role::Body => ("EU4HARMONY_SETTINGS_BODY_FONT", "EU4HARMONY_SETTINGS_BODY_WEIGHT", "EU4HARMONY_PREVIEW_BODY", 16.0),
        Role::Heading => ("EU4HARMONY_SETTINGS_HEADING_FONT", "EU4HARMONY_SETTINGS_HEADING_WEIGHT", "EU4HARMONY_PREVIEW_HEADING", 20.0),
        Role::Map => ("EU4HARMONY_SETTINGS_MAP_FONT", "EU4HARMONY_SETTINGS_MAP_WEIGHT", "EU4HARMONY_PREVIEW_MAP", 20.0),
        Role::Console => ("EU4HARMONY_SETTINGS_CONSOLE_FONT", "EU4HARMONY_SETTINGS_CONSOLE_WEIGHT", "EU4HARMONY_PREVIEW_CONSOLE", 15.0),
    }
}

/// What the page offers and tracks for one font role.
struct Lists {
    /// The families offered, by English name, in their drop-down list's order.
    families: Vec<String>,
    /// The same, as the preview language names them. When [`Self::has_default`]
    /// is true, index 0's display text comes from localisation instead.
    names: Vec<String>,
    /// Whether index 0 is the Simplified Chinese bundled default entry.
    has_default: bool,
    /// The weights offered for the family selected, in their list's order.
    weights: Vec<u16>,
    /// The face the role initially showed when the page was made, in its drawn weight.
    shown: Face,
    /// Current face shown in the UI (updated on family/weight/language/reset changes).
    current: Face,
    /// Current custom override (`None` = follows `language` default).
    custom: Option<Face>,
    /// Whether this role has been explicitly changed since the page opened.
    dirty: bool,
}

/// The language and fonts page of one opening of the settings window.
pub struct FontsPage<F> {
    fonts: F,
    language: Language,
    map_outline: bool,
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
        let mut page = Self { fonts, language, map_outline: next.map_outline, roles: Vec::new() };
        page.roles = page.build_roles(language, next, false);
        page
    }

    fn build_roles(&self, language: Language, source: &Settings, dirty: bool) -> Vec<Lists> {
        let shown: Vec<Face> = Role::ALL
            .into_iter()
            .map(|role| {
                let face = source.face(role, |f| self.fonts.has_family(f));
                self.fonts.drawn(&face).unwrap_or(face)
            })
            .collect();
        let offered = |text: &str, roles: &[Role]| {
            let (mut families, mut names): (Vec<String>, Vec<String>) =
                self.fonts.families_with(text, language).into_iter().filter(|(english, _)| !settings::is_bundled_default_family(english)).unzip();
            for family in roles.iter().flat_map(|&role| source.chosen(role).into_iter().chain([&shown[at(role)]])).map(|face| &face.family) {
                if !settings::is_bundled_default_family(family) && !families.contains(family) {
                    families.push(family.clone());
                    names.push(family.clone());
                }
            }
            (families, names)
        };
        let written = offered(language.native_name(), &[Role::Body, Role::Heading, Role::Map]);
        let latin = offered("A", &[Role::Console]);
        Role::ALL
            .into_iter()
            .zip(shown)
            .map(|(role, shown)| {
                let (mut families, mut names) = if role == Role::Console { latin.clone() } else { written.clone() };
                let default_family = (language == Language::SimpChinese)
                    .then(|| settings::bundled_default_family(role))
                    .flatten()
                    .filter(|family| self.fonts.has_family(family));
                let has_default = if let Some(family) = default_family {
                    families.insert(0, family.to_string());
                    names.insert(0, String::new());
                    true
                } else {
                    false
                };
                let (weights, ..) = weight_options(&self.fonts, &shown, &|k| k.to_string());
                Lists { families, names, has_default, weights, current: shown.clone(), shown, custom: source.chosen(role).cloned(), dirty }
            })
            .collect()
    }

    fn lists(&self, role: Role) -> &Lists {
        &self.roles[at(role)]
    }

    /// Associates self-rendering faces with each role's family dropdown in `window`.
    pub fn apply_option_faces(&self, window: &mut Window) {
        for role in Role::ALL {
            let (family_id, _, _) = ids(role);
            let faces: Vec<Option<Face>> = self.lists(role).families.iter().map(|family| Some(Face::regular(family.clone()))).collect();
            window.set_option_faces(family_id, faces);
        }
    }

    /// The page's widgets, showing `next` as [`FontsPage::new`] was given it.
    pub fn widgets(&self, next: &Settings, words: &dyn Fn(&str) -> String, words_in: &dyn Fn(Language, &str) -> String) -> Vec<Widget> {
        let mut out = vec![
            Widget::PageHeader(words("EU4HARMONY_SETTINGS_PAGE_LANGUAGE_FONTS")),
            Widget::Labeled {
                label: words("EU4HARMONY_SETTINGS_LANGUAGE"),
                status: None,
                control: Box::new(Widget::Dropdown {
                    id: "language",
                    options: Language::ALL.iter().map(|l| l.native_name().to_string()).collect(),
                    selected: Language::ALL.iter().position(|&l| l == self.language).unwrap_or(0),
                }),
            },
            Widget::Gap,
        ];
        for role in Role::ALL {
            out.push(self.role_card(role, next, words, words_in));
        }
        out
    }

    fn role_card(&self, role: Role, next: &Settings, words: &dyn Fn(&str) -> String, words_in: &dyn Fn(Language, &str) -> String) -> Widget {
        let (family_id, weight_id, preview_id) = ids(role);
        let (family_label, _, preview_key, preview_size) = labels(role);
        let lists = self.lists(role);
        let (_, weight_names, weight_selected) = weight_options(&self.fonts, &lists.current, words);
        let mut options = lists.names.clone();
        if lists.has_default {
            options[0] = words("EU4HARMONY_SETTINGS_FONT_DEFAULT");
        }
        let family_selected = lists.families.iter().position(|f| *f == lists.current.family).unwrap_or(0);
        let outline_toggle = (role == Role::Map).then(|| ("map_outline", words("EU4HARMONY_SETTINGS_MAP_OUTLINE"), next.map_outline));
        Widget::FontGroup {
            role_label: words(family_label),
            family_id,
            families: options,
            family_selected,
            weight_id,
            weights: weight_names,
            weight_selected,
            preview_id,
            preview_text: words_in(self.language, preview_key),
            preview_face: lists.current.clone(),
            preview_size,
            preview_outline: role == Role::Map && self.map_outline,
            outline_toggle,
        }
    }

    /// Updates the map preview outline when the `map_outline` toggle changes.
    pub fn set_map_outline(&mut self, window: &mut Window, outline: bool) {
        self.map_outline = outline;
        window.set_preview(ids(Role::Map).2, None, None, Some(outline));
    }

    /// Resets the four font roles and map outline to defaults while preserving
    /// the currently selected language.
    pub fn reset_defaults(&mut self, window: &mut Window, words: &dyn Fn(&str) -> String, words_in: &dyn Fn(Language, &str) -> String) {
        self.map_outline = Settings::default().map_outline;
        window.set_toggle("map_outline", self.map_outline);
        let reset = Settings { language: Some(self.language), ..Settings::default() };
        self.roles = self.build_roles(self.language, &reset, true);
        self.sync_window(window, words, words_in);
    }

    /// Pushes all four role dropdowns, option faces, and live previews to `window`.
    fn sync_window(&mut self, window: &mut Window, words: &dyn Fn(&str) -> String, words_in: &dyn Fn(Language, &str) -> String) {
        for role in Role::ALL {
            let (family_id, weight_id, preview_id) = ids(role);
            let (_, _, preview_key, _) = labels(role);
            let lists = &mut self.roles[at(role)];
            let mut options = lists.names.clone();
            if lists.has_default {
                options[0] = words("EU4HARMONY_SETTINGS_FONT_DEFAULT");
            }
            let family_selected = lists.families.iter().position(|f| *f == lists.current.family).unwrap_or(0);
            let (weights, weight_names, weight_selected) = weight_options(&self.fonts, &lists.current, words);
            lists.weights = weights;
            let faces: Vec<Option<Face>> = lists.families.iter().map(|family| Some(Face::regular(family.clone()))).collect();
            window.set_options(family_id, options, family_selected);
            window.set_option_faces(family_id, faces);
            window.set_options(weight_id, weight_names, weight_selected);
            window.set_preview(
                preview_id,
                Some(words_in(self.language, preview_key)),
                Some(lists.current.clone()),
                Some(role == Role::Map && self.map_outline),
            );
        }
    }

    /// Handles a selection in `language`, any font family, or any font weight
    /// dropdown in `window`.
    pub fn chose(&mut self, window: &mut Window, id: &str, index: usize, words: &dyn Fn(&str) -> String, words_in: &dyn Fn(Language, &str) -> String) {
        if id == "language" {
            let Some(&new_lang) = Language::ALL.get(index) else { return };
            if new_lang == self.language {
                return;
            }
            self.language = new_lang;
            // Rebuild offered lists for the new language: keep custom fonts
            // that cover the new language; reset unsupported custom fonts to
            // the new language's default.
            let supported_written: Vec<String> = self
                .fonts
                .families_with(new_lang.native_name(), new_lang)
                .into_iter()
                .filter(|(english, _)| !settings::is_bundled_default_family(english))
                .map(|(english, _)| english)
                .collect();
            let supported_latin: Vec<String> = self
                .fonts
                .families_with("A", new_lang)
                .into_iter()
                .filter(|(english, _)| !settings::is_bundled_default_family(english))
                .map(|(english, _)| english)
                .collect();
            let mut next = Settings { language: Some(new_lang), map_outline: self.map_outline, ..Settings::default() };
            let mut dirty_flags = [false; 4];
            for role in Role::ALL {
                let idx = at(role);
                let prev = &self.roles[idx];
                let supported = if role == Role::Console { &supported_latin } else { &supported_written };
                if let Some(custom) = &prev.custom {
                    if supported.contains(&custom.family) {
                        *next.chosen_mut(role) = Some(custom.clone());
                        dirty_flags[idx] = prev.dirty;
                    } else {
                        // Custom font lacks glyphs for the new language: reset to default.
                        *next.chosen_mut(role) = None;
                        dirty_flags[idx] = true;
                    }
                } else {
                    *next.chosen_mut(role) = None;
                    dirty_flags[idx] = prev.dirty;
                }
            }
            let mut rebuilt = self.build_roles(new_lang, &next, false);
            for (role, dirty) in rebuilt.iter_mut().zip(dirty_flags) {
                role.dirty = dirty;
            }
            self.roles = rebuilt;
            self.sync_window(window, words, words_in);
            return;
        }
        if let Some(role) = Role::ALL.into_iter().find(|&role| ids(role).0 == id) {
            let (_, weight_id, preview_id) = ids(role);
            let idx = at(role);
            let Some(family) = self.roles[idx].families.get(index).cloned() else { return };
            let prev_weight = window.selected(weight_id).and_then(|i| self.roles[idx].weights.get(i).copied()).unwrap_or(REGULAR);
            let (weights, names, selected) = weight_options(&self.fonts, &Face::new(family.clone(), prev_weight), words);
            let drawn_weight = weights.get(selected).copied().unwrap_or(prev_weight);
            let face = Face::new(family, drawn_weight);
            window.set_options(weight_id, names, selected);
            let is_bundled_default = self.roles[idx].has_default && index == 0;
            self.roles[idx].weights = weights;
            self.roles[idx].current = face.clone();
            self.roles[idx].custom = if is_bundled_default { None } else { Some(face.clone()) };
            self.roles[idx].dirty = true;
            window.set_preview(preview_id, None, Some(face), None);
            return;
        }
        if let Some(role) = Role::ALL.into_iter().find(|&role| ids(role).1 == id) {
            let (family_id, _, preview_id) = ids(role);
            let idx = at(role);
            let Some(&weight) = self.roles[idx].weights.get(index) else { return };
            let family_idx = window.selected(family_id).unwrap_or(0);
            let face = Face::new(self.roles[idx].current.family.clone(), weight);
            let is_bundled_default = self.roles[idx].has_default && family_idx == 0;
            self.roles[idx].current = face.clone();
            self.roles[idx].custom = if is_bundled_default { None } else { Some(face.clone()) };
            self.roles[idx].dirty = true;
            window.set_preview(preview_id, None, Some(face), None);
        }
    }

    /// Takes into `chosen`, the settings the page was made from, the
    /// language and the fonts `window` shows. A font left untouched stays as
    /// it was in `chosen`.
    pub fn read_back(&self, window: &Window, chosen: &mut Settings) {
        if let Some(&language) = window.selected("language").and_then(|i| Language::ALL.get(i)) {
            chosen.language = Some(language);
        }
        for role in Role::ALL {
            let (family_id, weight_id, _) = ids(role);
            let lists = self.lists(role);
            let selected = window.selected(family_id);
            let family = selected.and_then(|i| lists.families.get(i)).cloned().unwrap_or_else(|| lists.current.family.clone());
            let weight = window.selected(weight_id).and_then(|i| lists.weights.get(i).copied()).unwrap_or(lists.current.weight);
            let face = Face::new(family, weight);
            if lists.dirty {
                *chosen.chosen_mut(role) = if lists.custom.is_none() || (lists.has_default && selected == Some(0)) { None } else { Some(face) };
            } else if face != lists.shown {
                *chosen.chosen_mut(role) = if lists.has_default && selected == Some(0) { None } else { Some(face) };
            }
        }
    }
}

/// The weights offered for `face`'s family, their localized names, and which
/// of them draws `face`. A family not installed offers the weight chosen for it.
fn weight_options(fonts: &impl FontSystem, face: &Face, words: &dyn Fn(&str) -> String) -> (Vec<u16>, Vec<String>, usize) {
    let offered = fonts.weights(&face.family);
    if offered.is_empty() {
        return (vec![face.weight], vec![words(system_font::weight_key(face.weight))], 0);
    }
    let drawn = fonts.drawn(face).map_or(face.weight, |d| d.weight);
    let selected = offered.iter().position(|(w, _)| *w == drawn).unwrap_or(0);
    let weights: Vec<u16> = offered.iter().map(|(w, _)| *w).collect();
    let raw_labels: Vec<String> = weights.iter().map(|&w| words(system_font::weight_key(w))).collect();
    let names: Vec<String> = weights
        .iter()
        .zip(&raw_labels)
        .map(|(&w, label)| if raw_labels.iter().filter(|l| *l == label).count() > 1 { format!("{label} ({w})") } else { label.clone() })
        .collect();
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

    fn keys_in(language: Language, key: &str) -> String {
        format!("{}:{key}", language.id())
    }

    /// The page, and a window showing it alone.
    fn opened(next: &Settings) -> (FontsPage<Table>, Window) {
        let page = FontsPage::new(installed(), next);
        let mut window = Window::new("", page.widgets(next, &keys, &keys_in), 600.0);
        page.apply_option_faces(&mut window);
        (page, window)
    }

    /// The options of the drop-down list `id`.
    fn options(window: &Window, id: &str) -> Vec<String> {
        window
            .body
            .iter()
            .find_map(|widget| match widget {
                Widget::Dropdown { id: i, options, .. } if *i == id => Some(options.clone()),
                Widget::Labeled { control, .. } => match control.as_ref() {
                    Widget::Dropdown { id: i, options, .. } if *i == id => Some(options.clone()),
                    _ => None,
                },
                Widget::FontGroup { family_id, families, weight_id, weights, .. } => {
                    if *family_id == id {
                        Some(families.clone())
                    } else if *weight_id == id {
                        Some(weights.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    fn preview(window: &Window, id: &str) -> (String, Face, bool) {
        window
            .body
            .iter()
            .find_map(|widget| match widget {
                Widget::FontGroup { preview_id: i, preview_text: text, preview_face: face, preview_outline: outline, .. } if *i == id => {
                    Some((text.clone(), face.clone(), *outline))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing preview {id}"))
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
        let bold = select(&mut window, "body_weight", "EU4HARMONY_WEIGHT_700");
        page.chose(&mut window, "body_weight", bold, &keys, &keys_in);
        let georgia = select(&mut window, "body", "Georgia");
        page.chose(&mut window, "body", georgia, &keys, &keys_in);
        assert_eq!(options(&window, "body_weight"), ["EU4HARMONY_WEIGHT_400", "EU4HARMONY_WEIGHT_700"]);
        assert_eq!(window.selected("body_weight"), Some(1));
        assert_eq!(preview(&window, "preview_body").1, Face::new("Georgia", 700));
        let kaiti = select(&mut window, "body", "KaiTi");
        page.chose(&mut window, "body", kaiti, &keys, &keys_in);
        assert_eq!((options(&window, "body_weight"), window.selected("body_weight")), (vec!["EU4HARMONY_WEIGHT_400".to_string()], Some(0)));
        let heavy = select(&mut window, "body", "Heavy Sans");
        page.chose(&mut window, "body", heavy, &keys, &keys_in);
        assert_eq!(
            (options(&window, "body_weight"), window.selected("body_weight")),
            (vec!["EU4HARMONY_WEIGHT_800".to_string(), "EU4HARMONY_WEIGHT_900".to_string()], Some(0))
        );
        let mut chosen = english.clone();
        page.read_back(&window, &mut chosen);
        assert_eq!(chosen.body_font, Some(Face::new("Heavy Sans", 800)));
        assert_eq!((chosen.heading_font, chosen.map_font, chosen.console_font), (None, None, None));
        // Selecting the same language leaves the weight list untouched.
        let en_idx = Language::ALL.iter().position(|&l| l == Language::English).unwrap();
        page.chose(&mut window, "language", en_idx, &keys, &keys_in);
        assert_eq!(options(&window, "body_weight"), ["EU4HARMONY_WEIGHT_800", "EU4HARMONY_WEIGHT_900"]);
    }

    #[test]
    fn switching_language_refreshes_font_lists_keeps_supported_custom_and_resets_unsupported() {
        let english = Settings {
            language: Some(Language::English),
            // Georgia is Latin-only; Microsoft YaHei writes Every language.
            body_font: Some(Face::new("Georgia", 700)),
            heading_font: Some(Face::new("Microsoft YaHei", 700)),
            ..Settings::default()
        };
        let (mut page, mut window) = opened_with(installed_with_bundled(), &english);
        assert!(options(&window, "body").contains(&"Georgia".to_string()));
        let zh_idx = Language::ALL.iter().position(|&l| l == Language::SimpChinese).unwrap();
        window.set_options("language", Language::ALL.iter().map(|l| l.native_name().to_string()).collect(), zh_idx);
        page.chose(&mut window, "language", zh_idx, &keys, &keys_in);
        // Body list now hides Latin-only Georgia and puts Default at top; Georgia reset to default.
        assert!(!options(&window, "body").contains(&"Georgia".to_string()));
        assert_eq!(options(&window, "body").first().map(String::as_str), Some("EU4HARMONY_SETTINGS_FONT_DEFAULT"));
        assert_eq!(window.selected("body"), Some(0));
        // Heading custom font (Microsoft YaHei 700) covers Chinese and is preserved.
        assert_eq!(options(&window, "heading")[window.selected("heading").unwrap()], "Microsoft YaHei");
        assert_eq!(preview(&window, "preview_heading").1, Face::new("Microsoft YaHei", 700));
        assert_eq!(preview(&window, "preview_body").0, "simp_chinese:EU4HARMONY_PREVIEW_BODY");
        let mut saved = english.clone();
        page.read_back(&window, &mut saved);
        assert_eq!(saved.language, Some(Language::SimpChinese));
        assert_eq!(saved.body_font, None);
        assert_eq!(saved.heading_font, Some(Face::new("Microsoft YaHei", 700)));
    }

    #[test]
    fn reset_defaults_keeps_language_and_resets_fonts_and_map_outline() {
        let custom = Settings { language: Some(Language::Japanese), body_font: Some(Face::new("Heavy Sans", 900)), map_outline: false, ..Settings::default() };
        let (mut page, mut window) = opened(&custom);
        assert!(!preview(&window, "preview_map").2);
        page.reset_defaults(&mut window, &keys, &keys_in);
        assert!(preview(&window, "preview_map").2);
        assert_eq!(window.is_on("map_outline"), Some(true));
        let mut saved = custom.clone();
        page.read_back(&window, &mut saved);
        assert_eq!(saved.language, Some(Language::Japanese));
        assert_eq!(saved.body_font, None);
    }

    #[test]
    fn a_heading_left_lighter_than_its_default_follows_the_language() {
        let english = Settings { language: Some(Language::English), ..Settings::default() };
        let (page, window) = opened(&english);
        assert_eq!(options(&window, "heading_weight")[window.selected("heading_weight").unwrap()], "EU4HARMONY_WEIGHT_400");
        let mut chosen = english.clone();
        page.read_back(&window, &mut chosen);
        assert_eq!(chosen, english);
    }

    #[test]
    fn a_font_offered_is_one_that_writes_the_language() {
        let chinese = Settings { language: Some(Language::SimpChinese), ..Settings::default() };
        let (_, window) = opened(&chinese);
        assert!(!options(&window, "body").contains(&"Georgia".to_string()));
        assert!(options(&window, "console").contains(&"Georgia".to_string()));
        let gone = Settings { map_font: Some(Face::new("Gone", 700)), ..chinese };
        let (_, window) = opened(&gone);
        assert!(options(&window, "map").contains(&"Gone".to_string()));
        assert_eq!(options(&window, "map")[window.selected("map").unwrap()], "KaiTi");
    }

    fn installed_with_bundled() -> Table {
        let mut table = installed().0;
        table.extend([
            ("FZPingXianYaSongS-R-GB", &[400][..], Writes::Every),
            ("FZYaSongS-B-GB", &[400][..], Writes::Every),
            ("FZShengShiKaiShuS-EB-GB", &[400][..], Writes::Every),
        ]);
        Table(table)
    }

    fn opened_with(table: Table, next: &Settings) -> (FontsPage<Table>, Window) {
        let page = FontsPage::new(table, next);
        let mut window = Window::new("", page.widgets(next, &keys, &keys_in), 600.0);
        page.apply_option_faces(&mut window);
        (page, window)
    }

    #[test]
    fn bundled_chinese_defaults_hide_real_names_and_offer_top_default_only_in_simplified_chinese() {
        let real_names = ["FZPingXianYaSongS-R-GB", "FZYaSongS-B-GB", "FZShengShiKaiShuS-EB-GB"];
        let chinese = Settings { language: Some(Language::SimpChinese), ..Settings::default() };
        let (page, window) = opened_with(installed_with_bundled(), &chinese);
        for id in ["body", "heading", "map", "console"] {
            let list = options(&window, id);
            for real in real_names {
                assert!(!list.contains(&real.to_string()), "{id} must not expose {real}: {list:?}");
            }
        }
        for id in ["body", "heading", "map"] {
            let list = options(&window, id);
            assert_eq!(list.first().map(String::as_str), Some("EU4HARMONY_SETTINGS_FONT_DEFAULT"), "{id}: {list:?}");
            assert_eq!(window.selected(id), Some(0), "{id} starts on Default");
        }
        assert!(!options(&window, "console").contains(&"EU4HARMONY_SETTINGS_FONT_DEFAULT".to_string()));
        let mut unchanged = chinese.clone();
        page.read_back(&window, &mut unchanged);
        assert_eq!(unchanged, chinese);

        let custom = Settings { body_font: Some(Face::new("Microsoft YaHei", 700)), ..chinese.clone() };
        let (mut page, mut window) = opened_with(installed_with_bundled(), &custom);
        assert_eq!(options(&window, "body")[window.selected("body").unwrap()], "Microsoft YaHei");
        let default_index = select(&mut window, "body", "EU4HARMONY_SETTINGS_FONT_DEFAULT");
        assert_eq!(default_index, 0);
        page.chose(&mut window, "body", default_index, &keys, &keys_in);
        assert_eq!((options(&window, "body_weight"), window.selected("body_weight")), (vec!["EU4HARMONY_WEIGHT_400".to_string()], Some(0)));
        let mut saved = custom.clone();
        page.read_back(&window, &mut saved);
        assert_eq!(saved.body_font, None);

        for language in [Language::English, Language::TradChinese, Language::Japanese] {
            let settings = Settings { language: Some(language), ..Settings::default() };
            let (_, window) = opened_with(installed_with_bundled(), &settings);
            for id in ["body", "heading", "map", "console"] {
                let list = options(&window, id);
                assert!(!list.contains(&"EU4HARMONY_SETTINGS_FONT_DEFAULT".to_string()), "{language:?} {id}: {list:?}");
                for real in real_names {
                    assert!(!list.contains(&real.to_string()), "{language:?} {id} exposes {real}: {list:?}");
                }
            }
        }
    }
}
