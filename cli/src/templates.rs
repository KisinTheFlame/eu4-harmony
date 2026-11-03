//! Files `eu4-harmony new` writes: the template mod in `cli/template/` as it
//! is, named `template-mod` ("Template Mod", `template_mod`,
//! `TEMPLATE_MOD`), with the new mod's names in their place.

pub const DESCRIPTOR: &str = include_str!("../template/descriptor.mod");
pub const AGENTS: &str = include_str!("../template/AGENTS.template.md");
pub const MANIFEST: &str = include_str!("../template/eu4-harmony/manifest.toml");
pub const MAIN_LUA: &str = include_str!("../template/eu4-harmony/lua/main.lua");
pub const MAIN_TEST: &str = include_str!("../template/eu4-harmony/tests/main_test.lua");
pub const BUTTON_GUI: &str = include_str!("../template/eu4-harmony/ui/single_unitpanel/buttons/button.gui");
pub const LOC_ENGLISH: &str = include_str!("../template/eu4-harmony/localisation/template_mod_l_english.yml");
pub const LOC_SIMP_CHINESE: &str = include_str!("../template/eu4-harmony/localisation/template_mod_l_simp_chinese.yml");

/// The template mod's names, in the forms its files use them.
pub const ID: &str = "template-mod";
pub const NAME: &str = "Template Mod";
pub const LIB: &str = "template_mod";
pub const KEY: &str = "TEMPLATE_MOD";

/// `template` for the mod `id` named `name`: the template mod's names
/// replaced by its own.
pub fn fill(template: &str, id: &str, name: &str) -> String {
    let lib = id.replace('-', "_");
    template.replace(ID, id).replace(LIB, &lib).replace(KEY, &lib.to_ascii_uppercase()).replace(NAME, name)
}
