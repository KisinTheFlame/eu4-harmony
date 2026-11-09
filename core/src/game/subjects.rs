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
use super::{action, commands, countries, country, fail, with_state, Ctx, GResult, Session, Status, Tag};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{country as country_fields, functions, tag},
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::{
    choices::Choice,
    subjects::{args, SubjectInteraction as Listed},
    ProvinceId,
};

/// An interaction: its key, its number (the key's token), the static object
/// the game keeps it in and that object's class; [`points`] keeps where the
/// game's static initializer makes the object.
struct Interaction {
    key: &'static str,
    token: u32,
    object: usize,
    class: usize,
}

const INTERACTIONS: &[Interaction] = &[
    Interaction { key: "placate_rulers", token: 0x31d9, object: 0x1fd5238, class: 0x1ce6d80 },
    Interaction { key: "knowledge_sharing", token: 0x3776, object: 0x1fd5248, class: 0x1ce6f30 },
    Interaction { key: "change_colonial_type", token: 0x3bb4, object: 0x1fd5258, class: 0x1ce6ea0 },
    Interaction { key: "upgrade_subject_type", token: 0x3bb9, object: 0x1fd5268, class: 0x1ce6ee8 },
    Interaction { key: "embargo_rivals", token: 0x31da, object: 0x1fd5278, class: 0x1ce6e58 },
    Interaction { key: "place_relative_on_throne", token: 0x31dd, object: 0x1fd5288, class: 0x1ce6dc8 },
    Interaction { key: "enforce_religion", token: 0x31de, object: 0x1fd5298, class: 0x1ce6e10 },
    Interaction { key: "scutage", token: 0x31df, object: 0x1fd52a8, class: 0x1ce6af8 },
    Interaction { key: "subsidize_armies", token: 0x31e0, object: 0x1fd52b8, class: 0x1ce6b40 },
    Interaction { key: "enforce_culture", token: 0x3220, object: 0x1fd52c8, class: 0x1ce6a68 },
    Interaction { key: "siphon_income", token: 0x3221, object: 0x1fd52d8, class: 0x1ce6ab0 },
    Interaction { key: "seize_court_resources", token: 0x3d87, object: 0x1fd52e8, class: 0x1ce6a20 },
    Interaction { key: "request_extra_levies", token: 0x3d88, object: 0x1fd52f8, class: 0x1ce6990 },
    Interaction { key: "grant_administrative_autonomy", token: 0x3d89, object: 0x1fd5308, class: 0x1ce69d8 },
    Interaction { key: "support_loyalists", token: 0x3222, object: 0x1fd5318, class: 0x1ce6ca8 },
    Interaction { key: "send_officers", token: 0x3223, object: 0x1fd5328, class: 0x1ce6cf0 },
    Interaction { key: "divert_trade", token: 0x3224, object: 0x1fd5338, class: 0x1ce6c60 },
    Interaction { key: "fortify_march", token: 0x31e1, object: 0x1fd5348, class: 0x1ce6bd0 },
    Interaction { key: "seize_territory", token: 0x3225, object: 0x1fd5358, class: 0x1ce6c18 },
    Interaction { key: "start_colonial_war", token: 0x3226, object: 0x1fd5368, class: 0x1ce6b88 },
    Interaction { key: "replace_governor", token: 0x3227, object: 0x1fd5378, class: 0x1ce6618 },
    Interaction { key: "grant_province", token: 0x324a, object: 0x1fd54b8, class: 0x1ce6588 },
    Interaction { key: "grant_core_claim", token: 0x324b, object: 0x1fd54a8, class: 0x1ce65d0 },
    Interaction { key: "sacrifice_ruler", token: 0x3304, object: 0x1fd5498, class: 0x1ce64f8 },
    Interaction { key: "sacrifice_heir", token: 0x3305, object: 0x1fd5488, class: 0x1ce6540 },
    Interaction { key: "increase_tariffs", token: 0x324d, object: 0x1fd5388, class: 0x1ce6468 },
    Interaction { key: "decrease_tariffs", token: 0x324e, object: 0x1fd5398, class: 0x1ce64b0 },
    Interaction { key: "customize_subject", token: 0x3306, object: 0x1fd53a8, class: 0x1ce6900 },
    Interaction { key: "takeondebt", token: 0x3469, object: 0x1fd53b8, class: 0x1ce6948 },
    Interaction { key: "bestow_gifts", token: 0x35cc, object: 0x1fd53c8, class: 0x1ce6870 },
    Interaction { key: "send_additional_troops", token: 0x35cd, object: 0x1fd53d8, class: 0x1ce68b8 },
    Interaction { key: "demand_artifacts", token: 0x35ce, object: 0x1fd53e8, class: 0x1ce6798 },
    Interaction { key: "demand_additional_tribute", token: 0x35cf, object: 0x1fd53f8, class: 0x1ce67e0 },
    Interaction { key: "force_seppuku", token: 0x35f9, object: 0x1fd5408, class: 0x1ce6828 },
    Interaction { key: "return_land", token: 0x35fa, object: 0x1fd5418, class: 0x1ce6708 },
    Interaction { key: "contribute_to_capital", token: 0x35fb, object: 0x1fd5428, class: 0x1ce6750 },
    Interaction { key: "conscript_general", token: 0x35fc, object: 0x1fd5438, class: 0x1ce63d8 },
    Interaction { key: "force_isolation", token: 0x35fd, object: 0x1fd5448, class: 0x1ce6420 },
    Interaction { key: "press_sailors", token: 0x33b4, object: 0x1fd5458, class: 0x1ce6390 },
    Interaction { key: "allow_settlement_growth", token: 0x3a94, object: 0x1fd5468, class: 0x1ce6348 },
    Interaction { key: "block_settlement_growth", token: 0x3a93, object: 0x1fd5478, class: 0x1ce6300 },
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
const CHECK: GameFunction<2> = GameFunction::at(0xaf3e70);
/// Whether a subject type allows an interaction, (the type's interactions,
/// interaction); a subject's type is the game's own getter's
/// ([`functions::SUBJECT_TYPE`]).
const TYPE_ALLOWS: GameFunction<2> = GameFunction::at(0xb22910);
/// A country's overlord's valid byte, the tag's last; the subject's
/// interactions going on, a bit set of u32 words.
const OVERLORD_VALID: usize = country_fields::OVERLORD + tag::VALID / 8;
const ONGOING_BITS: usize = 0x78;

pub fn points() -> Vec<Point> {
    // Each interaction's class, and its object, as its initializer makes it.
    let mut points = vec![
        Point::keep(0x81ca2usize, "48 8d 05 d7 50 c6 01", "subject interaction placate_rulers").uses(&[0x1ce6d80]),
        Point::keep(0x81cb0usize, "48 89 05 81 35 f5 01", "subject interaction placate_rulers").uses(&[0x1fd5238]),
        Point::keep(0x81d52usize, "48 8d 05 d7 51 c6 01", "subject interaction knowledge_sharing").uses(&[0x1ce6f30]),
        Point::keep(0x81d60usize, "48 89 05 e1 34 f5 01", "subject interaction knowledge_sharing").uses(&[0x1fd5248]),
        Point::keep(0x81e02usize, "48 8d 05 97 50 c6 01", "subject interaction change_colonial_type").uses(&[0x1ce6ea0]),
        Point::keep(0x81e10usize, "48 89 05 41 34 f5 01", "subject interaction change_colonial_type").uses(&[0x1fd5258]),
        Point::keep(0x81eb2usize, "48 8d 05 2f 50 c6 01", "subject interaction upgrade_subject_type").uses(&[0x1ce6ee8]),
        Point::keep(0x81ec0usize, "48 89 05 a1 33 f5 01", "subject interaction upgrade_subject_type").uses(&[0x1fd5268]),
        Point::keep(0x81f62usize, "48 8d 05 ef 4e c6 01", "subject interaction embargo_rivals").uses(&[0x1ce6e58]),
        Point::keep(0x81f70usize, "48 89 05 01 33 f5 01", "subject interaction embargo_rivals").uses(&[0x1fd5278]),
        Point::keep(0x82012usize, "48 8d 05 af 4d c6 01", "subject interaction place_relative_on_throne").uses(&[0x1ce6dc8]),
        Point::keep(0x82020usize, "48 89 05 61 32 f5 01", "subject interaction place_relative_on_throne").uses(&[0x1fd5288]),
        Point::keep(0x820c2usize, "48 8d 05 47 4d c6 01", "subject interaction enforce_religion").uses(&[0x1ce6e10]),
        Point::keep(0x820d0usize, "48 89 05 c1 31 f5 01", "subject interaction enforce_religion").uses(&[0x1fd5298]),
        Point::keep(0x82172usize, "48 8d 05 7f 49 c6 01", "subject interaction scutage").uses(&[0x1ce6af8]),
        Point::keep(0x82180usize, "48 89 05 21 31 f5 01", "subject interaction scutage").uses(&[0x1fd52a8]),
        Point::keep(0x82222usize, "48 8d 05 17 49 c6 01", "subject interaction subsidize_armies").uses(&[0x1ce6b40]),
        Point::keep(0x82230usize, "48 89 05 81 30 f5 01", "subject interaction subsidize_armies").uses(&[0x1fd52b8]),
        Point::keep(0x822d2usize, "48 8d 05 8f 47 c6 01", "subject interaction enforce_culture").uses(&[0x1ce6a68]),
        Point::keep(0x822e0usize, "48 89 05 e1 2f f5 01", "subject interaction enforce_culture").uses(&[0x1fd52c8]),
        Point::keep(0x82382usize, "48 8d 05 27 47 c6 01", "subject interaction siphon_income").uses(&[0x1ce6ab0]),
        Point::keep(0x82390usize, "48 89 05 41 2f f5 01", "subject interaction siphon_income").uses(&[0x1fd52d8]),
        Point::keep(0x82432usize, "48 8d 05 e7 45 c6 01", "subject interaction seize_court_resources").uses(&[0x1ce6a20]),
        Point::keep(0x82440usize, "48 89 05 a1 2e f5 01", "subject interaction seize_court_resources").uses(&[0x1fd52e8]),
        Point::keep(0x824e2usize, "48 8d 05 a7 44 c6 01", "subject interaction request_extra_levies").uses(&[0x1ce6990]),
        Point::keep(0x824f0usize, "48 89 05 01 2e f5 01", "subject interaction request_extra_levies").uses(&[0x1fd52f8]),
        Point::keep(0x82592usize, "48 8d 05 3f 44 c6 01", "subject interaction grant_administrative_autonomy").uses(&[0x1ce69d8]),
        Point::keep(0x825a0usize, "48 89 05 61 2d f5 01", "subject interaction grant_administrative_autonomy").uses(&[0x1fd5308]),
        Point::keep(0x82642usize, "48 8d 05 5f 46 c6 01", "subject interaction support_loyalists").uses(&[0x1ce6ca8]),
        Point::keep(0x82650usize, "48 89 05 c1 2c f5 01", "subject interaction support_loyalists").uses(&[0x1fd5318]),
        Point::keep(0x826f2usize, "48 8d 05 f7 45 c6 01", "subject interaction send_officers").uses(&[0x1ce6cf0]),
        Point::keep(0x82700usize, "48 89 05 21 2c f5 01", "subject interaction send_officers").uses(&[0x1fd5328]),
        Point::keep(0x827a2usize, "48 8d 05 b7 44 c6 01", "subject interaction divert_trade").uses(&[0x1ce6c60]),
        Point::keep(0x827b0usize, "48 89 05 81 2b f5 01", "subject interaction divert_trade").uses(&[0x1fd5338]),
        Point::keep(0x82852usize, "48 8d 05 77 43 c6 01", "subject interaction fortify_march").uses(&[0x1ce6bd0]),
        Point::keep(0x82860usize, "48 89 05 e1 2a f5 01", "subject interaction fortify_march").uses(&[0x1fd5348]),
        Point::keep(0x82902usize, "48 8d 05 0f 43 c6 01", "subject interaction seize_territory").uses(&[0x1ce6c18]),
        Point::keep(0x82910usize, "48 89 05 41 2a f5 01", "subject interaction seize_territory").uses(&[0x1fd5358]),
        Point::keep(0x829b2usize, "48 8d 05 cf 41 c6 01", "subject interaction start_colonial_war").uses(&[0x1ce6b88]),
        Point::keep(0x829c0usize, "48 89 05 a1 29 f5 01", "subject interaction start_colonial_war").uses(&[0x1fd5368]),
        Point::keep(0x82a62usize, "48 8d 05 af 3b c6 01", "subject interaction replace_governor").uses(&[0x1ce6618]),
        Point::keep(0x82a70usize, "48 89 05 01 29 f5 01", "subject interaction replace_governor").uses(&[0x1fd5378]),
        Point::keep(0x82b12usize, "48 8d 05 6f 3a c6 01", "subject interaction grant_province").uses(&[0x1ce6588]),
        Point::keep(0x82b20usize, "48 89 05 91 29 f5 01", "subject interaction grant_province").uses(&[0x1fd54b8]),
        Point::keep(0x82bc2usize, "48 8d 05 07 3a c6 01", "subject interaction grant_core_claim").uses(&[0x1ce65d0]),
        Point::keep(0x82bd0usize, "48 89 05 d1 28 f5 01", "subject interaction grant_core_claim").uses(&[0x1fd54a8]),
        Point::keep(0x82c72usize, "48 8d 05 7f 38 c6 01", "subject interaction sacrifice_ruler").uses(&[0x1ce64f8]),
        Point::keep(0x82c80usize, "48 89 05 11 28 f5 01", "subject interaction sacrifice_ruler").uses(&[0x1fd5498]),
        Point::keep(0x82d22usize, "48 8d 05 17 38 c6 01", "subject interaction sacrifice_heir").uses(&[0x1ce6540]),
        Point::keep(0x82d30usize, "48 89 05 51 27 f5 01", "subject interaction sacrifice_heir").uses(&[0x1fd5488]),
        Point::keep(0x82dd2usize, "48 8d 05 8f 36 c6 01", "subject interaction increase_tariffs").uses(&[0x1ce6468]),
        Point::keep(0x82de0usize, "48 89 05 a1 25 f5 01", "subject interaction increase_tariffs").uses(&[0x1fd5388]),
        Point::keep(0x82e82usize, "48 8d 05 27 36 c6 01", "subject interaction decrease_tariffs").uses(&[0x1ce64b0]),
        Point::keep(0x82e90usize, "48 89 05 01 25 f5 01", "subject interaction decrease_tariffs").uses(&[0x1fd5398]),
        Point::keep(0x82f32usize, "48 8d 05 c7 39 c6 01", "subject interaction customize_subject").uses(&[0x1ce6900]),
        Point::keep(0x82f40usize, "48 89 05 61 24 f5 01", "subject interaction customize_subject").uses(&[0x1fd53a8]),
        Point::keep(0x82fe2usize, "48 8d 05 5f 39 c6 01", "subject interaction takeondebt").uses(&[0x1ce6948]),
        Point::keep(0x82ff0usize, "48 89 05 c1 23 f5 01", "subject interaction takeondebt").uses(&[0x1fd53b8]),
        Point::keep(0x83092usize, "48 8d 05 d7 37 c6 01", "subject interaction bestow_gifts").uses(&[0x1ce6870]),
        Point::keep(0x830a0usize, "48 89 05 21 23 f5 01", "subject interaction bestow_gifts").uses(&[0x1fd53c8]),
        Point::keep(0x83142usize, "48 8d 05 6f 37 c6 01", "subject interaction send_additional_troops").uses(&[0x1ce68b8]),
        Point::keep(0x83150usize, "48 89 05 81 22 f5 01", "subject interaction send_additional_troops").uses(&[0x1fd53d8]),
        Point::keep(0x831f2usize, "48 8d 05 9f 35 c6 01", "subject interaction demand_artifacts").uses(&[0x1ce6798]),
        Point::keep(0x83200usize, "48 89 05 e1 21 f5 01", "subject interaction demand_artifacts").uses(&[0x1fd53e8]),
        Point::keep(0x832a2usize, "48 8d 05 37 35 c6 01", "subject interaction demand_additional_tribute").uses(&[0x1ce67e0]),
        Point::keep(0x832b0usize, "48 89 05 41 21 f5 01", "subject interaction demand_additional_tribute").uses(&[0x1fd53f8]),
        Point::keep(0x83352usize, "48 8d 05 cf 34 c6 01", "subject interaction force_seppuku").uses(&[0x1ce6828]),
        Point::keep(0x83360usize, "48 89 05 a1 20 f5 01", "subject interaction force_seppuku").uses(&[0x1fd5408]),
        Point::keep(0x83402usize, "48 8d 05 ff 32 c6 01", "subject interaction return_land").uses(&[0x1ce6708]),
        Point::keep(0x83410usize, "48 89 05 01 20 f5 01", "subject interaction return_land").uses(&[0x1fd5418]),
        Point::keep(0x834b2usize, "48 8d 05 97 32 c6 01", "subject interaction contribute_to_capital").uses(&[0x1ce6750]),
        Point::keep(0x834c0usize, "48 89 05 61 1f f5 01", "subject interaction contribute_to_capital").uses(&[0x1fd5428]),
        Point::keep(0x83562usize, "48 8d 05 6f 2e c6 01", "subject interaction conscript_general").uses(&[0x1ce63d8]),
        Point::keep(0x83570usize, "48 89 05 c1 1e f5 01", "subject interaction conscript_general").uses(&[0x1fd5438]),
        Point::keep(0x83612usize, "48 8d 05 07 2e c6 01", "subject interaction force_isolation").uses(&[0x1ce6420]),
        Point::keep(0x83620usize, "48 89 05 21 1e f5 01", "subject interaction force_isolation").uses(&[0x1fd5448]),
        Point::keep(0x836c2usize, "48 8d 05 c7 2c c6 01", "subject interaction press_sailors").uses(&[0x1ce6390]),
        Point::keep(0x836d0usize, "48 89 05 81 1d f5 01", "subject interaction press_sailors").uses(&[0x1fd5458]),
        Point::keep(0x83772usize, "48 8d 05 cf 2b c6 01", "subject interaction allow_settlement_growth").uses(&[0x1ce6348]),
        Point::keep(0x83780usize, "48 89 05 e1 1c f5 01", "subject interaction allow_settlement_growth").uses(&[0x1fd5468]),
        Point::keep(0x83822usize, "48 8d 05 d7 2a c6 01", "subject interaction block_settlement_growth").uses(&[0x1ce6300]),
        Point::keep(0x83830usize, "48 89 05 41 1c f5 01", "subject interaction block_settlement_growth").uses(&[0x1fd5478]),
    ];
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
pub fn options(Session { m, ctx, .. }: Session, args::Options { subject }: args::Options) -> GResult<Vec<Listed>> {
    let subject = countries::named(m, &ctx, subject)?;
    let country = subject_of(m, &ctx, subject)?;
    let mut listed = Vec::new();
    for i in INTERACTIONS {
        let object = object(m, i)?;
        if !offered(m, &ctx, country, object)? {
            continue;
        }
        let active = ongoing(m, country, object)?;
        let text = command(&ctx, subject, i, active, None, None);
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
/// conditions do not hold, with the reasons. `province` is the one the
/// interaction's window has the player pick, written as the command's `index`.
pub fn interact(Session { m, ctx, commands, .. }: Session, args::Interact { subject, interaction, third_party, province }: args::Interact) -> GResult<()> {
    let subject = countries::named(m, &ctx, subject)?;
    let third_party = third_party.map(|third| countries::named(m, &ctx, third)).transpose()?;
    let province = province.map(|ProvinceId(id)| id);
    let country = subject_of(m, &ctx, subject)?;
    let Some(i) = INTERACTIONS.iter().find(|i| i.key == interaction) else {
        return fail(Status::InvalidArgument, format!("the game has no subject interaction {interaction}; subject.options lists them"));
    };
    if let Some(id) = province {
        if id == 0 {
            return Err(eu4_harmony_checks::provinces::not_found(0).into());
        }
        with_state(|s| s.geography.province_address(m, &ctx, id))?;
    }
    let object = object(m, i)?;
    if !offered(m, &ctx, country, object)? {
        return fail(Status::Rejected, format!("the game does not offer {interaction} toward {}", letters(subject)));
    }
    let text = command(&ctx, subject, i, ongoing(m, country, object)?, third_party, province);
    let (can, reasons) = judge(&text)?;
    if !can {
        return Err(action::not_allowed(USE, &reasons));
    }
    action::post(commands, &text, USE)
}

/// What the player does with an interaction, as its refusals say.
const USE: &str = "use this interaction";

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
    let kind = functions::SUBJECT_TYPE.call(m, [subject])?;
    Ok(kind != 0 && TYPE_ALLOWS.call(m, [kind, object])? as u8 != 0)
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

/// The `subject` command that carries out the interaction, or stops it; the
/// province, when there is one, is its `index`.
fn command(ctx: &Ctx, subject: Tag, i: &Interaction, stop: bool, third_party: Option<Tag>, province: Option<u32>) -> String {
    let third = third_party.map(|t| format!(" third_party=\"{}\"", letters(t))).unwrap_or_default();
    let cancel = if stop { " cancel=yes" } else { "" };
    let index = province.map(|p| format!(" index={p}")).unwrap_or_default();
    format!("subject={{ overlord=\"{}\" subject=\"{}\"{third}{cancel} interaction={}{index} }}", letters(ctx.player), letters(subject), i.token)
}

/// Whether the command's check of the interaction lets the player, and its
/// reasons when not, as the view's tooltip gives them.
fn judge(text: &str) -> GResult<(bool, Vec<String>)> {
    let judged = commands::with_command(text, |m, command| {
        if CHECK.call(m, [command, 0])? as u8 != 0 {
            return Ok((true, Vec::new()));
        }
        let said = game_string::written(m, |s| CHECK.call(m, [command, s]))?.1;
        Ok((false, super::diplomacy::reasons_said(&said)))
    });
    judged.map_err(|refused| action::refused(refused, USE))
}

fn letters(tag: Tag) -> String {
    String::from_utf8_lossy(&tag.letters()).into_owned()
}

crate::catalogue::bind! {
    eu4_harmony_remote::subjects: SubjectRequest => SubjectAnswer;
    Options => options;
    Interact => interact;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    #[test]
    fn the_command_names_both_countries_and_the_interaction_s_number() {
        let ctx = Ctx { world: 0, date: 0, player: game_tag(b"TUR", 1), country: 0, countries: 0 };
        let scutage = INTERACTIONS.iter().find(|i| i.key == "scutage").unwrap();
        assert_eq!(command(&ctx, game_tag(b"SER", 2), scutage, false, None, None), "subject={ overlord=\"TUR\" subject=\"SER\" interaction=12767 }");
        assert_eq!(
            command(&ctx, game_tag(b"SER", 2), scutage, true, Some(game_tag(b"BYZ", 3)), None),
            "subject={ overlord=\"TUR\" subject=\"SER\" third_party=\"BYZ\" cancel=yes interaction=12767 }"
        );
        let grant = INTERACTIONS.iter().find(|i| i.key == "grant_province").unwrap();
        assert_eq!(
            command(&ctx, game_tag(b"SER", 2), grant, false, None, Some(149)),
            format!("subject={{ overlord=\"TUR\" subject=\"SER\" interaction={} index=149 }}", grant.token)
        );
    }

    mod through_the_layer {
        use super::super::*;
        use crate::catalogue::fake::{game_tag, tur, wal};
        use eu4_harmony_remote::CountryTag;

        use crate::game::{
            buffers::{self, Image, Object},
            commands::record,
            Caller, Fail,
        };

        extern "C" fn not_for_the_overlord(_: usize, _: usize) -> usize {
            0
        }

        fn ser() -> Tag {
            game_tag(b"SER", 2)
        }

        /// Turkey, its subject Serbia and Wallachia, a country of its own,
        /// with every interaction where the game keeps it, none for the
        /// player's country. Runs `action` over Turkey's session.
        fn turkey(action: impl FnOnce(Session) -> GResult<()>) -> GResult<()> {
            let image = Image::new();
            let m = image.memory();
            for i in INTERACTIONS {
                image
                    .u64(i.object, m.at(i.class) as u64)
                    .i32(i.object + 8, i.token as i32)
                    .u64(i.class + FOR_OVERLORD, not_for_the_overlord as *const () as u64);
            }
            let size = OVERLORD_VALID + 1;
            let (turkey, wallachia) = (Object::country(tur(), size), Object::country(wal(), size));
            let serbia = Object::country(ser(), size).u64(country_fields::OVERLORD, tur().0);
            let countries = Object::countries(&[&turkey, &serbia, &wallachia]);
            let manager = image.country_manager(&countries);
            let result = action(buffers::session(m, 0, tur(), turkey.at(), countries.at(), Caller::Remote));
            drop((turkey, serbia, wallachia, countries, manager, image));
            result
        }

        fn refused(result: GResult<()>) -> (Status, String) {
            let Err(Fail { status, message }) = result else { panic!("the interaction was carried out") };
            (status, message)
        }

        #[test]
        fn an_interaction_is_one_of_the_game_s_with_a_subject_of_the_player_s_that_the_game_offers() {
            let interact = |subject: &[u8; 3], interaction: &str, third: Option<&[u8; 3]>, province: Option<u32>| {
                let asked = args::Interact {
                    subject: CountryTag(*subject),
                    interaction: interaction.into(),
                    third_party: third.map(|t| CountryTag(*t)),
                    province: province.map(ProvinceId),
                };
                refused(turkey(|s| super::super::interact(s, asked)))
            };
            assert_eq!(interact(b"WAL", "scutage", None, None), (Status::Rejected, "WAL is not a subject of the player's".into()));
            assert_eq!(
                interact(b"SER", "tribute", None, None),
                (Status::InvalidArgument, "the game has no subject interaction tribute; subject.options lists them".into())
            );
            assert_eq!(interact(b"SER", "scutage", Some(b"HUN"), None).0, Status::NotFound);
            assert_eq!(interact(b"QQQ", "scutage", None, None).0, Status::NotFound);
            assert_eq!(interact(b"SER", "grant_province", None, Some(0)).0, Status::NotFound);
            assert_eq!(interact(b"SER", "scutage", None, None), (Status::Rejected, "the game does not offer scutage toward SER".into()));
            assert!(record::posted().is_empty());
        }
    }

    #[test]
    fn each_interaction_has_its_own_key_object_and_class() {
        for (n, i) in INTERACTIONS.iter().enumerate() {
            assert!(INTERACTIONS[..n].iter().all(|o| o.key != i.key && o.object != i.object && o.class != i.class), "{}", i.key);
        }
    }
}
