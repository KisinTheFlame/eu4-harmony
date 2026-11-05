//! Merchants: what the trade view offers the player in a trade node, and
//! sending, recalling and steering merchants through the game's own commands
//! (docs/research/envoys.md, "Merchants").
use super::{
    context,
    envoys::{self, quoted, Kind},
    game,
    trade::{self, DEFINITION, ENTRIES, ENTRY_SIZE, HAS_MERCHANT, HOME, KEY, LINKS, LINK_SIZE, LINK_TO, MOST, NAME, STEER_LINK},
    Caller, Ctx, GResult, Tag,
};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    merchants::{Downstream, MerchantOptions, Merchants, Mission, Placed},
};

/// The trade nodes' and their countries' fields are trade's: a node's own
/// index, i32, and the province where a node's definition lies, i32.
const INDEX: usize = 0x120;
const LOCATION: usize = 0xdc;
/// Whether the country may send a merchant to the node, and the rules, the
/// ones that hold too: `(country, location, text, already there counts,
/// needs a free one, _) -> bool`.
const MAY_SEND: usize = 0x3532c0;
/// Whether the country's merchant may steer there, and why not: `(node, tag,
/// reasons) -> bool`.
const MAY_STEER: usize = 0xb5b290;
/// An envoy's ID, i32, and its mission; a mission's trade node, and its
/// phase, i32, [`RETURNING`] once the merchant is recalled.
const ENVOY_ID: usize = 0x44;
const MISSION: usize = 0x10;
const MISSION_NODE: usize = 0x80;
const PHASE: usize = 0x98;
const RETURNING: i32 = 2;
/// A merchant's mission's kind, u8: 0 collect, 1 steer.
const MISSION_KIND: usize = 0x9c;

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(0x2741b4usize, "48 03 46 18", "trade node's countries").uses(&[ENTRIES]),
        Point::keep(0x2741c7usize, "80 b8 ae 00 00 00 00", "merchant present").uses(&[HAS_MERCHANT]),
        Point::keep(0x101d025usize, "80 b9 ad 00 00 00 00", "home node").uses(&[HOME]),
        Point::keep(0x5da5c4usize, "44 89 80 a8 00 00 00", "steered along").uses(&[STEER_LINK]),
        Point::keep(0x101cb42usize, "48 8b 90 a8 00 00 00 48 63 82 dc 00 00 00", "trade node location").uses(&[DEFINITION, LOCATION]),
        Point::keep(0x13fd6c7usize, "8b 8a 20 01 00 00", "trade node index").uses(&[INDEX]),
        Point::keep(0x13fce02usize, "4c 8b 88 98 00 00 00 4c 8b 80 a0 00 00 00", "trade node links").uses(&[LINKS]),
        Point::keep(0x13fce42usize, "48 6b c8 78", "trade node link size").uses(&[LINK_SIZE]),
        Point::keep(0xb5bdf6usize, "48 8b 44 11 30", "link's node").uses(&[LINK_TO]),
        // The trade node's writer in saves writes its definition's key, and
        // the province tooltip its name (docs/research/provinces.md).
        Point::keep(0x9d7595usize, "48 8b 98 a8 00 00 00 48 83 c3 10", "trade node key").uses(&[KEY]),
        Point::keep(0x9f0e20usize, "4c 8b 88 a8 00 00 00 49 83 c1 30", "trade node name").uses(&[NAME]),
        Point::keep(MAY_SEND, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55", "may send a merchant"),
        Point::keep(MAY_STEER, "48 89 5c 24 08 55 48 8b ec 48 81 ec 80 00 00 00", "may steer"),
        Point::keep(0x274834usize, "44 39 52 44", "merchant's ID").uses(&[ENVOY_ID]),
        Point::keep(0x2748d4usize, "49 8b 51 10", "merchant's mission").uses(&[MISSION]),
        Point::keep(0x274257usize, "88 81 9c 00 00 00", "merchant's mission kind").uses(&[MISSION_KIND]),
        // A recall sends the mission home: its node, and the phase it sets.
        Point::keep(0x25ba8eusize, "48 8b 81 80 00 00 00", "mission's trade node").uses(&[MISSION_NODE]),
        Point::keep(0x25baf7usize, "c7 83 98 00 00 00 02 00 00 00", "mission returning").uses(&[PHASE]),
    ]
}

/// A trade node of the world's.
struct Node {
    address: usize,
    definition: usize,
    key: String,
}

fn nodes(m: &Memory, ctx: &Ctx) -> GResult<Vec<Node>> {
    trade::nodes(m, ctx)?
        .into_iter()
        .map(|address| {
            let definition = m.ptr(address + DEFINITION)?;
            Ok(Node { address, definition, key: game_text::to_utf8(&game_string::read(m, definition + KEY)?) })
        })
        .collect()
}

fn node(m: &Memory, ctx: &Ctx, key: &str) -> GResult<Node> {
    nodes(m, ctx)?.into_iter().find(|n| n.key == key).ok_or_else(|| eu4_harmony_checks::envoys::unknown("trade node", key).into())
}

fn name(m: &Memory, definition: usize) -> GResult<String> {
    Ok(game_text::plain(&game_string::read(m, definition + NAME)?))
}

/// The player's entry in the node.
fn entry(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<usize> {
    Ok(m.ptr(node.address + ENTRIES)? + ctx.player.index() as usize * ENTRY_SIZE)
}

/// The definitions of the nodes downstream, in the links' order.
fn downstream(m: &Memory, definition: usize) -> GResult<Vec<usize>> {
    let (begin, end) = (m.ptr(definition + LINKS)?, m.ptr(definition + LINKS + 8)?);
    (0..super::span(begin, end, LINK_SIZE, MOST)?).map(|i| Ok(m.ptr(begin + i * LINK_SIZE + LINK_TO)?)).collect()
}

/// The player's merchant in the node, or on its way there: what it does,
/// whether it is still on its way, and where it steers to.
#[derive(Clone, Copy)]
struct Present {
    mission: Mission,
    arriving: bool,
    steers_to: Option<usize>,
}

/// The player's merchant whose mission is in the node, not recalled, and
/// the mission.
fn sent(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<Option<(usize, usize)>> {
    for envoy in envoys::all(m, ctx.country, Kind::Merchant)? {
        let mission = m.ptr(envoy + MISSION)?;
        if mission != 0 && m.ptr(mission + MISSION_NODE)? == node.address && m.i32(mission + PHASE)? != RETURNING {
            return Ok(Some((envoy, mission)));
        }
    }
    Ok(None)
}

fn present(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<Option<Present>> {
    let entry = entry(m, ctx, node)?;
    let arrived = m.u8(entry + HAS_MERCHANT)? != 0;
    let mission = match sent(m, ctx, node)? {
        Some((_, mission)) => {
            if m.u8(mission + MISSION_KIND)? == 0 {
                Mission::Collect
            } else {
                Mission::Steer
            }
        }
        None if !arrived => return Ok(None),
        None if trade::collects(m, node.address, ctx.player)? => Mission::Collect,
        None => Mission::Steer,
    };
    let steers_to = if arrived && mission == Mission::Steer {
        let along = m.i32(entry + STEER_LINK)?;
        usize::try_from(along).ok().and_then(|i| downstream(m, node.definition).ok()?.get(i).copied())
    } else {
        None
    };
    Ok(Some(Present { mission, arriving: !arrived, steers_to }))
}

/// The player's merchants, and where the placed ones are.
pub fn list(_: Caller) -> GResult<Merchants> {
    let m = game()?;
    let ctx = context(m)?;
    let (merchants, free) = envoys::count(m, ctx.country, Kind::Merchant)?;
    let mut placed = Vec::new();
    for node in nodes(m, &ctx)? {
        if let Some(p) = present(m, &ctx, &node)? {
            let steer_to = p.steers_to.map(|d| game_string::read(m, d + KEY).map(|k| game_text::to_utf8(&k))).transpose()?;
            placed.push(Placed { name: name(m, node.definition)?, node: node.key, mission: p.mission, arriving: p.arriving, steer_to });
        }
    }
    Ok(Merchants { merchants, free, placed })
}

/// What the trade view offers the player in the node `key`.
pub fn options(_: Caller, key: &str) -> GResult<Box<MerchantOptions>> {
    let m = game()?;
    let ctx = context(m)?;
    read(m, &ctx, &node(m, &ctx, key)?).map(Box::new)
}

fn refused(m: &Memory, key: &str) -> GResult<Choice> {
    Ok(Choice::cannot(vec![envoys::localized(m, key)?], Vec::new()))
}

fn read(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<MerchantOptions> {
    let entry = entry(m, ctx, node)?;
    let home = m.u8(entry + HOME)? != 0;
    let now = present(m, ctx, node)?;
    let (collect, steer) = match now.map(|p| p.mission) {
        // The merchant there changes its mission.
        Some(Mission::Collect) => {
            (refused(m, "TRADE_MERCHANT_ALREADY_COLLECTING")?, if home { refused(m, "TRADE_TRANSIT_NODE_CAPITAL")? } else { Choice::can(Vec::new()) })
        }
        Some(Mission::Steer) => (Choice::can(Vec::new()), refused(m, "TRADE_MERCHANT_ALREADY_TRANSFERRING")?),
        None => {
            let location = with_province(m, ctx, m.i32(node.definition + LOCATION)?)?;
            let (may, text) = game_string::written(m, |out| m.call(m.at(MAY_SEND), &[ctx.country, location, out, 0, 1, 0]))?;
            let cost = vec![Amount::new(Resource::Merchant, 1.0)];
            let send = if may as u8 == 0 {
                Choice::cannot(envoys::failing(&text), cost.clone())
            } else if merchant(m, ctx, node)?.is_none() {
                // The check lets a merchant on its way count; the button
                // sends a free one.
                Choice::cannot(vec![envoys::localized(m, "TRADER_FREE_ENVOY")?], cost.clone())
            } else {
                Choice::can(cost.clone())
            };
            let steer = if home && send.can { Choice::cannot(vec![envoys::localized(m, "TRADE_TRANSIT_NODE_CAPITAL")?], cost) } else { send.clone() };
            (send, steer)
        }
    };
    // The recall command takes only a merchant that arrived.
    let recall = if now.is_some_and(|p| !p.arriving) { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    let (may_steer, text) = game_string::written(m, |out| m.call(m.at(MAY_STEER), &[node.address, ctx.player.0 as usize, out]))?;
    let steering_to = now.and_then(|p| p.steers_to);
    let steer_to = downstream(m, node.definition)?
        .into_iter()
        .map(|d| {
            let choice = if may_steer as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(envoys::failing(&text), Vec::new()) };
            Ok(Downstream { node: game_text::to_utf8(&game_string::read(m, d + KEY)?), name: name(m, d)?, current: steering_to == Some(d), choice })
        })
        .collect::<GResult<_>>()?;
    Ok(MerchantOptions {
        node: node.key.clone(),
        name: name(m, node.definition)?,
        mission: now.map(|p| p.mission),
        arriving: now.is_some_and(|p| p.arriving),
        collect,
        steer,
        recall,
        steer_to,
    })
}

fn with_province(m: &Memory, ctx: &Ctx, id: i32) -> GResult<usize> {
    let id = u32::try_from(id).map_err(|_| "a trade node without a province".to_string())?;
    super::with_state(|s| s.geography.province_address(m, ctx, id))
}

/// The merchant the trade view sends for a mission in the node: the one the
/// player has there, whose mission then changes, or else a free one.
fn merchant(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<Option<i32>> {
    if let Some((envoy, _)) = sent(m, ctx, node)? {
        return Ok(Some(m.i32(envoy + ENVOY_ID)?));
    }
    for envoy in envoys::all(m, ctx.country, Kind::Merchant)? {
        if envoys::free(m, envoy)? {
            return Ok(Some(m.i32(envoy + ENVOY_ID)?));
        }
    }
    Ok(None)
}

/// The command the trade view's send button posts: the node by the province
/// where it lies, the merchant by its ID, and the mission, 0 to collect and
/// 1 to steer.
fn send_written(player: Tag, location: i32, merchant: i32, mission: Mission) -> String {
    let kind = match mission {
        Mission::Collect => 0,
        Mission::Steer => 1,
    };
    format!("send_merchant={{ province={location} country={} name={merchant} type={kind} }}", quoted(player))
}

/// Sends a merchant to the node `key` for `mission`, or has the one there
/// take it up.
pub fn send(_: Caller, key: &str, mission: Mission) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let node = node(m, &ctx, key)?;
    let options = read(m, &ctx, &node)?;
    let choice = match mission {
        Mission::Collect => &options.collect,
        Mission::Steer => &options.steer,
    };
    let what = match mission {
        Mission::Collect => "send a merchant to collect there",
        Mission::Steer => "send a merchant to steer trade there",
    };
    if !choice.can {
        return Err(eu4_harmony_checks::envoys::not_allowed(what, &choice.reasons).into());
    }
    let Some(merchant) = merchant(m, &ctx, &node)? else {
        return Err(eu4_harmony_checks::envoys::not_allowed(what, &[]).into());
    };
    envoys::post(&send_written(ctx.player, m.i32(node.definition + LOCATION)?, merchant, mission), "the merchant command")
}

/// Recalls the player's merchant from the node `key`.
pub fn recall(_: Caller, key: &str) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let node = node(m, &ctx, key)?;
    match present(m, &ctx, &node)? {
        None => return Err(eu4_harmony_checks::envoys::none_there("merchant").into()),
        Some(p) if p.arriving => return Err(eu4_harmony_checks::envoys::on_its_way("merchant").into()),
        Some(_) => {}
    }
    let text = format!("cancel_merchant={{ node={} country={} }}", m.i32(node.definition + LOCATION)?, quoted(ctx.player));
    envoys::post(&text, "the recall merchant command")
}

/// Has the player's merchant steering in the node `key` steer to the node
/// `to` downstream.
pub fn steer_to(_: Caller, key: &str, to: &str) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let node = node(m, &ctx, key)?;
    let options = read(m, &ctx, &node)?;
    let Some(along) = options.steer_to.iter().position(|d| d.node == to) else {
        return Err(eu4_harmony_checks::envoys::unknown("trade node downstream", to).into());
    };
    let choice = &options.steer_to[along].choice;
    if !choice.can {
        return Err(eu4_harmony_checks::envoys::not_allowed("steer trade there", &choice.reasons).into());
    }
    let text = format!("steer_command={{ country={} to={along} trade={} }}", quoted(ctx.player), m.i32(node.address + INDEX)?);
    envoys::post(&text, "the steer command")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_send_command_names_the_node_s_province_the_merchant_and_the_mission() {
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        assert_eq!(send_written(ven, 112, 7, Mission::Collect), r#"send_merchant={ province=112 country="VEN" name=7 type=0 }"#);
        assert_eq!(send_written(ven, 112, 7, Mission::Steer), r#"send_merchant={ province=112 country="VEN" name=7 type=1 }"#);
    }
}
