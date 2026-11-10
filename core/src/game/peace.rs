//! Peace: the peace view, where the player demands terms of a country they
//! are at war with or offers terms to it (docs/research/peace.md).
//!
//! The framework makes a peace treaty as the peace view does, which holds
//! every term the view offers, none chosen; it reads them as the view lists
//! them, and chooses those a request names as ticking them in the view does,
//! after the same check, whose reasons are the term's tooltip. A treaty is
//! sent in a peace offer, made and sent as the diplomacy module sends the
//! white peace ([`super::diplomacy`]); the war score it costs and whether the
//! other country would accept it are the game's own answers for the treaty,
//! as the view's send button and its accept or decline icon show them.
use super::{
    commands, countries, country,
    diplomacy::{self, Made, Ticket},
    Ctx, GResult, Session, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::province,
        diplomacy::{action, region, scripted, term, treaty},
    },
    points::{GameFunction, Point},
    unicode::game_text,
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    peace::{args, Evaluation, TermOption, Terms},
    CountryTag, ProvinceId,
};

/// The peace offer's constructor: (action, from, to, demand, treaty, date),
/// the treaty copied, so it holds one of its own.
const PEACE_NEW: GameFunction<6> = GameFunction::at(0x598af0);
/// A peace treaty's constructor and destructor: (treaty, actor, other,
/// demand), every term the peace view offers in it and none chosen;
/// (treaty).
const TREATY_NEW: GameFunction<4> = GameFunction::at(0x975d40);
const TREATY_DESTROY: GameFunction<1> = GameFunction::at(0x9761c0);
/// Whether the player may choose a term of a treaty: (treaty, term, list,
/// 0, reasons), `list` a `std::vector` the check fills and the caller frees,
/// `reasons` a `std::string` or null, which gets the term's tooltip.
const TERM_ALLOWED: GameFunction<5> = GameFunction::at(0x97a1c0);
/// Chooses a term: (treaty, term, 0, 0).
const TERM_CHOOSE: GameFunction<4> = GameFunction::at(0x97a790);
/// A treaty's war score cost, whole points: (treaty, 1).
const TREATY_COST: GameFunction<2> = GameFunction::at(0x979060);
/// The most gold war score buys: (treaty, out, &war score in thousandths of
/// a point), in thousandths of a ducat, as the peace view's gold buttons
/// take it.
const GOLD_FOR: GameFunction<3> = GameFunction::at(0x97b530);
/// What the peace view lets the gold reach: what all the war score buys,
/// in thousandths; and its step, a tenth of a ducat.
const ALL_THE_WAR_SCORE: i32 = 100_000;
const GOLD_STEP: i32 = 100;

/// The most terms a treaty is believed to hold.
const MOST_TERMS: usize = 100_000;

/// A term as a request names it ([`eu4_harmony_remote::peace::Term`]), its
/// country found in the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Term {
    pub term: String,
    pub province: Option<u32>,
    pub country: Option<Tag>,
    pub region: Option<String>,
    /// For `gold`, ducats.
    pub amount: Option<f64>,
}

/// The terms a request names, each country found in the game.
fn asked(m: &Memory, ctx: &Ctx, terms: Terms) -> GResult<Vec<Term>> {
    terms
        .0
        .into_iter()
        .map(|t| {
            Ok(Term {
                term: t.term,
                province: t.province.map(|ProvinceId(p)| p),
                country: t.country.map(|c| countries::named(m, ctx, c)).transpose()?,
                region: t.region,
                amount: t.amount,
            })
        })
        .collect()
}

/// Every term the peace view offers against `country`: on the demand tab, or
/// with `offer` on the offer tab. Gold, which the view sets with its buttons
/// rather than lists, comes with the most the view lets the player set and
/// no cost of its own.
pub fn terms(Session { m, ctx, .. }: Session, args::TermOptions { country, offer }: args::TermOptions) -> GResult<Vec<TermOption>> {
    let enemy = countries::named(m, &ctx, country)?;
    let offer = offer.unwrap_or(false);
    at_war(m, &ctx, enemy)?;
    let treaty = Treaty::new(m, &ctx, enemy, offer)?;
    let mut out = Vec::new();
    for at in treaty.terms()? {
        let read = read_term(m, at)?;
        let gold = read.kind == term::KIND_GOLD;
        let (allowed, reasons) = allowed(m, treaty.at(), at)?;
        let cost =
            if gold { Vec::new() } else { vec![Amount::new(Resource::WarScore, m.call(m.vslot(at, term::COST)?, &[at, treaty.at(), 1])? as i32 as f64)] };
        out.push(TermOption {
            term: read.key,
            province: read.province.map(ProvinceId),
            country: read.country.map(|c| CountryTag(c.letters())),
            region: read.region,
            amount: if gold { Some(gold_most(m, treaty.at())? as f64 / 1000.0) } else { None },
            name: term_name(m, at, treaty.at())?,
            choice: if allowed { Choice::can(cost) } else { Choice::cannot(reasons, cost) },
        });
    }
    Ok(out)
}

/// Whether the player can send a treaty of `terms` to `country` (with
/// `offer`, terms they offer), its war score cost, and whether `country`
/// would accept it.
pub fn evaluate(Session { m, ctx, .. }: Session, args::Evaluate { country, terms, offer }: args::Evaluate) -> GResult<Evaluation> {
    let enemy = countries::named(m, &ctx, country)?;
    let terms = asked(m, &ctx, terms)?;
    let offer = offer.unwrap_or(false);
    at_war(m, &ctx, enemy)?;
    let treaty = composed(m, &ctx, enemy, &terms, offer)?;
    let cost = vec![Amount::new(Resource::WarScore, TREATY_COST.call(m, [treaty.at(), 1])? as i32 as f64)];
    let offered = Offer::new(m, &ctx, enemy, &treaty, offer)?;
    let choice = match diplomacy::refusals(m, offered.at())? {
        None => Choice::can(cost),
        Some(reasons) => Choice::cannot(reasons, cost),
    };
    Ok(Evaluation { choice, acceptance: diplomacy::acceptance(m, offered.at())? })
}

/// Demands `terms` of `country`, as the peace view's demand tab sends them.
pub fn demand(session: Session, args::Demand { country, terms }: args::Demand) -> GResult<Ticket> {
    let enemy = countries::named(session.m, &session.ctx, country)?;
    send(session, enemy, &asked(session.m, &session.ctx, terms)?, false)
}

/// Offers `terms` to `country`, as the peace view's offer tab sends them.
pub fn offer(session: Session, args::Offer { country, terms }: args::Offer) -> GResult<Ticket> {
    let enemy = countries::named(session.m, &session.ctx, country)?;
    send(session, enemy, &asked(session.m, &session.ctx, terms)?, true)
}

fn send(Session { m, ctx, commands, .. }: Session, enemy: Tag, terms: &[Term], offer: bool) -> GResult<Ticket> {
    at_war(m, &ctx, enemy)?;
    let treaty = composed(m, &ctx, enemy, terms, offer)?;
    let offered = Offer::new(m, &ctx, enemy, &treaty, offer)?;
    diplomacy::verdict(m, offered.at())?;
    diplomacy::send(commands, m, &ctx, offered.at(), enemy, Vec::new())
}

/// Whether `enemy` is a country the player is at war with.
fn at_war(m: &Memory, ctx: &Ctx, enemy: Tag) -> GResult<()> {
    diplomacy::other(m, ctx, enemy)?;
    country(m, ctx, enemy)?;
    if !super::world::at_war(m, ctx, ctx.player, enemy)? {
        return Err(checks::not_at_war(diplomacy::letters(enemy)));
    }
    Ok(())
}

/// A treaty with `terms` chosen, each as ticking it in the peace view
/// chooses it: the first term the view offers that has all the request
/// gives and is not chosen yet, if the view's check lets the player choose
/// it; gold set to its amount, within what the view allows.
fn composed<'a>(m: &'a Memory, ctx: &Ctx, enemy: Tag, terms: &[Term], offer: bool) -> GResult<Treaty<'a>> {
    let treaty = Treaty::new(m, ctx, enemy, offer)?;
    let offered: Vec<(usize, Read)> = treaty.terms()?.into_iter().map(|at| read_term(m, at).map(|r| (at, r))).collect::<GResult<_>>()?;
    let mut chosen: Vec<usize> = Vec::new();
    for asked in terms {
        let matching: Vec<&(usize, Read)> = offered.iter().filter(|(_, r)| r.matches(asked)).collect();
        let Some(&&(at, ref read)) = matching.iter().find(|(at, _)| !chosen.contains(at)) else {
            return Err(if matching.is_empty() { checks::no_such_term(described(asked)) } else { checks::term_twice(described(asked)) });
        };
        let gold = read.kind == term::KIND_GOLD;
        if gold && asked.amount.is_none() {
            return Err(checks::no_such_term(format!("{}: gold needs an amount", described(asked))));
        }
        let (can, reasons) = allowed(m, treaty.at(), at)?;
        if !can {
            return Err(checks::term_refused(described(asked), &reasons));
        }
        TERM_CHOOSE.call(m, [treaty.at(), at, 0, 0])?;
        chosen.push(at);
        if let (true, Some(amount)) = (gold, asked.amount) {
            let most = gold_most(m, treaty.at())?;
            let wanted = (amount * 1000.0 / GOLD_STEP as f64).round() * GOLD_STEP as f64;
            if !(0.0..=most as f64).contains(&wanted) {
                return Err(checks::gold_out_of_range(amount, most as f64 / 1000.0));
            }
            set_gold(m, treaty.at(), at, wanted as i32)?;
        }
    }
    Ok(treaty)
}

/// A term as a message names it.
fn described(asked: &Term) -> String {
    let mut about = Vec::new();
    if let Some(p) = asked.province {
        about.push(format!("province {p}"));
    }
    if let Some(c) = asked.country {
        about.push(format!("country {}", diplomacy::letters(c)));
    }
    if let Some(r) = &asked.region {
        about.push(format!("region {r}"));
    }
    if about.is_empty() {
        asked.term.clone()
    } else {
        format!("{} ({})", asked.term, about.join(", "))
    }
}

/// Sets the amount of the chosen gold term at `at`, as the peace view's
/// gold buttons do (`GOLD_SET`): its effect on the treaty taken back, the
/// amount changed, the effect put on again.
fn set_gold(m: &Memory, treaty_at: usize, at: usize, amount: i32) -> GResult<()> {
    let apply = m.vslot(at, term::APPLY)?;
    let (effects, demand) = (treaty_at + treaty::EFFECTS, m.u8(treaty_at + treaty::DEMAND)? as usize);
    m.call(apply, &[at, effects, demand, 1])?;
    m.write(at + term::GOLD, &amount.to_le_bytes())?;
    m.call(apply, &[at, effects, demand, 0])?;
    Ok(())
}

/// The most gold the peace view lets the player set on `treaty`, in
/// thousandths of a ducat.
fn gold_most(m: &Memory, treaty: usize) -> GResult<i32> {
    let mut most = 0i32;
    let all = ALL_THE_WAR_SCORE;
    GOLD_FOR.call(m, [treaty, &mut most as *mut i32 as usize, &all as *const i32 as usize])?;
    Ok(unsafe { std::ptr::read_volatile(&most) })
}

/// Whether the peace view lets the player choose the term at `at`, and why
/// not, its tooltip's lines.
fn allowed(m: &Memory, treaty: usize, at: usize) -> GResult<(bool, Vec<String>)> {
    let mut list = [0usize; 3];
    let list_at = list.as_mut_ptr() as usize;
    let (can, text) = game_string::written(m, |s| TERM_ALLOWED.call(m, [treaty, at, list_at, 0, s]))?;
    crate::game_heap::free_vector(m, unsafe { std::ptr::read_volatile(&list) })?;
    Ok((can as u8 != 0, if can as u8 != 0 { Vec::new() } else { diplomacy::text_lines(&text) }))
}

/// A term's name, as the peace view lists it.
fn term_name(m: &Memory, at: usize, treaty: usize) -> GResult<String> {
    let name = m.vslot(at, term::NAME)?;
    Ok(game_text::plain(&game_string::written(m, |s| m.call(name, &[at, s, treaty]))?.1).trim().to_string())
}

/// A term, as the framework reads it.
#[derive(Clone, Debug, PartialEq)]
struct Read {
    kind: i32,
    /// Its key, as requests name it.
    key: String,
    province: Option<u32>,
    country: Option<Tag>,
    region: Option<String>,
}

impl Read {
    /// Whether it has all `asked` gives.
    fn matches(&self, asked: &Term) -> bool {
        self.key == asked.term
            && asked.province.is_none_or(|p| self.province == Some(p))
            && asked.country.is_none_or(|c| self.country.is_some_and(|t| t.index() == c.index()))
            && asked.region.as_ref().is_none_or(|r| self.region.as_ref() == Some(r))
    }
}

/// The term at `at`: its kind, its key and what it is about.
fn read_term(m: &Memory, at: usize) -> GResult<Read> {
    let mut out = [0u64; 2];
    let kind = m.call(m.vslot(at, term::KIND)?, &[at, out.as_mut_ptr() as usize])?;
    let kind = m.i32(kind)?;
    let text = |at: usize| -> GResult<String> { Ok(game_string::key(m, at)?) };
    let key = if kind == term::KIND_SCRIPTED {
        text(m.ptr(at + term::SCRIPTED)? + scripted::KEY)?
    } else {
        let token = m.call(m.vslot(at, term::TOKEN)?, &[at])? as u32;
        let name = commands::type_name(m, token).ok_or_else(|| super::Fail::from(format!("a peace treaty term of token {token:#x}, which has no name")))?;
        term_key(&name)
    };
    let tag = Tag(m.u64(at + term::TAKER)?);
    let province = if term::WITH_PROVINCE.contains(&kind) {
        let p = m.ptr(at + term::PROVINCE)?;
        if p == 0 {
            None
        } else {
            Some(m.i32(p + province::ID)? as u32)
        }
    } else {
        None
    };
    Ok(Read {
        kind,
        key,
        province,
        country: (term::WITH_TAKER.contains(&kind) && tag.valid() && tag.normal()).then_some(tag),
        region: if kind == term::KIND_REGION { Some(text(m.ptr(m.ptr(at + term::REGION)? + region::OBJECT)? + region::KEY)?) } else { None },
    })
}

/// A term's key, as requests name it: its type's name without `po_`.
fn term_key(name: &str) -> String {
    name.strip_prefix("po_").unwrap_or(name).to_string()
}

/// A peace treaty the framework made; destroyed when dropped.
pub(super) struct Treaty<'a> {
    m: &'a Memory,
    object: Vec<u64>,
    /// Its constructor ran, so its destructor must.
    built: bool,
}

impl<'a> Treaty<'a> {
    /// As the peace view makes it: on the demand tab, the player's demand of
    /// `enemy`; on the offer tab, `enemy` taking from the player.
    pub(super) fn new(m: &'a Memory, ctx: &Ctx, enemy: Tag, offer: bool) -> GResult<Self> {
        let mut treaty = Self { m, object: vec![0; treaty::SIZE / 8], built: false };
        let (actor, other, demand) = if offer { (enemy, ctx.player, 0) } else { (ctx.player, enemy, 1) };
        TREATY_NEW.call(m, [treaty.at(), actor.0 as usize, other.0 as usize, demand])?;
        treaty.built = true;
        Ok(treaty)
    }

    fn at(&self) -> usize {
        self.object.as_ptr() as usize
    }

    /// Every term it holds.
    fn terms(&self) -> GResult<Vec<usize>> {
        let (start, end) = (self.m.ptr(self.at() + treaty::TERMS)?, self.m.ptr(self.at() + treaty::TERMS + 8)?);
        let size = end.wrapping_sub(start);
        if end < start || size % 8 != 0 || size / 8 > MOST_TERMS {
            return Err("a peace treaty's terms are out of bounds".to_string().into());
        }
        Ok(self.m.bytes(start, size)?.chunks_exact(8).map(|p| u64::from_le_bytes(p.try_into().unwrap()) as usize).filter(|&p| p != 0).collect())
    }
}

impl Drop for Treaty<'_> {
    fn drop(&mut self) {
        if !self.built {
            return;
        }
        if let Err(e) = TREATY_DESTROY.call(self.m, [self.at()]) {
            log!("peace: destroying a treaty: {e}");
        }
    }
}

/// A peace offer of the player's to `enemy` holding a copy of a treaty, as
/// the peace view makes one for its send button: its demand flag the view's
/// tab.
pub(super) struct Offer<'a>(Made<'a>);

impl<'a> Offer<'a> {
    pub(super) fn new(m: &'a Memory, ctx: &Ctx, enemy: Tag, treaty: &Treaty, offer: bool) -> GResult<Self> {
        let mut made = Made::new(m, action::PEACE_SIZE);
        let demand = usize::from(!offer);
        PEACE_NEW.call(m, [made.at(), ctx.player.0 as usize, enemy.0 as usize, demand, treaty.at(), ctx.date as u32 as usize])?;
        made.built = true;
        Ok(Self(made))
    }

    pub(super) fn at(&self) -> usize {
        self.0.at()
    }
}

/// The functions peace calls; the treaty's and the terms' fields are in
/// `objects::diplomacy`.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(PEACE_NEW, "48 89 4c 24 08 53 48 83 ec 20 48 8b d9", "peace offer constructor"),
        Point::keep(TREATY_NEW, "48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48 89 4c 24 08 57 41 56 41 57 48 83 ec 20", "peace treaty constructor"),
        Point::keep(TREATY_DESTROY, "48 89 5c 24 08 57 48 83 ec 20 48 8d 05 e7 16 2e 01", "peace treaty destructor"),
        Point::keep(TERM_ALLOWED, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b ec 48 81 ec", "peace term check"),
        Point::keep(TERM_CHOOSE, "48 89 6c 24 18 48 89 74 24 20 48 89 54 24 10 57 48 83 ec 30 41 0f b6 e9", "peace term chosen"),
        Point::keep(TREATY_COST, "48 8b c4 53 48 83 ec 40 8b 99 f0 00 00 00 48 89 68 08 48 8b", "peace treaty cost"),
        // Setting the gold: the chosen term's effect taken back, the amount
        // written, the effect put on again.
        Point::keep(0x97b417usize, "48 8b 07 41 b1 01 44 0f b6 46 20 48 8d 96 e8 00 00 00 48 8b cf ff 90 18 01 00 00", "peace treaty gold").uses(&[
            treaty::DEMAND,
            treaty::EFFECTS,
            term::APPLY,
        ]),
        Point::keep(0x97b432usize, "41 8b 06 89 47 30", "peace treaty gold amount").uses(&[term::GOLD]),
        Point::keep(GOLD_FOR, "48 83 ec 08 45 8b 00 4c 8b ca 4c 8b d9 45 85 c0 79 0c 33 c9 49 8b c1 89", "peace treaty gold for war score"),
        // The peace view's gold buttons: at most what all the war score
        // buys, in steps of a tenth.
        Point::keep(0x1311869usize, "c7 44 24 58 a0 86 01 00", "peace view gold most").uses(&[ALL_THE_WAR_SCORE as usize]),
        Point::keep(0x13118a5usize, "41 b8 32 00 00 00", "peace view gold step").uses(&[(GOLD_STEP / 2) as usize]),
    ]
}

/// Peace, the transfer of occupation and calls to arms: the refusals the
/// framework words itself. The game's own reasons are passed through as its
/// tooltips give them, one a line.
pub mod checks {
    use crate::game::{Fail, Status};

    use std::fmt::Display;

    /// The refusal of a country the player is not at war with, where the peace
    /// view needs one they are.
    pub fn not_at_war(enemy: impl Display) -> Fail {
        Fail::new(Status::Rejected, format!("the player is not at war with {enemy}"))
    }

    /// The refusal of a term the peace view does not offer: `term` as the
    /// request names it.
    pub fn no_such_term(term: impl Display) -> Fail {
        Fail::new(Status::InvalidArgument, format!("the peace view offers no term {term}; peace.terms lists those it offers"))
    }

    /// The refusal of the same term given twice.
    pub fn term_twice(term: impl Display) -> Fail {
        Fail::new(Status::InvalidArgument, format!("the term {term} is given twice"))
    }

    /// The refusal of a term the peace view does not let the player add, with
    /// its tooltip's `reasons`, one a line.
    pub fn term_refused(term: impl Display, reasons: &[String]) -> Fail {
        let refused = format!("the peace view does not let the player add {term}");
        Fail::new(Status::Rejected, if reasons.is_empty() { format!("{refused}, and gives no reason") } else { format!("{refused}:\n{}", reasons.join("\n")) })
    }

    /// The refusal of an amount of gold the peace view does not let the player
    /// set: below nothing, or above `most`, in ducats.
    pub fn gold_out_of_range(amount: f64, most: f64) -> Fail {
        Fail::new(Status::Rejected, format!("the peace view lets the player set from 0 to {most:.1} ducats, not {amount}"))
    }

    /// The refusal of an ally the transfer occupation window does not list for
    /// a province.
    pub fn not_a_receiver(ally: impl Display, province: u32) -> Fail {
        Fail::new(Status::Rejected, format!("the transfer occupation window does not list {ally} to take province {province}"))
    }

    /// The refusal of a call to arms the diplomacy panel does not offer.
    pub fn no_call_to_arms(ally: impl Display) -> Fail {
        Fail::new(Status::Rejected, format!("the diplomacy panel offers no call to arms to {ally}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_term_refused_gives_the_view_s_reasons() {
            assert_eq!(
                term_refused("humiliate", &["Already humiliated".into()]).message,
                "the peace view does not let the player add humiliate:\nAlready humiliated"
            );
            assert_eq!(term_refused("humiliate", &[]).message, "the peace view does not let the player add humiliate, and gives no reason");
            assert_eq!(no_such_term("demand_provinces 236").status, Status::InvalidArgument);
            assert_eq!(gold_out_of_range(500.0, 123.44).message, "the peace view lets the player set from 0 to 123.4 ducats, not 500");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::fake::game_tag;

    #[test]
    fn a_term_is_named_by_its_type_without_po() {
        assert_eq!(term_key("po_demand_provinces"), "demand_provinces");
        assert_eq!(term_key("revoke_reform"), "revoke_reform");
    }

    #[test]
    fn a_term_asked_matches_what_it_gives() {
        let eng = game_tag(b"ENG", 12);
        let read = Read { kind: 3, key: "demand_provinces".into(), province: Some(236), country: Some(eng), region: None };
        let asked = |province: Option<u32>, country: Option<Tag>| Term { term: "demand_provinces".into(), province, country, ..Default::default() };
        assert!(read.matches(&asked(None, None)));
        assert!(read.matches(&asked(Some(236), Some(game_tag(b"ENG", 12)))));
        assert!(!read.matches(&asked(Some(237), None)));
        assert!(!read.matches(&asked(None, Some(game_tag(b"FRA", 13)))));
        assert!(!read.matches(&Term { term: "humiliate".into(), ..Default::default() }));
        assert_eq!(described(&asked(Some(236), Some(eng))), "demand_provinces (province 236, country ENG)");
        assert_eq!(described(&Term { term: "humiliate".into(), ..Default::default() }), "humiliate");
    }

    mod through_the_layer {
        use crate::game::As;
        use super::super::*;
        use crate::catalogue::fake::{byz, tur};
        use eu4_harmony_remote::CountryTag;

        use crate::game::{
            buffers::{self, Image, Object},
            commands::record,
            Caller, Fail, Status,
        };
        use crate::objects::campaign::{country as country_fields, globals};

        /// Turkey and Byzantium at peace. Runs `action` over Turkey's
        /// session.
        fn turkey<T>(action: impl FnOnce(Session) -> GResult<T>) -> GResult<T> {
            let image = Image::new();
            for (n, rva) in globals::SPECIAL_COUNTRIES.into_iter().enumerate() {
                image.i32(rva, 1000 + n as i32);
            }
            let turkey = Object::country(tur(), country_fields::HOSTILE_COUNT + 4);
            let byzantium = Object::country(byz(), country_fields::HOSTILE_COUNT + 4);
            let countries = Object::countries(&[&turkey, &byzantium]);
            let manager = image.country_manager(&countries);
            let result = action(buffers::session(image.memory(), 0, tur(), turkey.at(), countries.at(), Caller::Remote(As::Player)));
            drop((turkey, byzantium, countries, manager, image));
            result
        }

        fn refused<T: std::fmt::Debug>(result: GResult<T>) -> (Status, String) {
            let Err(Fail { status, message }) = result else { panic!("the action was carried out: {result:?}") };
            (status, message)
        }

        #[test]
        fn a_treaty_is_with_another_country_the_player_is_at_war_with() {
            let humiliate =
                || Terms(vec![eu4_harmony_remote::peace::Term { term: "humiliate".into(), province: None, country: None, region: None, amount: None }]);
            let (byz, tur) = (CountryTag(*b"BYZ"), CountryTag(*b"TUR"));
            let not_at_war = (Status::Rejected, "the player is not at war with BYZ".to_string());
            let with_byzantium = [
                refused(turkey(|s| terms(s, args::TermOptions { country: byz, offer: None }))),
                refused(turkey(|s| evaluate(s, args::Evaluate { country: byz, terms: humiliate(), offer: None }))),
                refused(turkey(|s| demand(s, args::Demand { country: byz, terms: humiliate() }))),
                refused(turkey(|s| offer(s, args::Offer { country: byz, terms: humiliate() }))),
            ];
            assert!(with_byzantium.iter().all(|r| *r == not_at_war), "{with_byzantium:?}");
            assert_eq!(
                refused(turkey(|s| demand(s, args::Demand { country: tur, terms: humiliate() }))),
                (Status::InvalidArgument, "the player's own country".into())
            );
            assert_eq!(refused(turkey(|s| demand(s, args::Demand { country: CountryTag(*b"QQQ"), terms: humiliate() }))).0, Status::NotFound);
            assert!(record::posted().is_empty());
        }
    }
}
