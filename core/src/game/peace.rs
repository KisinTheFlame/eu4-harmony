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
    commands, context, country,
    diplomacy::{self, Action, Made, Ticket},
    game, Caller, Ctx, GResult, Tag,
};
use crate::{
    game_string,
    memory::Memory,
    objects::{
        campaign::province,
        diplomacy::{action, region, scripted, term, treaty},
    },
    points::Point,
    unicode::game_text,
};
use eu4_harmony_checks::peace as checks;
use eu4_harmony_remote::choices::{Acceptance, Amount, Choice, Resource};

/// The peace offer's constructor: (action, from, to, demand, treaty, date),
/// the treaty copied.
const PEACE_NEW: usize = 0x598af0;
/// A peace treaty's constructor and destructor: (treaty, actor, other,
/// demand), every term the peace view offers in it and none chosen;
/// (treaty).
const TREATY_NEW: usize = 0x975d40;
const TREATY_DESTROY: usize = 0x9761c0;
/// Whether the player may choose a term of a treaty: (treaty, term, list,
/// 0, reasons), `list` a `std::vector` the check fills and the caller frees,
/// `reasons` a `std::string` or null, which gets the term's tooltip.
const TERM_ALLOWED: usize = 0x97a1c0;
/// Chooses a term: (treaty, term, 0, 0).
const TERM_CHOOSE: usize = 0x97a790;
/// A treaty's war score cost, whole points: (treaty, 1).
const TREATY_COST: usize = 0x979060;
/// The gold a treaty asks, set: (treaty, &amount), in thousandths of a
/// ducat; and the most gold war score buys: (treaty, out, &war score in
/// thousandths of a point), as the peace view's gold buttons take them.
const GOLD_SET: usize = 0x97b390;
const GOLD_FOR: usize = 0x97b530;
/// What the peace view lets the gold reach: what all the war score buys,
/// in thousandths; and its step, a tenth of a ducat.
const ALL_THE_WAR_SCORE: i32 = 100_000;
const GOLD_STEP: i32 = 100;

/// The most terms a treaty is believed to hold.
const MOST_TERMS: usize = 100_000;

/// A term as a request names it ([`eu4_harmony_remote::peace::Term`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Term {
    pub term: String,
    pub province: Option<u32>,
    pub country: Option<Tag>,
    pub region: Option<String>,
    /// For `gold`, ducats.
    pub amount: Option<f64>,
}

/// A term the peace view offers.
#[derive(Clone, Debug, PartialEq)]
pub struct TermOption {
    pub term: String,
    pub province: Option<u32>,
    pub country: Option<Tag>,
    pub region: Option<String>,
    /// For `gold`, the most ducats the view lets the player set.
    pub amount: Option<f64>,
    /// Its name, as the view lists it.
    pub name: String,
    /// Whether the player can choose it now, and its war score cost.
    pub choice: Choice,
}

/// A treaty evaluated: whether the player can send it, its war score cost,
/// and whether the other country would accept it.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub choice: Choice,
    pub acceptance: Acceptance,
}

/// Every term the peace view offers against `enemy`: on the demand tab, or
/// with `offer` on the offer tab. Gold, which the view sets with its buttons
/// rather than lists, comes with the most the view lets the player set and
/// no cost of its own.
pub fn terms(_: Caller, enemy: Tag, offer: bool) -> GResult<Vec<TermOption>> {
    let m = game()?;
    let ctx = at_war(m, enemy)?;
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
            province: read.province,
            country: read.country,
            region: read.region,
            amount: if gold { Some(gold_most(m, treaty.at())? as f64 / 1000.0) } else { None },
            name: term_name(m, at, treaty.at())?,
            choice: if allowed { Choice::can(cost) } else { Choice::cannot(reasons, cost) },
        });
    }
    Ok(out)
}

/// Whether the player can send a treaty of `terms` to `enemy` (with
/// `offer`, terms they offer), its war score cost, and whether `enemy`
/// would accept it.
pub fn evaluate(_: Caller, enemy: Tag, terms: &[Term], offer: bool) -> GResult<Evaluation> {
    let m = game()?;
    let ctx = at_war(m, enemy)?;
    let treaty = composed(m, &ctx, enemy, terms, offer)?;
    let cost = vec![Amount::new(Resource::WarScore, m.call(m.at(TREATY_COST), &[treaty.at(), 1])? as i32 as f64)];
    let offered = Offer::new(m, &ctx, enemy, &treaty, offer)?;
    let choice = match diplomacy::refusals(m, offered.at(), Action::OfferPeace)? {
        None => Choice::can(cost),
        Some(reasons) => Choice::cannot(reasons, cost),
    };
    Ok(Evaluation { choice, acceptance: diplomacy::acceptance(m, offered.at())? })
}

/// Demands `terms` of `enemy`, as the peace view's demand tab sends them.
pub fn demand(_: Caller, enemy: Tag, terms: &[Term]) -> GResult<Ticket> {
    send(enemy, terms, false)
}

/// Offers `terms` to `enemy`, as the peace view's offer tab sends them.
pub fn offer(_: Caller, enemy: Tag, terms: &[Term]) -> GResult<Ticket> {
    send(enemy, terms, true)
}

fn send(enemy: Tag, terms: &[Term], offer: bool) -> GResult<Ticket> {
    let m = game()?;
    let ctx = at_war(m, enemy)?;
    let treaty = composed(m, &ctx, enemy, terms, offer)?;
    let offered = Offer::new(m, &ctx, enemy, &treaty, offer)?;
    diplomacy::verdict(m, offered.at(), Action::OfferPeace)?;
    diplomacy::send(m, &ctx, offered.at(), Action::OfferPeace, enemy, Vec::new())
}

/// The game, with `enemy` a country the player is at war with.
fn at_war(m: &Memory, enemy: Tag) -> GResult<Ctx> {
    let ctx = context(m)?;
    diplomacy::other(m, &ctx, enemy)?;
    country(m, &ctx, enemy)?;
    if !super::world::hostile(m, &ctx, ctx.player, enemy)? {
        return Err(checks::not_at_war(diplomacy::letters(enemy)).into());
    }
    Ok(ctx)
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
            return Err(if matching.is_empty() { checks::no_such_term(described(asked)) } else { checks::term_twice(described(asked)) }.into());
        };
        let gold = read.kind == term::KIND_GOLD;
        if gold && asked.amount.is_none() {
            return Err(checks::no_such_term(format!("{}: gold needs an amount", described(asked))).into());
        }
        let (can, reasons) = allowed(m, treaty.at(), at)?;
        if !can {
            return Err(checks::term_refused(described(asked), &reasons).into());
        }
        m.call(m.at(TERM_CHOOSE), &[treaty.at(), at, 0, 0])?;
        chosen.push(at);
        if let (true, Some(amount)) = (gold, asked.amount) {
            let most = gold_most(m, treaty.at())?;
            let wanted = (amount * 1000.0 / GOLD_STEP as f64).round() * GOLD_STEP as f64;
            if !(0.0..=most as f64).contains(&wanted) {
                return Err(checks::gold_out_of_range(amount, most as f64 / 1000.0).into());
            }
            let wanted = wanted as i32;
            m.call(m.at(GOLD_SET), &[treaty.at(), &wanted as *const i32 as usize])?;
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

/// The most gold the peace view lets the player set on `treaty`, in
/// thousandths of a ducat.
fn gold_most(m: &Memory, treaty: usize) -> GResult<i32> {
    let mut most = 0i32;
    let all = ALL_THE_WAR_SCORE;
    m.call(m.at(GOLD_FOR), &[treaty, &mut most as *mut i32 as usize, &all as *const i32 as usize])?;
    Ok(unsafe { std::ptr::read_volatile(&most) })
}

/// Whether the peace view lets the player choose the term at `at`, and why
/// not, its tooltip's lines.
fn allowed(m: &Memory, treaty: usize, at: usize) -> GResult<(bool, Vec<String>)> {
    let mut list = [0usize; 3];
    let list_at = list.as_mut_ptr() as usize;
    let (can, text) = game_string::written(m, |s| m.call(m.at(TERM_ALLOWED), &[treaty, at, list_at, 0, s]))?;
    commands::free_vector(m, unsafe { std::ptr::read_volatile(&list) })?;
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
    let text = |at: usize| -> GResult<String> { Ok(game_text::to_utf8(&game_string::read(m, at)?)) };
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
struct Treaty<'a> {
    m: &'a Memory,
    object: Vec<u64>,
    /// Its constructor ran, so its destructor must.
    built: bool,
}

impl<'a> Treaty<'a> {
    /// As the peace view makes it: on the demand tab, the player's demand of
    /// `enemy`; on the offer tab, `enemy` taking from the player.
    fn new(m: &'a Memory, ctx: &Ctx, enemy: Tag, offer: bool) -> GResult<Self> {
        let mut treaty = Self { m, object: vec![0; treaty::SIZE / 8], built: false };
        let (actor, other, demand) = if offer { (enemy, ctx.player, 0) } else { (ctx.player, enemy, 1) };
        m.call(m.at(TREATY_NEW), &[treaty.at(), actor.0 as usize, other.0 as usize, demand])?;
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
        if let Err(e) = self.m.call(self.m.at(TREATY_DESTROY), &[self.at()]) {
            log!("peace: destroying a treaty: {e}");
        }
    }
}

/// A peace offer of the player's to `enemy` holding a copy of a treaty, as
/// the peace view makes one for its send button: its demand flag the view's
/// tab.
struct Offer<'a>(Made<'a>);

impl<'a> Offer<'a> {
    fn new(m: &'a Memory, ctx: &Ctx, enemy: Tag, treaty: &Treaty, offer: bool) -> GResult<Self> {
        let mut made = Made::new(m, action::PEACE_SIZE);
        let demand = usize::from(!offer);
        m.call(m.at(PEACE_NEW), &[made.at(), ctx.player.0 as usize, enemy.0 as usize, demand, treaty.at(), ctx.date as u32 as usize])?;
        made.built = true;
        Ok(Self(made))
    }

    fn at(&self) -> usize {
        self.0.at()
    }
}

/// The functions peace calls; the treaty's and the terms' fields are in
/// `objects::diplomacy`, and the peace offer's constructor and the treaty's
/// with diplomacy's points.
pub fn points() -> Vec<Point> {
    vec![
        Point::keep(TERM_ALLOWED, "48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56 41 57 48 8b ec 48 81 ec", "peace term check"),
        Point::keep(TERM_CHOOSE, "48 89 6c 24 18 48 89 74 24 20 48 89 54 24 10 57 48 83 ec 30 41 0f b6 e9", "peace term chosen"),
        Point::keep(TREATY_COST, "48 8b c4 53 48 83 ec 40 8b 99 f0 00 00 00 48 89 68 08 48 8b", "peace treaty cost"),
        Point::keep(GOLD_SET, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 56 41 57 48 81 ec 50", "peace treaty gold"),
        Point::keep(GOLD_FOR, "48 83 ec 08 45 8b 00 4c 8b ca 4c 8b d9 45 85 c0 79 0c 33 c9 49 8b c1 89", "peace treaty gold for war score"),
        // The peace view's gold buttons: at most what all the war score
        // buys, in steps of a tenth.
        Point::keep(0x1311869usize, "c7 44 24 58 a0 86 01 00", "peace view gold most").uses(&[ALL_THE_WAR_SCORE as usize]),
        Point::keep(0x13118a5usize, "41 b8 32 00 00 00", "peace view gold step").uses(&[(GOLD_STEP / 2) as usize]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::fake::game_tag;

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
}
