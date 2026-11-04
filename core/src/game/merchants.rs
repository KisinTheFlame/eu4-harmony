//! Merchants: what the trade view offers the player in a trade node, and
//! sending, recalling and steering merchants through the game's own commands
//! (docs/research/envoys.md, "Merchants").
use super::{
    context,
    envoys::{self, quoted, Kind},
    game, Caller, Ctx, GResult, Tag,
};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    merchants::{Downstream, MerchantOptions, Merchants, Mission, Placed},
};

/// The world's trade nodes: an array of [`NODE_SIZE`]-byte nodes and their
/// count, i32.
const NODES: usize = 0x21b0;
const NODE_COUNT: usize = 0x21bc;
const NODE_SIZE: usize = 0x138;
/// A node's countries: an array of [`ENTRY_SIZE`]-byte entries, by the
/// country's index.
const COUNTRIES: usize = 0x18;
const ENTRY_SIZE: usize = 0xc0;
/// A node's definition, and its own index, i32.
const DEFINITION: usize = 0xa8;
const INDEX: usize = 0x120;
/// A country's entry: the link its merchant steers along (i32), whether it
/// is the country's home node, whether the country has a merchant there.
const STEERS_ALONG: usize = 0xa8;
const HOME: usize = 0xad;
const PRESENT: usize = 0xae;
/// A definition's key and localised name (`std::string`), the links
/// downstream (a vector of [`LINK_SIZE`]-byte links, each the definition it
/// leads to at [`LINK_TO`]), and the province where the node lies, i32.
const KEY: usize = 0x10;
const NAME: usize = 0x30;
const LINKS: usize = 0x98;
const LINK_SIZE: usize = 0x78;
const LINK_TO: usize = 0x30;
const LOCATION: usize = 0xdc;
/// The most nodes and links read.
const MOST: usize = 1_000;
/// Whether the country collects in the node: `(node, tag) -> bool`.
const COLLECTS: usize = 0xb58e00;
/// Whether the country may send a merchant to the node, and the rules, the
/// ones that hold too: `(country, location, text, already there counts,
/// needs a free one, _) -> bool`.
const MAY_SEND: usize = 0x3532c0;
/// Whether the country's merchant may steer there, and why not: `(node, tag,
/// reasons) -> bool`.
const MAY_STEER: usize = 0xb5b290;
/// An envoy's ID, i32, and its mission; a mission's destination province.
const ENVOY_ID: usize = 0x44;
const MISSION: usize = 0x10;
const DESTINATION: usize = 0x38;
/// A province's trade node.
const TRADE_NODE: usize = 0xe8;

pub fn points() -> Vec<Point> {
    vec![
        // The steer command's execute finds the node by its index.
        Point::keep(0x5da508usize, "3b 81 bc 21 00 00", "trade node count").uses(&[NODE_COUNT]),
        Point::keep(0x5da510usize, "4c 69 d0 38 01 00 00 4c 03 91 b0 21 00 00", "trade nodes").uses(&[NODE_SIZE, NODES]),
        Point::keep(0x2741b4usize, "48 03 46 18", "trade node's countries").uses(&[COUNTRIES]),
        Point::keep(0x2741c7usize, "80 b8 ae 00 00 00 00", "merchant present").uses(&[PRESENT]),
        Point::keep(0x101d025usize, "80 b9 ad 00 00 00 00", "home node").uses(&[HOME]),
        Point::keep(0x5da5c4usize, "44 89 80 a8 00 00 00", "steered along").uses(&[STEERS_ALONG]),
        Point::keep(0x101cb42usize, "48 8b 90 a8 00 00 00 48 63 82 dc 00 00 00", "trade node location").uses(&[DEFINITION, LOCATION]),
        Point::keep(0x13fd6c7usize, "8b 8a 20 01 00 00", "trade node index").uses(&[INDEX]),
        Point::keep(0x13fce02usize, "4c 8b 88 98 00 00 00 4c 8b 80 a0 00 00 00", "trade node links").uses(&[LINKS]),
        Point::keep(0x13fce42usize, "48 6b c8 78", "trade node link size").uses(&[LINK_SIZE]),
        Point::keep(0xb5bdf6usize, "48 8b 44 11 30", "link's node").uses(&[LINK_TO]),
        // The trade node's writer in saves writes its definition's key, and
        // the province tooltip its name (docs/research/provinces.md).
        Point::keep(0x9d7595usize, "48 8b 98 a8 00 00 00 48 83 c3 10", "trade node key").uses(&[KEY]),
        Point::keep(0x9f0e20usize, "4c 8b 88 a8 00 00 00 49 83 c1 30", "trade node name").uses(&[NAME]),
        Point::keep(COLLECTS, "48 8b 49 18", "collects in the node"),
        Point::keep(MAY_SEND, "48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55", "may send a merchant"),
        Point::keep(MAY_STEER, "48 89 5c 24 08 55 48 8b ec 48 81 ec 80 00 00 00", "may steer"),
        Point::keep(0x274834usize, "44 39 52 44", "merchant's ID").uses(&[ENVOY_ID]),
        Point::keep(0x2748d4usize, "49 8b 51 10", "merchant's mission").uses(&[MISSION]),
        Point::keep(0x2748ddusize, "48 8b 42 38", "mission's destination").uses(&[DESTINATION]),
        Point::keep(0x274196usize, "49 8b b3 e8 00 00 00", "province's trade node").uses(&[TRADE_NODE]),
    ]
}

/// A trade node of the world's.
struct Node {
    address: usize,
    definition: usize,
    key: String,
}

fn nodes(m: &Memory, ctx: &Ctx) -> GResult<Vec<Node>> {
    let (array, count) = (m.ptr(ctx.world + NODES)?, m.i32(ctx.world + NODE_COUNT)?);
    if array == 0 || !(0..=MOST as i32).contains(&count) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for i in 0..count as usize {
        let address = array + i * NODE_SIZE;
        let definition = m.ptr(address + DEFINITION)?;
        if definition == 0 {
            continue;
        }
        let key = game_text::to_utf8(&game_string::read(m, definition + KEY)?);
        if !key.is_empty() && !out.iter().any(|n: &Node| n.key == key) {
            out.push(Node { address, definition, key });
        }
    }
    Ok(out)
}

fn node(m: &Memory, ctx: &Ctx, key: &str) -> GResult<Node> {
    nodes(m, ctx)?.into_iter().find(|n| n.key == key).ok_or_else(|| eu4_harmony_checks::envoys::unknown("trade node", key).into())
}

fn name(m: &Memory, definition: usize) -> GResult<String> {
    Ok(game_text::plain(&game_string::read(m, definition + NAME)?))
}

/// The player's entry in the node.
fn entry(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<usize> {
    Ok(m.ptr(node.address + COUNTRIES)? + ctx.player.index() as usize * ENTRY_SIZE)
}

/// The definitions of the nodes downstream, in the links' order.
fn downstream(m: &Memory, definition: usize) -> GResult<Vec<usize>> {
    let (begin, end) = (m.ptr(definition + LINKS)?, m.ptr(definition + LINKS + 8)?);
    (0..super::span(begin, end, LINK_SIZE, MOST)?).map(|i| Ok(m.ptr(begin + i * LINK_SIZE + LINK_TO)?)).collect()
}

/// What the player's merchant in the node does, and where it steers to.
fn mission(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<Option<(Mission, Option<usize>)>> {
    let entry = entry(m, ctx, node)?;
    if m.u8(entry + PRESENT)? == 0 {
        return Ok(None);
    }
    if m.call(m.at(COLLECTS), &[node.address, ctx.player.0 as usize])? as u8 != 0 {
        return Ok(Some((Mission::Collect, None)));
    }
    let along = m.i32(entry + STEERS_ALONG)?;
    let to = usize::try_from(along).ok().and_then(|i| downstream(m, node.definition).ok()?.get(i).copied());
    Ok(Some((Mission::Steer, to)))
}

/// The player's merchants, and where the placed ones are.
pub fn list(_: Caller) -> GResult<Merchants> {
    let m = game()?;
    let ctx = context(m)?;
    let (merchants, free) = envoys::count(m, ctx.country, Kind::Merchant)?;
    let mut placed = Vec::new();
    for node in nodes(m, &ctx)? {
        if let Some((mission, to)) = mission(m, &ctx, &node)? {
            let steer_to = to.map(|d| game_string::read(m, d + KEY).map(|k| game_text::to_utf8(&k))).transpose()?;
            placed.push(Placed { name: name(m, node.definition)?, node: node.key, mission, steer_to });
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
    let now = mission(m, ctx, node)?;
    let (collect, steer) = match now {
        // The merchant there changes its mission.
        Some((Mission::Collect, _)) => {
            (refused(m, "TRADE_MERCHANT_ALREADY_COLLECTING")?, if home { refused(m, "TRADE_TRANSIT_NODE_CAPITAL")? } else { Choice::can(Vec::new()) })
        }
        Some((Mission::Steer, _)) => (Choice::can(Vec::new()), refused(m, "TRADE_MERCHANT_ALREADY_TRANSFERRING")?),
        None => {
            let location = with_province(m, ctx, m.i32(node.definition + LOCATION)?)?;
            let (may, text) = game_string::written(m, |out| m.call(m.at(MAY_SEND), &[ctx.country, location, out, 0, 1, 0]))?;
            let cost = vec![Amount::new(Resource::Merchant, 1.0)];
            let send = if may as u8 != 0 { Choice::can(cost.clone()) } else { Choice::cannot(envoys::failing(&text), cost.clone()) };
            let steer = if home && send.can { Choice::cannot(vec![envoys::localized(m, "TRADE_TRANSIT_NODE_CAPITAL")?], cost) } else { send.clone() };
            (send, steer)
        }
    };
    let recall = if now.is_some() { Choice::can(Vec::new()) } else { Choice::cannot(Vec::new(), Vec::new()) };
    let (may_steer, text) = game_string::written(m, |out| m.call(m.at(MAY_STEER), &[node.address, ctx.player.0 as usize, out]))?;
    let steering_to = now.and_then(|(_, to)| to);
    let steer_to = downstream(m, node.definition)?
        .into_iter()
        .map(|d| {
            let choice = if may_steer as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(envoys::failing(&text), Vec::new()) };
            Ok(Downstream { node: game_text::to_utf8(&game_string::read(m, d + KEY)?), name: name(m, d)?, current: steering_to == Some(d), choice })
        })
        .collect::<GResult<_>>()?;
    Ok(MerchantOptions { node: node.key.clone(), name: name(m, node.definition)?, mission: now.map(|(mission, _)| mission), collect, steer, recall, steer_to })
}

fn with_province(m: &Memory, ctx: &Ctx, id: i32) -> GResult<usize> {
    let id = u32::try_from(id).map_err(|_| "a trade node without a province".to_string())?;
    super::with_state(|s| s.geography.province_address(m, ctx, id))
}

/// The merchant the trade view sends for a mission in the node: the one the
/// player has there, whose mission then changes, or else a free one.
fn merchant(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<Option<i32>> {
    let merchants = envoys::all(m, ctx.country, Kind::Merchant)?;
    for &envoy in &merchants {
        let mission = m.ptr(envoy + MISSION)?;
        if mission != 0 {
            let destination = m.ptr(mission + DESTINATION)?;
            if destination != 0 && m.ptr(destination + TRADE_NODE)? == node.address {
                return Ok(Some(m.i32(envoy + ENVOY_ID)?));
            }
        }
    }
    for &envoy in &merchants {
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
    if mission(m, &ctx, &node)?.is_none() {
        return Err(eu4_harmony_checks::envoys::none_there("merchant").into());
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
