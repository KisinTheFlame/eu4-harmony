//! Subjects: the interactions the subjects view offers the player toward one
//! of the player's subjects, and carrying one out (docs/research/subjects.md).
//!
//! The game has a fixed set of interactions, each a static object of a class
//! of its own, whose number is the token of its key. One is carried out by the
//! game's `subject` command, which names the overlord, the subject and the
//! interaction; its check asks the interaction's own conditions, which write
//! their reasons as the view's tooltip shows them. The framework writes that
//! command, has the game read it, asks its checks, and posts it through the
//! command layer.
use super::{
    commands::{self, Refused},
    context, country, fail, game, Caller, Ctx, GResult, Status, Tag,
};
use crate::{game_string, memory::Memory, objects::campaign::country as country_fields, points::Point, unicode::game_text};
use eu4_harmony_remote::{choices::Choice, subjects::Interaction as Listed};

/// An interaction: its key, its number (the key's token), the static object
/// the game keeps it in and that object's class, and where the game's
/// static initializer makes the object: the instruction that loads the class,
/// and the one that stores it into the object, each with its bytes.
struct Interaction {
    key: &'static str,
    token: u32,
    object: usize,
    class: usize,
    class_at: (usize, &'static str),
    object_at: (usize, &'static str),
}

const INTERACTIONS: &[Interaction] = &[
    Interaction {
        key: "placate_rulers",
        token: 0x31d9,
        object: 0x1fd5238,
        class: 0x1ce6d80,
        class_at: (0x81ca2, "48 8d 05 d7 50 c6 01"),
        object_at: (0x81cb0, "48 89 05 81 35 f5 01"),
    },
    Interaction {
        key: "knowledge_sharing",
        token: 0x3776,
        object: 0x1fd5248,
        class: 0x1ce6f30,
        class_at: (0x81d52, "48 8d 05 d7 51 c6 01"),
        object_at: (0x81d60, "48 89 05 e1 34 f5 01"),
    },
    Interaction {
        key: "change_colonial_type",
        token: 0x3bb4,
        object: 0x1fd5258,
        class: 0x1ce6ea0,
        class_at: (0x81e02, "48 8d 05 97 50 c6 01"),
        object_at: (0x81e10, "48 89 05 41 34 f5 01"),
    },
    Interaction {
        key: "upgrade_subject_type",
        token: 0x3bb9,
        object: 0x1fd5268,
        class: 0x1ce6ee8,
        class_at: (0x81eb2, "48 8d 05 2f 50 c6 01"),
        object_at: (0x81ec0, "48 89 05 a1 33 f5 01"),
    },
    Interaction {
        key: "embargo_rivals",
        token: 0x31da,
        object: 0x1fd5278,
        class: 0x1ce6e58,
        class_at: (0x81f62, "48 8d 05 ef 4e c6 01"),
        object_at: (0x81f70, "48 89 05 01 33 f5 01"),
    },
    Interaction {
        key: "place_relative_on_throne",
        token: 0x31dd,
        object: 0x1fd5288,
        class: 0x1ce6dc8,
        class_at: (0x82012, "48 8d 05 af 4d c6 01"),
        object_at: (0x82020, "48 89 05 61 32 f5 01"),
    },
    Interaction {
        key: "enforce_religion",
        token: 0x31de,
        object: 0x1fd5298,
        class: 0x1ce6e10,
        class_at: (0x820c2, "48 8d 05 47 4d c6 01"),
        object_at: (0x820d0, "48 89 05 c1 31 f5 01"),
    },
    Interaction {
        key: "scutage",
        token: 0x31df,
        object: 0x1fd52a8,
        class: 0x1ce6af8,
        class_at: (0x82172, "48 8d 05 7f 49 c6 01"),
        object_at: (0x82180, "48 89 05 21 31 f5 01"),
    },
    Interaction {
        key: "subsidize_armies",
        token: 0x31e0,
        object: 0x1fd52b8,
        class: 0x1ce6b40,
        class_at: (0x82222, "48 8d 05 17 49 c6 01"),
        object_at: (0x82230, "48 89 05 81 30 f5 01"),
    },
    Interaction {
        key: "enforce_culture",
        token: 0x3220,
        object: 0x1fd52c8,
        class: 0x1ce6a68,
        class_at: (0x822d2, "48 8d 05 8f 47 c6 01"),
        object_at: (0x822e0, "48 89 05 e1 2f f5 01"),
    },
    Interaction {
        key: "siphon_income",
        token: 0x3221,
        object: 0x1fd52d8,
        class: 0x1ce6ab0,
        class_at: (0x82382, "48 8d 05 27 47 c6 01"),
        object_at: (0x82390, "48 89 05 41 2f f5 01"),
    },
    Interaction {
        key: "seize_court_resources",
        token: 0x3d87,
        object: 0x1fd52e8,
        class: 0x1ce6a20,
        class_at: (0x82432, "48 8d 05 e7 45 c6 01"),
        object_at: (0x82440, "48 89 05 a1 2e f5 01"),
    },
    Interaction {
        key: "request_extra_levies",
        token: 0x3d88,
        object: 0x1fd52f8,
        class: 0x1ce6990,
        class_at: (0x824e2, "48 8d 05 a7 44 c6 01"),
        object_at: (0x824f0, "48 89 05 01 2e f5 01"),
    },
    Interaction {
        key: "grant_administrative_autonomy",
        token: 0x3d89,
        object: 0x1fd5308,
        class: 0x1ce69d8,
        class_at: (0x82592, "48 8d 05 3f 44 c6 01"),
        object_at: (0x825a0, "48 89 05 61 2d f5 01"),
    },
    Interaction {
        key: "support_loyalists",
        token: 0x3222,
        object: 0x1fd5318,
        class: 0x1ce6ca8,
        class_at: (0x82642, "48 8d 05 5f 46 c6 01"),
        object_at: (0x82650, "48 89 05 c1 2c f5 01"),
    },
    Interaction {
        key: "send_officers",
        token: 0x3223,
        object: 0x1fd5328,
        class: 0x1ce6cf0,
        class_at: (0x826f2, "48 8d 05 f7 45 c6 01"),
        object_at: (0x82700, "48 89 05 21 2c f5 01"),
    },
    Interaction {
        key: "divert_trade",
        token: 0x3224,
        object: 0x1fd5338,
        class: 0x1ce6c60,
        class_at: (0x827a2, "48 8d 05 b7 44 c6 01"),
        object_at: (0x827b0, "48 89 05 81 2b f5 01"),
    },
    Interaction {
        key: "fortify_march",
        token: 0x31e1,
        object: 0x1fd5348,
        class: 0x1ce6bd0,
        class_at: (0x82852, "48 8d 05 77 43 c6 01"),
        object_at: (0x82860, "48 89 05 e1 2a f5 01"),
    },
    Interaction {
        key: "seize_territory",
        token: 0x3225,
        object: 0x1fd5358,
        class: 0x1ce6c18,
        class_at: (0x82902, "48 8d 05 0f 43 c6 01"),
        object_at: (0x82910, "48 89 05 41 2a f5 01"),
    },
    Interaction {
        key: "start_colonial_war",
        token: 0x3226,
        object: 0x1fd5368,
        class: 0x1ce6b88,
        class_at: (0x829b2, "48 8d 05 cf 41 c6 01"),
        object_at: (0x829c0, "48 89 05 a1 29 f5 01"),
    },
    Interaction {
        key: "replace_governor",
        token: 0x3227,
        object: 0x1fd5378,
        class: 0x1ce6618,
        class_at: (0x82a62, "48 8d 05 af 3b c6 01"),
        object_at: (0x82a70, "48 89 05 01 29 f5 01"),
    },
    Interaction {
        key: "grant_province",
        token: 0x324a,
        object: 0x1fd54b8,
        class: 0x1ce6588,
        class_at: (0x82b12, "48 8d 05 6f 3a c6 01"),
        object_at: (0x82b20, "48 89 05 91 29 f5 01"),
    },
    Interaction {
        key: "grant_core_claim",
        token: 0x324b,
        object: 0x1fd54a8,
        class: 0x1ce65d0,
        class_at: (0x82bc2, "48 8d 05 07 3a c6 01"),
        object_at: (0x82bd0, "48 89 05 d1 28 f5 01"),
    },
    Interaction {
        key: "sacrifice_ruler",
        token: 0x3304,
        object: 0x1fd5498,
        class: 0x1ce64f8,
        class_at: (0x82c72, "48 8d 05 7f 38 c6 01"),
        object_at: (0x82c80, "48 89 05 11 28 f5 01"),
    },
    Interaction {
        key: "sacrifice_heir",
        token: 0x3305,
        object: 0x1fd5488,
        class: 0x1ce6540,
        class_at: (0x82d22, "48 8d 05 17 38 c6 01"),
        object_at: (0x82d30, "48 89 05 51 27 f5 01"),
    },
    Interaction {
        key: "increase_tariffs",
        token: 0x324d,
        object: 0x1fd5388,
        class: 0x1ce6468,
        class_at: (0x82dd2, "48 8d 05 8f 36 c6 01"),
        object_at: (0x82de0, "48 89 05 a1 25 f5 01"),
    },
    Interaction {
        key: "decrease_tariffs",
        token: 0x324e,
        object: 0x1fd5398,
        class: 0x1ce64b0,
        class_at: (0x82e82, "48 8d 05 27 36 c6 01"),
        object_at: (0x82e90, "48 89 05 01 25 f5 01"),
    },
    Interaction {
        key: "customize_subject",
        token: 0x3306,
        object: 0x1fd53a8,
        class: 0x1ce6900,
        class_at: (0x82f32, "48 8d 05 c7 39 c6 01"),
        object_at: (0x82f40, "48 89 05 61 24 f5 01"),
    },
    Interaction {
        key: "takeondebt",
        token: 0x3469,
        object: 0x1fd53b8,
        class: 0x1ce6948,
        class_at: (0x82fe2, "48 8d 05 5f 39 c6 01"),
        object_at: (0x82ff0, "48 89 05 c1 23 f5 01"),
    },
    Interaction {
        key: "bestow_gifts",
        token: 0x35cc,
        object: 0x1fd53c8,
        class: 0x1ce6870,
        class_at: (0x83092, "48 8d 05 d7 37 c6 01"),
        object_at: (0x830a0, "48 89 05 21 23 f5 01"),
    },
    Interaction {
        key: "send_additional_troops",
        token: 0x35cd,
        object: 0x1fd53d8,
        class: 0x1ce68b8,
        class_at: (0x83142, "48 8d 05 6f 37 c6 01"),
        object_at: (0x83150, "48 89 05 81 22 f5 01"),
    },
    Interaction {
        key: "demand_artifacts",
        token: 0x35ce,
        object: 0x1fd53e8,
        class: 0x1ce6798,
        class_at: (0x831f2, "48 8d 05 9f 35 c6 01"),
        object_at: (0x83200, "48 89 05 e1 21 f5 01"),
    },
    Interaction {
        key: "demand_additional_tribute",
        token: 0x35cf,
        object: 0x1fd53f8,
        class: 0x1ce67e0,
        class_at: (0x832a2, "48 8d 05 37 35 c6 01"),
        object_at: (0x832b0, "48 89 05 41 21 f5 01"),
    },
    Interaction {
        key: "force_seppuku",
        token: 0x35f9,
        object: 0x1fd5408,
        class: 0x1ce6828,
        class_at: (0x83352, "48 8d 05 cf 34 c6 01"),
        object_at: (0x83360, "48 89 05 a1 20 f5 01"),
    },
    Interaction {
        key: "return_land",
        token: 0x35fa,
        object: 0x1fd5418,
        class: 0x1ce6708,
        class_at: (0x83402, "48 8d 05 ff 32 c6 01"),
        object_at: (0x83410, "48 89 05 01 20 f5 01"),
    },
    Interaction {
        key: "contribute_to_capital",
        token: 0x35fb,
        object: 0x1fd5428,
        class: 0x1ce6750,
        class_at: (0x834b2, "48 8d 05 97 32 c6 01"),
        object_at: (0x834c0, "48 89 05 61 1f f5 01"),
    },
    Interaction {
        key: "conscript_general",
        token: 0x35fc,
        object: 0x1fd5438,
        class: 0x1ce63d8,
        class_at: (0x83562, "48 8d 05 6f 2e c6 01"),
        object_at: (0x83570, "48 89 05 c1 1e f5 01"),
    },
    Interaction {
        key: "force_isolation",
        token: 0x35fd,
        object: 0x1fd5448,
        class: 0x1ce6420,
        class_at: (0x83612, "48 8d 05 07 2e c6 01"),
        object_at: (0x83620, "48 89 05 21 1e f5 01"),
    },
    Interaction {
        key: "press_sailors",
        token: 0x33b4,
        object: 0x1fd5458,
        class: 0x1ce6390,
        class_at: (0x836c2, "48 8d 05 c7 2c c6 01"),
        object_at: (0x836d0, "48 89 05 81 1d f5 01"),
    },
    Interaction {
        key: "allow_settlement_growth",
        token: 0x3a94,
        object: 0x1fd5468,
        class: 0x1ce6348,
        class_at: (0x83772, "48 8d 05 cf 2b c6 01"),
        object_at: (0x83780, "48 89 05 e1 1c f5 01"),
    },
    Interaction {
        key: "block_settlement_growth",
        token: 0x3a93,
        object: 0x1fd5478,
        class: 0x1ce6300,
        class_at: (0x83822, "48 8d 05 d7 2a c6 01"),
        object_at: (0x83830, "48 89 05 41 1c f5 01"),
    },
];

/// An interaction's methods, by vtable offset: its name, (interaction, out
/// `std::string`), localized; its own conditions, (interaction, command,
/// reasons), with the reasons appended to a `std::string` or null; whether the
/// overlord may use it at all, (interaction, overlord's tag); and the bit of
/// a subject's interactions going on that is its, (interaction), 0 for one
/// that does not go on.
const NAME: usize = 0x10;
const CONDITIONS: usize = 0x28;
const FOR_OVERLORD: usize = 0x30;
const ONGOING: usize = 0x38;
/// The `subject` command's check of an interaction, (command, reasons): the
/// subject is the overlord's, its type allows the interaction, the command
/// stops it only while it goes on, then the interaction's own conditions.
const CHECK: usize = 0xaf3e70;
/// A subject's type, (subject country), and whether a type allows an
/// interaction, (the type's interactions, interaction).
const SUBJECT_TYPE: usize = 0x3d79f0;
const TYPE_ALLOWS: usize = 0xb22910;
/// A country's overlord, a tag, its valid byte the tag's last; the subject's
/// interactions going on, a bit set of u32 words.
const OVERLORD_VALID: usize = 0x18f7;
const ONGOING_BITS: usize = 0x78;

pub fn points() -> Vec<Point> {
    let mut points: Vec<Point> = INTERACTIONS
        .iter()
        .flat_map(|i| [Point::keep(i.class_at.0, i.class_at.1, "subject interaction class"), Point::keep(i.object_at.0, i.object_at.1, "subject interaction")])
        .collect();
    points.extend([
        Point::keep(CHECK, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 4c 8b 99 98 00 00 00", "subject interaction check"),
        // The check: the subject's type allows it, then the bit of the
        // interactions going on, then the interaction's own conditions.
        Point::keep(0xaf3ee6usize, "48 8b cf e8 02 3b 8e ff 48 85 c0 74 6d 49 8b d3 48 8b c8 e8 12 ea 02 00", "subject interaction type"),
        Point::keep(0xaf3f02usize, "49 8b 03 49 8b cb ff 50 38 85 c0 74 2e 80 bf f7 18 00 00 00", "subject interaction going on")
            .uses(&[ONGOING, OVERLORD_VALID]),
        Point::keep(0xaf3f2busize, "8b 44 97 78", "subject interactions going on").uses(&[ONGOING_BITS]),
        Point::keep(0xaf3f4ausize, "48 8b 01", "subject interaction conditions"),
        Point::keep(0xaf3f5cusize, "48 ff 60 28", "subject interaction conditions call").uses(&[CONDITIONS]),
        Point::keep(0xaf3e35usize, "48 8b 01 48 8b 53 50 ff 50 30", "subject interaction for the overlord").uses(&[FOR_OVERLORD]),
        Point::keep(SUBJECT_TYPE, "80 b9 f7 18 00 00 00 74 4c 48 8b 81 f0 18 00 00", "subject's type").uses(&[OVERLORD_VALID, country_fields::OVERLORD]),
        Point::keep(TYPE_ALLOWS, "44 0f b6 42 08 4c 8b c9", "subject type allows"),
        // The interaction's name, as its class's own method writes it.
        Point::keep(0xaf9dc0usize, "40 53 48 83 ec 20 48 8b da 45 33 c9 48 8b cb 48 8d", "subject interaction name"),
        Point::method(0x1ce6dc8, NAME, 0xaf9dc0, "subject interaction name"),
        // The command writes the interaction's number, the object's `+8`.
        Point::keep(0xaf3c58usize, "48 8b 87 98 00 00 00 48 85 c0 74 11 44 8b 40 08 ba db 31 00 00", "subject command interaction"),
    ]);
    points
}

/// The interactions the game has for the player toward `subject`: those
/// the player's country may use at all and the subject's type allows.
pub fn options(_: Caller, subject: Tag) -> GResult<Vec<Listed>> {
    let m = game()?;
    let ctx = context(m)?;
    let country = subject_of(m, &ctx, subject)?;
    let mut listed = Vec::new();
    for i in INTERACTIONS {
        let object = object(m, i)?;
        if !offered(m, &ctx, country, object)? {
            continue;
        }
        let active = ongoing(m, country, object)?;
        let text = command(&ctx, subject, i, active, None);
        let (can, reasons) = judge(&text)?;
        let name = game_text::plain(&game_string::written(m, |s| m.call(m.vslot(object, NAME)?, &[object, s]))?.1).trim().to_string();
        let choice = if can { Choice::can(Vec::new()) } else { Choice::cannot(reasons, Vec::new()) };
        listed.push(Listed { interaction: i.key.into(), name, active, choice });
    }
    Ok(listed)
}

/// Carries out `interaction` with `subject`, or stops it while it goes on, as
/// the subjects view's button does: [`Status::InvalidArgument`] for a key the
/// game has no interaction for; [`Status::Rejected`] when the player's
/// country may not use it, the subject's type does not allow it, or its
/// conditions do not hold, with the reasons.
pub fn interact(_: Caller, subject: Tag, interaction: &str, third_party: Option<Tag>) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let country = subject_of(m, &ctx, subject)?;
    let Some(i) = INTERACTIONS.iter().find(|i| i.key == interaction) else {
        return fail(Status::InvalidArgument, format!("the game has no subject interaction {interaction}; subject.options lists them"));
    };
    if let Some(third) = third_party {
        super::country(m, &ctx, third)?;
    }
    let object = object(m, i)?;
    if !offered(m, &ctx, country, object)? {
        return fail(Status::Rejected, format!("the game does not offer {interaction} toward {}", letters(subject)));
    }
    let text = command(&ctx, subject, i, ongoing(m, country, object)?, third_party);
    let (can, reasons) = judge(&text)?;
    if !can {
        return fail(Status::Rejected, if reasons.is_empty() { "the game does not allow it, and gives no reason".into() } else { reasons.join("\n") });
    }
    commands::post(&text).map_err(Into::into)
}

/// The country of `subject`, one of the player's subjects.
fn subject_of(m: &Memory, ctx: &Ctx, subject: Tag) -> GResult<usize> {
    let at = country(m, ctx, subject)?;
    let overlord = Tag(m.u64(at + country_fields::OVERLORD)?);
    if m.u8(at + OVERLORD_VALID)? == 0 || overlord.index() != ctx.player.index() {
        return fail(Status::Rejected, format!("{} is not a subject of the player's", letters(subject)));
    }
    Ok(at)
}

/// The interaction's object, checked by its class.
fn object(m: &Memory, i: &Interaction) -> GResult<usize> {
    let object = m.at(i.object);
    if m.ptr(object)? != m.at(i.class) || m.u32(object + 8)? != i.token {
        return Err(format!("the subject interaction {} is not where it was", i.key).into());
    }
    Ok(object)
}

/// Whether the view offers the interaction: the player's country may use it
/// and the subject's type allows it.
fn offered(m: &Memory, ctx: &Ctx, subject: usize, object: usize) -> GResult<bool> {
    if m.call(m.vslot(object, FOR_OVERLORD)?, &[object, ctx.player.0 as usize])? as u8 == 0 {
        return Ok(false);
    }
    let kind = m.call(m.at(SUBJECT_TYPE), &[subject])?;
    Ok(kind != 0 && m.call(m.at(TYPE_ALLOWS), &[kind, object])? as u8 != 0)
}

/// Whether the interaction goes on with the subject, as the check reads it.
fn ongoing(m: &Memory, subject: usize, object: usize) -> GResult<bool> {
    let bit = m.call(m.vslot(object, ONGOING)?, &[object])? as i32;
    if bit <= 0 {
        return Ok(false);
    }
    let word = m.u32(subject + ONGOING_BITS + (bit as usize >> 5) * 4)?;
    Ok(word >> (bit & 31) & 1 == 1)
}

/// The `subject` command that carries out the interaction, or stops it.
fn command(ctx: &Ctx, subject: Tag, i: &Interaction, stop: bool, third_party: Option<Tag>) -> String {
    let third = third_party.map(|t| format!(" third_party=\"{}\"", letters(t))).unwrap_or_default();
    let cancel = if stop { " cancel=yes" } else { "" };
    format!("subject={{ overlord=\"{}\" subject=\"{}\"{third}{cancel} interaction={} }}", letters(ctx.player), letters(subject), i.token)
}

/// Whether the command's check of the interaction lets the player, and its
/// reasons when not, as the view's tooltip gives them.
fn judge(text: &str) -> GResult<(bool, Vec<String>)> {
    let judged = commands::with_command(text, |m, command| {
        if m.call(m.at(CHECK), &[command, 0])? as u8 != 0 {
            return Ok((true, Vec::new()));
        }
        let said = game_string::written(m, |s| m.call(m.at(CHECK), &[command, s]))?.1;
        Ok((false, super::diplomacy::reasons_said(&said)))
    });
    judged.map_err(|refused| match refused {
        Refused::Game(fail) => fail,
        other => other.into(),
    })
}

fn letters(tag: Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    #[test]
    fn the_command_names_both_countries_and_the_interaction_s_number() {
        let ctx = Ctx { world: 0, date: 0, player: game_tag(b"TUR", 1), country: 0, countries: 0 };
        let scutage = INTERACTIONS.iter().find(|i| i.key == "scutage").unwrap();
        assert_eq!(command(&ctx, game_tag(b"SER", 2), scutage, false, None), "subject={ overlord=\"TUR\" subject=\"SER\" interaction=12767 }");
        assert_eq!(
            command(&ctx, game_tag(b"SER", 2), scutage, true, Some(game_tag(b"BYZ", 3))),
            "subject={ overlord=\"TUR\" subject=\"SER\" third_party=\"BYZ\" cancel=yes interaction=12767 }"
        );
    }

    #[test]
    fn each_interaction_has_its_own_key_object_and_class() {
        for (n, i) in INTERACTIONS.iter().enumerate() {
            assert!(INTERACTIONS[..n].iter().all(|o| o.key != i.key && o.object != i.object && o.class != i.class), "{}", i.key);
        }
    }
}
