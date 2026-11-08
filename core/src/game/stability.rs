//! Stability: raising it with administrative points, as the stability and
//! expansion view's button does (docs/research/power-spending.md,
//! "Stability").
use super::{
    action,
    power::{self, BOOST_STABILITY, CAN_INCREASE_STABILITY},
    GResult, Session,
};
use crate::points::Point;
use eu4_harmony_remote::{
    choices::{Amount, Choice, PowerKind, Resource},
    stability::{StabilityAction, StabilityOption},
};

/// A country's stability, in thousandths, and the most it can be.
const STABILITY: usize = 0xbd8;
const MOST: i32 = 3000;

pub fn points() -> Vec<Point> {
    // The spend_power command's check refuses stability at its most.
    vec![Point::keep(
        0x94f6b1usize,
        "48 8b 05 18 e2 9e 01 48 8b 88 18 01 00 00 4a 8b 04 31 81 b8 d8 0b 00 00 b8 0b 00 00 0f 8c 30 03 00 00 32 c0",
        "stability at its most",
    )
    .uses(&[STABILITY, MOST as usize])]
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

/// What the stability button offers now.
pub fn options(session: Session) -> GResult<Vec<StabilityOption>> {
    Ok(vec![StabilityOption { action: StabilityAction::Raise, choice: choice(session)? }])
}

/// Raises stability by one, as the button does.
pub fn raise(session: Session) -> GResult<()> {
    action::allowed(&choice(session)?, "raise stability")?;
    power::spend(session.commands, session.ctx.player, BOOST_STABILITY, PowerKind::Adm, 0, "raise stability")
}

#[cfg(test)]
mod tests {
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

    fn ottomans(stability: i32, adm: i32) -> Ottomans {
        let image = Image::new();
        image.localisation().function(COST.rva(), fifty as *const ()).u64(NO_ID, 0).u64(FUNCTIONS, 0);
        let player = Object::country(tur(), STABILITY + 4).i32(STABILITY, stability).i32(country::POWER, adm);
        let countries = Object::countries(&[&player]);
        let session = buffers::session(image.memory(), 0, tur(), player.at(), countries.at(), Caller::Remote);
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
    fn stability_at_its_most_or_too_few_points_are_refused_with_the_button_s_tooltip() {
        assert_eq!(refused(raise(ottomans(3000, 100).session)), (Status::Rejected, "STABILITY_BOOST_MAXED".into()));
        assert_eq!(refused(raise(ottomans(0, 10).session)), (Status::Rejected, "STABILITY_BOOST_NOPOWER POWER_COST=50 POWER_AVAILABLE=10".into()));
        assert!(record::posted().is_empty());
    }
}
