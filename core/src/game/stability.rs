//! Stability: raising it with administrative points and reducing war
//! exhaustion with diplomatic points, as the stability and expansion view's
//! buttons do (docs/research/power-spending.md, "Stability" and "The
//! country's other spendings"), and treating a rebel faction harshly with
//! military points, as its rebels' buttons do ([`super::spendings`]).
use super::commands::{write, Block};
use super::{
    action,
    power::{self, BOOST_STABILITY, CAN_INCREASE_STABILITY, CAN_REDUCE_WAR_EXHAUSTION},
    spendings, Ctx, GResult, Session,
};
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::country::STABILITY,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    stability::{args, StabilityAction, StabilityOption},
};

/// The most a country's stability, in thousandths, can be.
const MOST: i32 = 3000;
/// The spending that reduces war exhaustion, paid in diplomatic points.
const REDUCE_WAR_EXHAUSTION: i32 = 22;
/// `(country, text)`: whether the country may reduce war exhaustion now,
/// the spend_power check's own for the kind, writing the button's reasons
/// when not; `can_reduce_war_exhaustion` failing writes none.
const MAY_REDUCE_WAR_EXHAUSTION: GameFunction<2> = GameFunction::at(0x38d270);
/// `(country, text)`: whether the country may change its native policy at
/// all now, writing the view's reasons when not (no colonists, stability
/// at −3); the command's check also refuses the policy it has.
const MAY_SET_NATIVE_POLICY: GameFunction<2> = GameFunction::at(0x3ed100);
/// The country's native policy, an i32: the place in [`NATIVE_POLICIES`].
const NATIVE_POLICY: usize = 0x286c;
/// The native policies by number, as the game's tokens name them; the
/// command refuses 4 and above. 0, none, is not one of the view's buttons.
const NATIVE_POLICIES: [&str; 4] = ["native_policy_none", "native_policy_coexist", "native_policy_trade", "native_policy_hostile"];

pub fn points() -> Vec<Point> {
    vec![
        // The spend_power command's check refuses stability at its most.
        Point::keep(
            0x94f6b1usize,
            "48 8b 05 18 e2 9e 01 48 8b 88 18 01 00 00 4a 8b 04 31 81 b8 d8 0b 00 00 b8 0b 00 00 0f 8c 30 03 00 00 32 c0",
            "stability at its most",
        )
        .uses(&[STABILITY, MOST as usize]),
        Point::keep(MAY_REDUCE_WAR_EXHAUSTION, "48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8d ac 24 40 ff ff ff 48 81 ec c0 01 00 00", "may reduce war exhaustion"),
        // The spend_power check's case for the kind asks it, and the view's
        // button builds its spending from this constant: the kind,
        // diplomatic points, no values.
        Point::keep(0x94f6dausize, "33 d2 48 8b cf e8 8c db a3 ff", "war exhaustion's spend check"),
        Point::keep(0x1dc27b0usize, "16 00 00 00 01 00 00 00 00 00 00 00 ff ff ff ff", "war exhaustion's spending"),
        Point::keep(MAY_SET_NATIVE_POLICY, "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 60 48 8b da 48 8b f1", "may set the native policy"),
        // The native policy command's check: the policy below 4 and not the
        // country's own; the view's button posts it with the policy's number.
        Point::keep(0x447201usize, "8b 4f 58 83 f9 04 7d 5e 39 8b 6c 28 00 00 74 56", "native policy command check").uses(&[NATIVE_POLICY]),
        Point::keep(0xf841bcusize, "c7 43 44 ed 32 00 00", "native policy button's command"),
        Point::keep(0x446f9eusize, "ba 00 29 00 00", "native policy command's policy"),
    ]
}

/// Whether the player can set the native policy `policy` (its number) now.
fn native_policy(m: &Memory, ctx: &Ctx, policy: usize) -> GResult<Choice> {
    let (may, why) = game_string::written(m, |out| MAY_SET_NATIVE_POLICY.call(m, [ctx.country, out]))?;
    if may & 0xff == 0 {
        return Ok(Choice::cannot(super::reasons(&why), Vec::new()));
    }
    if m.i32(ctx.country + NATIVE_POLICY)? == policy as i32 {
        return Ok(Choice::cannot(vec![checks::SET_ALREADY.to_string()], Vec::new()));
    }
    Ok(Choice::can(Vec::new()))
}

/// The native policy command: the country and the policy's number.
fn native_policy_written(tag: super::Tag, policy: usize) -> String {
    write("set_native_policy", Block::new().tag("country", tag.letters()).word("policy", policy))
}

/// Whether the player can reduce war exhaustion now, with the button's
/// reasons, or `can_reduce_war_exhaustion`'s failing lines when it writes
/// none, and the cost.
fn war_exhaustion(m: &Memory, ctx: &Ctx) -> GResult<Choice> {
    let cost = power::cost(m, ctx.player, REDUCE_WAR_EXHAUSTION, PowerKind::Dip, 0)?;
    let price = vec![Amount::new(Resource::Dip, cost as f64)];
    let (may, why) = game_string::written(m, |out| MAY_REDUCE_WAR_EXHAUSTION.call(m, [ctx.country, out]))?;
    if may & 0xff != 0 {
        return Ok(Choice::can(price));
    }
    let mut reasons = super::reasons(&why);
    if reasons.is_empty() {
        reasons = power::scripted(m, CAN_REDUCE_WAR_EXHAUSTION, ctx.player)?.1;
    }
    Ok(Choice::cannot(reasons, price))
}

/// Whether the player can raise stability now, as the button's tooltip
/// says: not at the most, not when `can_increase_stability` does not allow
/// it, not with too few points.
fn choice(Session { m, ctx, .. }: Session) -> GResult<Choice> {
    let cost = power::cost(m, ctx.player, BOOST_STABILITY, PowerKind::Adm, 0)?;
    let price = vec![Amount::new(Resource::Adm, cost as f64)];
    if m.i32(ctx.country + STABILITY)? >= MOST {
        return Ok(Choice::cannot(power::tooltip(m, c"STABILITY_BOOST_MAXED", &[])?, price));
    }
    let (allowed, why) = power::scripted(m, CAN_INCREASE_STABILITY, ctx.player)?;
    if !allowed {
        return Ok(Choice::cannot(why, price));
    }
    let have = power::points_of(m, ctx.country, PowerKind::Adm)?;
    if cost > have {
        let values = [(c"POWER_COST", power::whole(cost)), (c"POWER_AVAILABLE", power::whole(have))];
        return Ok(Choice::cannot(power::tooltip(m, c"STABILITY_BOOST_NOPOWER", &values)?, price));
    }
    Ok(Choice::can(price))
}

/// What the view's buttons offer now: raising stability, reducing war
/// exhaustion, then harsh treatment of each rebel faction against the
/// player.
pub fn options(session: Session) -> GResult<Vec<StabilityOption>> {
    let plain = |action, choice| StabilityOption { action, faction: None, rebels: None, name: None, policy: None, choice };
    let mut out = vec![plain(StabilityAction::Raise, choice(session)?)];
    let Session { m, ctx, .. } = session;
    out.push(plain(StabilityAction::ReduceWarExhaustion, war_exhaustion(m, &ctx)?));
    for faction in spendings::factions(m, &ctx)? {
        let choice = spendings::harsh_treatment(m, &ctx, &faction)?;
        out.push(StabilityOption {
            action: StabilityAction::HarshTreatment,
            faction: Some(faction.number()),
            rebels: Some(faction.rebels),
            name: Some(faction.name),
            policy: None,
            choice,
        });
    }
    for (number, key) in NATIVE_POLICIES.iter().enumerate().skip(1) {
        let choice = native_policy(m, &ctx, number)?;
        out.push(StabilityOption { policy: Some(key.to_string()), ..plain(StabilityAction::SetNativePolicy, choice) });
    }
    Ok(out)
}

/// Sets the player's native policy, as the view's button does.
pub fn set_native_policy(Session { m, ctx, commands, .. }: Session, args::SetNativePolicy { policy }: args::SetNativePolicy) -> GResult<()> {
    let Some(number) = NATIVE_POLICIES.iter().skip(1).position(|k| *k == policy).map(|i| i + 1) else {
        return Err(checks::no_policy(&policy));
    };
    action::carry_out(commands, &native_policy(m, &ctx, number)?, "set this native policy", || Ok(native_policy_written(ctx.player, number)))
}

/// Treats the rebel faction of number `faction` harshly, as its button does
/// once the player confirms.
pub fn harsh_treatment(Session { m, ctx, commands, .. }: Session, args::HarshTreatment { faction: number }: args::HarshTreatment) -> GResult<()> {
    let Some(faction) = spendings::factions(m, &ctx)?.into_iter().find(|f| f.number() == number) else {
        return Err(checks::no_faction(number));
    };
    let choice = spendings::harsh_treatment(m, &ctx, &faction)?;
    action::carry_out(commands, &choice, "treat these rebels harshly", || Ok(spendings::harsh_written(ctx.player, faction.id)))
}

/// The refusals the framework words itself in the stability view.
pub mod checks {
    use crate::game::{Fail, Status};

    /// The refusal of a number that names no rebel faction against the
    /// player.
    pub fn no_faction(number: u32) -> Fail {
        Fail::new(Status::NotFound, format!("no rebel faction {number} rises against the player"))
    }

    /// The refusal of a policy the game has not.
    pub fn no_policy(policy: &str) -> Fail {
        Fail::new(Status::InvalidArgument, format!("{policy} is no native policy: native_policy_coexist, native_policy_trade or native_policy_hostile"))
    }

    /// The policy the player has already: the command's check refuses it
    /// without a word.
    pub const SET_ALREADY: &str = "This is the native policy already.";
}

/// Raises stability by one, as the button does.
pub fn raise(session: Session) -> GResult<()> {
    action::allowed(&choice(session)?, "raise stability")?;
    power::spend(session.commands, session.ctx.player, BOOST_STABILITY, PowerKind::Adm, 0, "raise stability")
}

/// Reduces war exhaustion, as the button does.
pub fn reduce_war_exhaustion(Session { m, ctx, commands, .. }: Session) -> GResult<()> {
    action::allowed(&war_exhaustion(m, &ctx)?, "reduce war exhaustion")?;
    power::spend(commands, ctx.player, REDUCE_WAR_EXHAUSTION, PowerKind::Dip, 0, "reduce war exhaustion")
}

crate::catalogue::bind! {
    eu4_harmony_remote::stability: StabilityRequest => StabilityAnswer;
    Options => options;
    Raise => raise;
    ReduceWarExhaustion => reduce_war_exhaustion;
    SetNativePolicy => set_native_policy;
    HarshTreatment => harsh_treatment;
}

#[cfg(test)]
mod tests {
    use crate::game::As;
    use super::*;
    use crate::catalogue::fake::tur;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::record,
        power::{COST, FUNCTIONS, NO_ID},
        Caller, Fail, Status,
    };
    use crate::objects::campaign::country;

    /// What a spending costs: 50 points.
    extern "C" fn fifty() -> usize {
        50
    }

    /// The Ottomans at `stability` thousandths, with `adm` administrative
    /// points, raising stability costing 50; no scripted function forbids
    /// it.
    struct Ottomans {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    /// The war exhaustion check: no war exhaustion while stability is 0,
    /// as this test arranges it.
    extern "C" fn may_reduce_war_exhaustion(country: usize, text: usize) -> usize {
        let stability = unsafe { *((country + STABILITY) as *const i32) };
        if stability != 0 {
            return 1;
        }
        unsafe { crate::game::buffers::write_reason(text, b"NOREDUCE_WAR_NO_WE") };
        0
    }

    /// The native policy check: stability at −3 refuses it, as the game's.
    extern "C" fn may_set_native_policy(country: usize, text: usize) -> usize {
        let stability = unsafe { *((country + STABILITY) as *const i32) };
        if stability != -3000 {
            return 1;
        }
        unsafe { crate::game::buffers::write_reason(text, b"NATIVE_POLICY_STAB_TOO_LOW") };
        0
    }

    fn ottomans(stability: i32, adm: i32) -> Ottomans {
        let image = Image::new();
        image
            .localisation()
            .function(COST.rva(), fifty as *const ())
            .u64(NO_ID, 0)
            .u64(FUNCTIONS, 0)
            .function(MAY_REDUCE_WAR_EXHAUSTION.rva(), may_reduce_war_exhaustion as *const ())
            .function(MAY_SET_NATIVE_POLICY.rva(), may_set_native_policy as *const ());
        let size = (country::REBEL_FACTIONS + 16).max(NATIVE_POLICY + 4);
        let player = Object::country(tur(), size).i32(STABILITY, stability).i32(country::POWER, adm).i32(NATIVE_POLICY, 1);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), 0, tur(), player.at(), countries.at(), Caller::Remote(As::Player));
        Ottomans { _image: image, _objects: vec![player, countries], session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("{result:?} was posted") };
        (status, message)
    }

    #[test]
    fn stability_is_raised_with_the_button_s_spending_for_its_cost() {
        let ottomans = ottomans(1000, 100);
        let options = options(ottomans.session).unwrap();
        assert!(options[0].choice.can);
        assert_eq!(options[0].choice.cost, [Amount::new(Resource::Adm, 50.0)]);
        raise(ottomans.session).unwrap();
        assert_eq!(record::posted(), [r#"spend_power_command={ country="TUR" power={ category=2 skill=0 } }"#]);
    }

    #[test]
    fn war_exhaustion_is_reduced_with_the_button_s_spending_or_refused_with_its_reasons() {
        let ottomans = ottomans(1000, 100);
        let options = options(ottomans.session).unwrap();
        assert_eq!(options[1].action, StabilityAction::ReduceWarExhaustion);
        assert!(options[1].choice.can);
        assert_eq!(options[1].choice.cost, [Amount::new(Resource::Dip, 50.0)]);
        reduce_war_exhaustion(ottomans.session).unwrap();
        assert_eq!(record::posted(), [r#"spend_power_command={ country="TUR" power={ category=22 skill=1 } }"#]);
        assert_eq!(refused(reduce_war_exhaustion(self::ottomans(0, 100).session)), (Status::Rejected, "NOREDUCE_WAR_NO_WE".into()));
        assert!(record::posted().is_empty());
    }

    #[test]
    fn a_native_policy_is_set_by_its_number_but_the_one_it_has() {
        let ottomans = ottomans(1000, 100);
        let options = options(ottomans.session).unwrap();
        let policies: Vec<_> =
            options.iter().filter(|o| o.action == StabilityAction::SetNativePolicy).map(|o| (o.policy.clone().unwrap(), o.choice.can)).collect();
        assert_eq!(policies, [("native_policy_coexist".into(), false), ("native_policy_trade".into(), true), ("native_policy_hostile".into(), true)]);
        let set = |policy: &str| set_native_policy(ottomans.session, args::SetNativePolicy { policy: policy.into() });
        set("native_policy_hostile").unwrap();
        assert_eq!(record::posted(), [r#"set_native_policy={ country="TUR" policy=3 }"#]);
        assert_eq!(refused(set("native_policy_coexist")), (Status::Rejected, checks::SET_ALREADY.into()));
        assert_eq!(refused(set("native_policy_none")).0, Status::InvalidArgument);
        assert_eq!(refused(set_native_policy(self::ottomans(-3000, 100).session, args::SetNativePolicy { policy: "native_policy_trade".into() })), (Status::Rejected, "NATIVE_POLICY_STAB_TOO_LOW".into()));
        assert!(record::posted().is_empty());
    }

    #[test]
    fn stability_at_its_most_or_too_few_points_are_refused_with_the_button_s_tooltip() {
        assert_eq!(refused(raise(ottomans(3000, 100).session)), (Status::Rejected, "STABILITY_BOOST_MAXED".into()));
        assert_eq!(refused(raise(ottomans(0, 10).session)), (Status::Rejected, "STABILITY_BOOST_NOPOWER POWER_COST=50 POWER_AVAILABLE=10".into()));
        assert!(record::posted().is_empty());
    }
}
