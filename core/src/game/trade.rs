//! Trade: the trade nodes, and one node as the trade node view shows it
//! (docs/research/trade.md). The view shows every country's trade power and
//! merchants in any node the player has discovered, so the player's view
//! leaves out only the nodes none of whose provinces the player has
//! discovered, which the trade map leaves out.
use super::{thousandths, with_state, world, Ctx, GResult, Session, Status, Tag, View};
use crate::containers;
use crate::{
    game_string,
    memory::Memory,
    objects::campaign::{globals, province},
    points::{GameFunction, Point},
};
use eu4_harmony_remote::trade::args;
use eu4_harmony_remote::{
    trade::{Flow, Merchant, MerchantAction, NodeValue, TradeNode, TradePower},
    CountryTag,
};
use std::collections::BTreeSet;

/// The world's trade nodes: inline, [`NODE_SIZE`] bytes each, at the
/// pointer [`NODES`], their count (i32) at [`NODE_COUNT`]; a node's place is
/// its definition's index, and place 0 is no node.
pub(super) const NODES: usize = 0x21b0;
pub(super) const NODE_COUNT: usize = 0x21bc;
pub(super) const NODE_SIZE: usize = 0x138;
/// A node's fields: its definition; its value and the part its provinces
/// make, fixed point in ducats; the trade power of all its countries; the
/// countries' entries, a pointer to [`ENTRY_SIZE`]-byte entries by the
/// country's index; what comes in, a vector (begin, end) of
/// [`INCOMING_SIZE`]-byte items.
pub(super) const DEFINITION: usize = province::TRADE_NODE_DEFINITION;
const VALUE: usize = 0xb0;
const LOCAL_VALUE: usize = 0xb4;
const TOTAL_POWER: usize = 0xc8;
pub(super) const ENTRIES: usize = 0x18;
pub(super) const ENTRY_SIZE: usize = 0xc0;
const INCOMING: usize = 0xf0;
const INCOMING_SIZE: usize = 0x20;
/// An incoming item's value, fixed point, and the definition it comes from.
const INCOMING_VALUE: usize = 0x10;
const INCOMING_FROM: usize = 0x18;
/// A definition's key and name (`std::string`s), its index, and its links
/// downstream, a vector (begin, end) of [`LINK_SIZE`]-byte items, each with
/// the definition it leads to at [`LINK_TO`].
pub(super) const KEY: usize = province::TRADE_NODE_KEY;
pub(super) const NAME: usize = 0x30;
pub(super) const INDEX: usize = 0xd8;
pub(super) const LINKS: usize = 0x98;
pub(super) const LINK_SIZE: usize = 0x78;
pub(super) const LINK_TO: usize = 0x30;
/// A country's entry: its tag; its trade power before what it gives and
/// gets (`val`), the demand and power that bound it, what it gives away and
/// gets, fixed point; its merchant: whether it has one, of what kind
/// ([`STEER`] or collect), toward which link; whether it is its home node.
const ENTRY_TAG: usize = 0x10;
const OWN_POWER: usize = 0x48;
const MAX_DEMAND: usize = 0x44;
const MAX_POWER: usize = 0x4c;
const GIVEN: usize = 0x50;
const GOT: usize = 0x54;
pub(super) const HAS_MERCHANT: usize = 0xae;
const MERCHANT_KIND: usize = 0xac;
const STEER: u8 = 1;
pub(super) const STEER_LINK: usize = 0xa8;
pub(super) const HOME: usize = 0xad;
/// The game's share of a country in a node: (node, out, tag), thousandths;
/// whether a country collects there: (node, tag).
const SHARE: GameFunction<3> = GameFunction::at(0xb573a0);
pub(super) const COLLECTS: GameFunction<2> = GameFunction::at(0xb58e00);
/// The most nodes, links or flows believed.
pub(super) const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // A province's node found by its definition's index.
        Point::keep(
            0x8bd173usize,
            "48 63 83 d8 00 00 00 48 8b 0d f7 2c a8 01 85 c0 7e 18 3b 81 bc 21 00 00 7d 10 48 69 c0 38 01 00 00 48 03 81 b0 21 00 00",
            "trade node by index",
        )
        .uses(&[INDEX, globals::WORLD, NODE_COUNT, NODE_SIZE, NODES]),
        Point::keep(0x25c175usize, "4c 8b a0 a8 00 00 00 49 83 c4 30", "trade node name").uses(&[DEFINITION, NAME]),
        Point::keep(0xcc34fdusize, "39 82 b0 00 00 00", "trade node value").uses(&[VALUE]),
        Point::keep(0x13d37dbusize, "8b 88 b4 00 00 00", "trade node local value").uses(&[LOCAL_VALUE]),
        Point::keep(0xb56fc9usize, "48 8b 81 f8 00 00 00 48 2b 81 f0 00 00 00 48 c1 f8 05", "trade node incoming").uses(&[INCOMING + 8, INCOMING]),
        // The daily push writes an incoming item: its value, what steering added, where from.
        Point::keep(0xb54c79usize, "89 5a 10 89 4a 14 4c 89 42 18", "trade node incoming item").uses(&[INCOMING_VALUE, INCOMING_FROM]),
        Point::keep(0xb54745usize, "4d 8b bd a8 00 00 00 49 81 c7 98 00 00 00", "trade node links").uses(&[DEFINITION, LINKS]),
        Point::keep(0xb54befusize, "48 6b 44 24 50 78", "trade node link size").uses(&[LINK_SIZE]),
        Point::keep(0xb54bfdusize, "48 8b 40 30 48 63 88 d8 00 00 00", "trade node link's target").uses(&[LINK_TO, INDEX]),
        Point::keep(0x13d29f8usize, "48 8b 53 10", "trade node country's tag").uses(&[ENTRY_TAG]),
        Point::keep(0xb5e5e6usize, "45 8b 86 a8 00 00 00", "trade node merchant's link").uses(&[STEER_LINK]),
        // The share: the country's power, bounded by its demand, plus what it gets, less what it gives.
        Point::keep(
            SHARE,
            "48 89 5c 24 18 49 c1 e8 20 48 8d 5c 24 08 49 0f bf c0 4c 8b da c7 44 24 08 e8 03 00 00 4c 8d 0c 40 49 c1 e1 06 4c 03 49 18 \
             48 63 89 c8 00 00 00 49 63 41 44 4d 63 41 4c 4c 0f af c0 48 b8 cf f7 53 e3 a5 9b c4 20 49 f7 e8 45 33 c0 48 c1 fa 07 48 8b c2 \
             48 c1 e8 3f 48 03 d0 41 8b 41 54 85 d2 44 0f 4f c2 41 2b 41 50 45 39 41 48 45 0f 4c 41 48 44 03 c0",
            "trade share",
        )
        .uses(&[ENTRIES, TOTAL_POWER, MAX_DEMAND, MAX_POWER, GOT, GIVEN, OWN_POWER]),
        // Whether a country collects: the pirates, a merchant collecting, or its home node with none steering.
        Point::keep(
            COLLECTS,
            "48 8b 49 18 48 8b c2 48 c1 e8 20 48 0f bf c0 48 c1 ea 20 4c 8d 04 40 0f bf c2 49 c1 e0 06 49 03 c8 3b 05 b1 4a 7e 01 75 05 \
             48 85 c9 75 2c 32 d2 38 91 ae 00 00 00 74 10 0f b6 81 ac 00 00 00 84 c0 0f 95 c2 84 c0 74 12 48 85 c9 74 10 80 b9 ad 00 00 00 00",
            "trade node collecting",
        )
        .uses(&[ENTRIES, HAS_MERCHANT, MERCHANT_KIND, HOME]),
    ]
}

/// The nodes, by their place: (place, address); place 0 is left out.
pub(super) fn nodes(m: &Memory, ctx: &Ctx) -> GResult<Vec<usize>> {
    let data = m.ptr(ctx.world + NODES)?;
    let count = m.i32(ctx.world + NODE_COUNT)?;
    if !(0..=MOST as i32).contains(&count) {
        return Err("trade node count".to_string().into());
    }
    let mut out = Vec::new();
    for place in 1..count as usize {
        let node = data + place * NODE_SIZE;
        let definition = m.ptr(node + DEFINITION)?;
        if definition == 0 || m.i32(definition + INDEX)? as usize != place {
            return Err(format!("trade node {place} is not in its place").into());
        }
        out.push(node);
    }
    Ok(out)
}

/// Whether the country `tag` collects in the node at `node`.
pub(super) fn collects(m: &Memory, node: usize, tag: Tag) -> GResult<bool> {
    Ok(COLLECTS.call(m, [node, tag.0 as usize])? as u8 != 0)
}

fn text(m: &Memory, string: usize) -> GResult<String> {
    Ok(game_string::key(m, string)?)
}

fn key_of(m: &Memory, definition: usize) -> GResult<String> {
    text(m, definition + KEY)
}

/// The definitions of the nodes `view` sees: every node in the full view; in
/// the player's view those with a province the player has discovered.
fn seen(m: &Memory, ctx: &Ctx, view: View) -> GResult<Option<BTreeSet<usize>>> {
    if view == View::Full {
        return Ok(None);
    }
    let mut seen = BTreeSet::new();
    for address in with_state(|s| s.geography.addresses(m, ctx))? {
        let node = m.ptr(address + province::TRADE_NODE)?;
        if node != 0 && !world::undiscovered(m, super::player::Eyes::acting(view, ctx.player), address)? {
            seen.insert(m.ptr(node + DEFINITION)?);
        }
    }
    Ok(Some(seen))
}

/// Every trade node with its value; in the player's view those with a
/// province the player has discovered.
pub fn list(Session { m, ctx, view, .. }: Session) -> GResult<Vec<NodeValue>> {
    let seen = seen(m, &ctx, view)?;
    let mut out = Vec::new();
    for node in nodes(m, &ctx)? {
        let definition = m.ptr(node + DEFINITION)?;
        if seen.as_ref().is_none_or(|s| s.contains(&definition)) {
            out.push(node);
        }
    }
    out.into_iter()
        .map(|node| {
            let definition = m.ptr(node + DEFINITION)?;
            Ok(NodeValue { node: key_of(m, definition)?, name: game_string::shown(m, definition + NAME)?, value: thousandths(m.i32(node + VALUE)?) })
        })
        .collect()
}

/// The incoming items of a node: (from definition, value).
fn incoming(m: &Memory, node: usize) -> GResult<Vec<(usize, i32)>> {
    let begin = m.ptr(node + INCOMING)?;
    let count = containers::span(begin, m.ptr(node + INCOMING + 8)?, INCOMING_SIZE, MOST)?;
    (0..count).map(|i| begin + i * INCOMING_SIZE).map(|item| Ok((m.ptr(item + INCOMING_FROM)?, m.i32(item + INCOMING_VALUE)?))).collect()
}

/// One trade node, by its key; in the player's view one none of whose
/// provinces the player has discovered is refused.
pub fn node(Session { m, ctx, view, .. }: Session, args::Node { node: key }: args::Node) -> GResult<TradeNode> {
    let all = nodes(m, &ctx)?;
    let mut found = None;
    for &node in &all {
        if key_of(m, m.ptr(node + DEFINITION)?)? == key {
            found = Some(node);
        }
    }
    let Some(node) = found else {
        return super::fail(Status::NotFound, format!("no trade node {key}"));
    };
    let definition = m.ptr(node + DEFINITION)?;
    if seen(m, &ctx, view)?.is_some_and(|s| !s.contains(&definition)) {
        return super::fail(Status::NotFound, format!("the player has not discovered trade node {key}"));
    }
    let by_definition = |d: usize| -> GResult<usize> {
        let place = m.i32(d + INDEX)?;
        all.get((place as usize).wrapping_sub(1)).copied().filter(|_| place > 0).ok_or_else(|| format!("no trade node {place}").into())
    };
    let incoming_flows =
        incoming(m, node)?.into_iter().map(|(from, value)| Ok(Flow { node: key_of(m, from)?, ducats: thousandths(value) })).collect::<GResult<_>>()?;
    let links = links(m, definition)?;
    let mut outgoing = Vec::new();
    for &to in &links {
        let target = by_definition(to)?;
        let ducats = incoming(m, target)?.into_iter().filter(|&(from, _)| from == definition).map(|(_, v)| v).sum();
        outgoing.push(Flow { node: key_of(m, to)?, ducats: thousandths(ducats) });
    }
    let mut countries = countries_in(m, &ctx, node, &links)?;
    countries.sort_by(|a, b| b.power.total_cmp(&a.power));
    Ok(TradeNode {
        node: key,
        name: game_string::shown(m, definition + NAME)?,
        value: thousandths(m.i32(node + VALUE)?),
        local_value: thousandths(m.i32(node + LOCAL_VALUE)?),
        incoming: incoming_flows,
        outgoing,
        countries,
    })
}

/// The definitions a node's links lead to, in their order.
fn links(m: &Memory, definition: usize) -> GResult<Vec<usize>> {
    let begin = m.ptr(definition + LINKS)?;
    let count = containers::span(begin, m.ptr(definition + LINKS + 8)?, LINK_SIZE, MOST)?;
    (0..count).map(|i| Ok(m.ptr(begin + i * LINK_SIZE + LINK_TO)?)).collect()
}

/// The countries with trade power, a merchant or their home in the node, as
/// the view lists them.
fn countries_in(m: &Memory, ctx: &Ctx, node: usize, links: &[usize]) -> GResult<Vec<TradePower>> {
    let entries = m.ptr(node + ENTRIES)?;
    let mut out = Vec::new();
    for tag in world::countries(m, ctx)? {
        if !tag.normal() {
            continue;
        }
        let entry = entries + tag.index() as usize * ENTRY_SIZE;
        if Tag(m.u64(entry + ENTRY_TAG)?).0 != tag.0 {
            continue;
        }
        let fixed = |at: usize| m.i32(entry + at);
        let bound = ((fixed(MAX_DEMAND)? as i64 * fixed(MAX_POWER)? as i64) / 1000).max(0) as i32;
        let power = fixed(OWN_POWER)?.min(bound) + fixed(GOT)? - fixed(GIVEN)?;
        let has_merchant = m.u8(entry + HAS_MERCHANT)? != 0;
        let home = m.u8(entry + HOME)? != 0;
        if power == 0 && !has_merchant && !home {
            continue;
        }
        let merchant = if !has_merchant {
            None
        } else if m.u8(entry + MERCHANT_KIND)? == STEER {
            let link = m.i32(entry + STEER_LINK)?;
            let toward = links.get(link as usize).filter(|_| link >= 0).map(|&d| key_of(m, d)).transpose()?;
            Some(Merchant { action: MerchantAction::Steer, toward })
        } else {
            Some(Merchant { action: MerchantAction::Collect, toward: None })
        };
        let mut share = 0i32;
        let out_at = &mut share as *mut i32 as usize;
        SHARE.call(m, [node, out_at, tag.0 as usize])?;
        let collects = collects(m, node, tag)?;
        out.push(TradePower { country: CountryTag(tag.letters()), power: thousandths(power), share: share as f64 / 10.0, collects, home, merchant });
    }
    Ok(out)
}
