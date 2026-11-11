//! Merchants: what the trade view offers the player in a trade node, and
//! sending, recalling and steering merchants and choosing the node's trading
//! policy through the game's own commands (docs/research/envoys.md,
//! "Merchants").
use super::commands::{write, Block};
use super::{
    action,
    envoys::{self, Kind},
    trade::{self, DEFINITION, ENTRIES, ENTRY_SIZE, HAS_MERCHANT, HOME, KEY, LINKS, LINK_SIZE, LINK_TO, MOST, NAME, STEER_LINK},
    Ctx, GResult, Session, Tag,
};
use crate::containers;
use crate::{
    game_string,
    memory::Memory,
    points::{GameFunction, Point},
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    merchants::{args, Downstream, MerchantOptions, Merchants, Mission, Placed, TradingPolicies, TradingPolicy},
};

/// The trade nodes' and their countries' fields are trade's: a node's own
/// index, i32, and the province where a node's definition lies, i32.
const INDEX: usize = 0x120;
const LOCATION: usize = 0xdc;
/// Whether the country may send a merchant to the node, and the rules, the
/// ones that hold too: `(country, location, text, already there counts,
/// needs a free one, _) -> bool`.
const MAY_SEND: GameFunction<6> = GameFunction::at(0x3532c0);
/// Whether the country's merchant may steer there, and why not: `(node, tag,
/// reasons) -> bool`.
const MAY_STEER: GameFunction<3> = GameFunction::at(0xb5b290);
/// An envoy's ID, i32, and its mission; a mission's trade node, and its
/// phase, i32, [`RETURNING`] once the merchant is recalled.
const ENVOY_ID: usize = 0x44;
const MISSION: usize = 0x10;
const MISSION_NODE: usize = 0x80;
const PHASE: usize = 0x98;
const RETURNING: i32 = 2;
/// A merchant's mission's kind, u8: 0 collect, 1 steer.
const MISSION_KIND: usize = 0x9c;
/// The trading policies: `() -> database`, its policies an array of
/// pointers and their count, i32.
const POLICIES: GameFunction<0> = GameFunction::at(0xd38b60);
const POLICY_LIST: usize = 0x48;
const POLICY_COUNT: usize = 0x54;
/// A policy's key (`std::string`) and its index, i32.
const POLICY_KEY: usize = 0x10;
const POLICY_INDEX: usize = 0x50;
/// A country's entry: its trading policy there, or null.
const POLICY: usize = 0xb0;
/// Whether the country may choose the policy in the node, and why not:
/// `(node, tag, policy, reasons) -> bool`, appending to `reasons`.
const MAY_CHOOSE: GameFunction<4> = GameFunction::at(0xb5c6a0);

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
        Point::keep(POLICIES, "48 89 5c 24 18 57 48 83 ec 20 48 8b 05 07 33 6f 01", "trading policies"),
        // The default policy is the first of the list whose conditions hold.
        Point::keep(0xd397dfusize, "48 8b 71 48 48 63 41 54", "trading policies' list").uses(&[POLICY_LIST, POLICY_COUNT]),
        // The policy's tooltip names it by its key's localisation.
        Point::keep(0xd37c7fusize, "48 8d 59 10", "trading policy key").uses(&[POLICY_KEY]),
        // The policy button's click: the command's policy and node.
        Point::keep(0x13cba84usize, "44 8b a1 d8 00 00 00", "trade node definition index").uses(&[trade::INDEX]),
        Point::keep(0x13cba8fusize, "4c 63 70 50", "trading policy index").uses(&[POLICY_INDEX]),
        Point::keep(0x13cc601usize, "48 8b ab b0 00 00 00", "node's trading policy").uses(&[POLICY]),
        Point::keep(MAY_CHOOSE, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48 89 7c 24 20 41 56", "may choose a trading policy"),
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
            Ok(Node { address, definition, key: game_string::key(m, definition + KEY)? })
        })
        .collect()
}

fn node(m: &Memory, ctx: &Ctx, key: &str) -> GResult<Node> {
    nodes(m, ctx)?.into_iter().find(|n| n.key == key).ok_or_else(|| envoys::checks::unknown("trade node", key))
}

fn name(m: &Memory, definition: usize) -> GResult<String> {
    Ok(game_string::shown(m, definition + NAME)?)
}

/// The player's entry in the node.
fn entry(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<usize> {
    Ok(m.ptr(node.address + ENTRIES)? + ctx.player.index() as usize * ENTRY_SIZE)
}

/// The definitions of the nodes downstream, in the links' order.
fn downstream(m: &Memory, definition: usize) -> GResult<Vec<usize>> {
    let (begin, end) = (m.ptr(definition + LINKS)?, m.ptr(definition + LINKS + 8)?);
    (0..containers::span(begin, end, LINK_SIZE, MOST)?).map(|i| Ok(m.ptr(begin + i * LINK_SIZE + LINK_TO)?)).collect()
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
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Merchants> {
    let (merchants, free) = envoys::count(m, ctx.country, Kind::Merchant)?;
    let mut placed = Vec::new();
    for node in nodes(m, &ctx)? {
        if let Some(p) = present(m, &ctx, &node)? {
            let steer_to = p.steers_to.map(|d| game_string::key(m, d + KEY)).transpose()?;
            placed.push(Placed { name: name(m, node.definition)?, node: node.key, mission: p.mission, arriving: p.arriving, steer_to });
        }
    }
    Ok(Merchants { merchants, free, placed })
}

/// What the trade view offers the player in the node `key`.
pub fn options(Session { m, ctx, .. }: Session, key: &str) -> GResult<Box<MerchantOptions>> {
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
            let (may, text) = game_string::written(m, |out| MAY_SEND.call(m, [ctx.country, location, out, 0, 1, 0]))?;
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
    let (may_steer, text) = game_string::written(m, |out| MAY_STEER.call(m, [node.address, ctx.player.0 as usize, out]))?;
    let steering_to = now.and_then(|p| p.steers_to);
    let steer_to = downstream(m, node.definition)?
        .into_iter()
        .map(|d| {
            let choice = if may_steer as u8 != 0 { Choice::can(Vec::new()) } else { Choice::cannot(envoys::failing(&text), Vec::new()) };
            Ok(Downstream { node: game_string::key(m, d + KEY)?, name: name(m, d)?, current: steering_to == Some(d), choice })
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
    write("send_merchant", Block::new().word("province", location).tag("country", player.letters()).word("name", merchant).word("type", kind))
}

/// Sends a merchant to the node `key` for `mission`, or has the one there
/// take it up.
pub fn send(Session { m, ctx, commands, .. }: Session, key: &str, mission: Mission) -> GResult<()> {
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
        return Err(action::not_allowed(what, &choice.reasons));
    }
    let Some(merchant) = merchant(m, &ctx, &node)? else {
        return Err(action::not_allowed(what, &[]));
    };
    action::post(commands, &send_written(ctx.player, m.i32(node.definition + LOCATION)?, merchant, mission), what)
}

/// Recalls the player's merchant from the node `key`.
pub fn recall(Session { m, ctx, commands, .. }: Session, key: &str) -> GResult<()> {
    let node = node(m, &ctx, key)?;
    match present(m, &ctx, &node)? {
        None => return Err(envoys::checks::none_there("merchant")),
        Some(p) if p.arriving => return Err(envoys::checks::on_its_way("merchant")),
        Some(_) => {}
    }
    let text = write("cancel_merchant", Block::new().word("node", m.i32(node.definition + LOCATION)?).tag("country", ctx.player.letters()));
    action::post(commands, &text, "recall the merchant")
}

/// Has the player's merchant steering in the node `node` steer to the node
/// `to` downstream.
pub fn steer_to(Session { m, ctx, commands, .. }: Session, args::SteerTo { node, to }: args::SteerTo) -> GResult<()> {
    let (node, to) = (self::node(m, &ctx, &node)?, to.as_str());
    let options = read(m, &ctx, &node)?;
    let Some(along) = options.steer_to.iter().position(|d| d.node == to) else {
        return Err(envoys::checks::unknown("trade node downstream", to));
    };
    let choice = &options.steer_to[along].choice;
    if !choice.can {
        return Err(action::not_allowed("steer trade there", &choice.reasons));
    }
    let text = write("steer_command", Block::new().tag("country", ctx.player.letters()).word("to", along).word("trade", m.i32(node.address + INDEX)?));
    action::post(commands, &text, "steer trade there")
}

/// The trading policies of the game's, each its address and content key, in
/// the game's order.
fn all_policies(m: &Memory) -> GResult<Vec<(usize, String)>> {
    let database = POLICIES.call(m, [])?;
    let (list, count) = (m.ptr(database + POLICY_LIST)?, m.i32(database + POLICY_COUNT)?);
    if list == 0 || !(0..=MOST as i32).contains(&count) {
        return Ok(Vec::new());
    }
    (0..count as usize)
        .map(|i| {
            let policy = m.ptr(list + i * 8)?;
            Ok((policy, game_string::key(m, policy + POLICY_KEY)?))
        })
        .collect()
}

/// The trading policies the trade view offers the player in the node `key`.
pub fn policies(Session { m, ctx, .. }: Session, key: &str) -> GResult<TradingPolicies> {
    read_policies(m, &ctx, &node(m, &ctx, key)?)
}

fn read_policies(m: &Memory, ctx: &Ctx, node: &Node) -> GResult<TradingPolicies> {
    let following = m.ptr(entry(m, ctx, node)? + POLICY)?;
    let mut current = None;
    let mut policies = Vec::new();
    for (policy, key) in all_policies(m)? {
        let choice = if policy == following {
            // Choosing it again changes nothing.
            current = Some(key.clone());
            Choice::cannot(Vec::new(), Vec::new())
        } else {
            let (may, text) = game_string::written(m, |out| MAY_CHOOSE.call(m, [node.address, ctx.player.0 as usize, policy, out]))?;
            if may as u8 != 0 {
                Choice::can(Vec::new())
            } else {
                Choice::cannot(super::conditions::refusal(&text), Vec::new())
            }
        };
        policies.push(TradingPolicy { name: envoys::localized(m, &key)?, current: policy == following, policy: key, choice });
    }
    Ok(TradingPolicies { node: node.key.clone(), name: name(m, node.definition)?, current, policies })
}

/// The command the trade view's policy button posts: the node by its
/// definition's index, the policy by its index.
fn policy_written(player: Tag, definition: i32, policy: i32) -> String {
    let members = Block::new().tag("dummy", player.letters()).word("dummy", definition).word("dummy", policy);
    write("select_trading_policy", Block::new().block("members", members))
}

/// Chooses the trading policy `policy` for the player in the node `node`.
pub fn set_policy(Session { m, ctx, commands, .. }: Session, args::SetPolicy { node, policy }: args::SetPolicy) -> GResult<()> {
    let (node, policy) = (self::node(m, &ctx, &node)?, policy.as_str());
    let Some(address) = all_policies(m)?.into_iter().find(|(_, k)| k == policy).map(|(a, _)| a) else {
        return Err(envoys::checks::unknown("trading policy", policy));
    };
    let offered = read_policies(m, &ctx, &node)?.policies.into_iter().find(|p| p.policy == policy);
    match offered {
        Some(p) if p.current => return Err(envoys::checks::policy_followed(policy)),
        Some(p) if !p.choice.can => return Err(action::not_allowed("choose that trading policy there", &p.choice.reasons)),
        _ => {}
    }
    // The command changes the policy only where the player's merchant is.
    if m.u8(entry(m, &ctx, &node)? + HAS_MERCHANT)? == 0 {
        return Err(envoys::checks::none_there("merchant"));
    }
    let text = policy_written(ctx.player, m.i32(node.definition + trade::INDEX)?, m.i32(address + POLICY_INDEX)?);
    action::post(commands, &text, "choose that trading policy there")
}

crate::catalogue::bind! {
    eu4_harmony_remote::merchants: MerchantRequest => MerchantAnswer;
    List => list;
    Options { node } => session options(node.as_str());
    Collect { node } => session send(node.as_str(), Mission::Collect);
    Steer { node } => session send(node.as_str(), Mission::Steer);
    Recall { node } => session recall(node.as_str());
    SteerTo => steer_to;
    Policies { node } => session policies(node.as_str());
    SetPolicy => set_policy;
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

    #[test]
    fn the_policy_command_names_the_country_the_node_and_the_policy_in_its_members_order() {
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        assert_eq!(policy_written(ven, 42, 3), r#"select_trading_policy={ members={ dummy="VEN" dummy=42 dummy=3 } }"#);
    }

    use crate::game::{
        buffers::{self, no, write_inline, yes, Image, Object, Provinces},
        commands::record,
        envoys::POOLS,
        trade::{COLLECTS, NODES, NODE_COUNT, NODE_SIZE},
        Caller, Fail, Status,
    };
    use std::cell::Cell;

    /// A province or a policy of the test's the game's tooltip bars, in a
    /// byte neither uses, read by the game's functions below.
    const BARRED: usize = 8;

    extern "C" fn may_send(_: usize, location: usize, out: usize) -> usize {
        if unsafe { *((location + BARRED) as *const u8) } == 0 {
            return 1;
        }
        write_inline(out, b"\xa3no too far");
        0
    }

    extern "C" fn may_choose(_: usize, _: usize, policy: usize, out: usize) -> usize {
        if unsafe { *((policy + BARRED) as *const u8) } == 0 {
            return 1;
        }
        write_inline(out, "no_navy");
        0
    }

    thread_local! {
        /// The trading policies' database of this thread's test.
        static DATABASE: Cell<usize> = const { Cell::new(0) };
    }

    extern "C" fn policies() -> usize {
        DATABASE.with(Cell::get)
    }

    /// Venice, in a world of the trade nodes `venice` (1, in province 1),
    /// downstream of which is `genoa` (2, in province 2): its merchant 5
    /// steers in venice toward genoa, following the trading policy
    /// `trade_power` there; its merchant 7 is free when `free`; its merchant
    /// 9 is on its way to genoa when `arriving`. The policies are
    /// `trade_power`, `price` and `navy`, the last barred.
    struct Venice {
        _image: Image,
        _provinces: Provinces,
        _objects: Vec<Object>,
        session: Session,
    }

    fn venice(free: bool, arriving: bool) -> Venice {
        let image = Image::new();
        image
            .function(MAY_SEND.rva(), may_send as *const ())
            .function(MAY_STEER.rva(), yes as *const ())
            .function(MAY_CHOOSE.rva(), may_choose as *const ())
            .function(COLLECTS.rva(), no as *const ())
            .function(POLICIES.rva(), policies as *const ())
            .function(game_string::DESTROY_AT.rva(), no as *const ());
        buffers::localisation(&image);
        let ven = Tag(u64::from_le_bytes(*b"VEN\0\0\0\0\0"));
        let provinces = Provinces::new(&image, 3);
        let policy =
            |key: &str, index: i32, barred: bool| Object::new(POLICY_INDEX + 4).string(POLICY_KEY, key).i32(POLICY_INDEX, index).u8(BARRED, barred.into());
        let policies = [policy("trade_power", 0, false), policy("price", 1, false), policy("navy", 2, true)];
        let database = Object::new(POLICY_COUNT + 4)
            .vector(POLICY_LIST, &policies.iter().map(|p| p.at() as u64).collect::<Vec<_>>())
            .i32(POLICY_COUNT, policies.len() as i32);
        DATABASE.with(|d| d.set(database.at()));
        let definition = |key: &str, index: i32| Object::new(LOCATION + 4).string(KEY, key).string(NAME, key).i32(trade::INDEX, index).i32(LOCATION, index);
        let genoa = definition("genoa", 2);
        let link = Object::new(LINK_SIZE).u64(LINK_TO, genoa.at() as u64);
        let venice = definition("venice", 1).u64(LINKS, link.at() as u64).u64(LINKS + 8, (link.at() + LINK_SIZE) as u64);
        let entry = ven.index() as usize * ENTRY_SIZE;
        let entries = |size| Object::new(entry + ENTRY_SIZE).u8(entry + HAS_MERCHANT, size).i32(entry + STEER_LINK, 0);
        let (in_venice, in_genoa) = (entries(1).u64(entry + POLICY, policies[0].at() as u64), entries(0));
        let nodes = Object::new(3 * NODE_SIZE);
        for (place, (definition, entries)) in [(1, (&venice, &in_venice)), (2, (&genoa, &in_genoa))] {
            nodes.set(place * NODE_SIZE + DEFINITION, &(definition.at() as u64).to_le_bytes());
            nodes.set(place * NODE_SIZE + ENTRIES, &(entries.at() as u64).to_le_bytes());
            nodes.set(place * NODE_SIZE + INDEX, &(place as i32).to_le_bytes());
        }
        provinces.world_set(NODES, &(nodes.at() as u64).to_le_bytes()).world_set(NODE_COUNT, &3i32.to_le_bytes());
        let mission = |place: usize| Object::new(MISSION_KIND + 8).u64(MISSION_NODE, (nodes.at() + place * NODE_SIZE) as u64).u8(MISSION_KIND, 1);
        let merchant = |id: i32, away: bool| buffers::envoy(away, ENVOY_ID + 4).i32(ENVOY_ID, id);
        let mut merchants = vec![merchant(5, true).points_to(MISSION, mission(1)), merchant(7, !free)];
        if arriving {
            merchants.push(merchant(9, true).points_to(MISSION, mission(2)));
        }
        let country = Object::country(ven, POOLS + 8).points_to(POOLS, buffers::envoys(vec![(Kind::Merchant, merchants)]));
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), provinces.world(), ven, country.at(), countries.at(), Caller::Remote);
        let mut objects = vec![database, genoa, link, venice, in_venice, in_genoa, nodes, country, countries];
        objects.extend(policies);
        Venice { _image: image, _provinces: provinces, _objects: objects, session }
    }

    fn refused(result: GResult<()>) -> (Status, String) {
        let Err(Fail { status, message }) = result else { panic!("the merchant went") };
        (status, message)
    }

    #[test]
    fn merchants_are_sent_recalled_and_steered_by_the_trade_view_s_commands() {
        let venice = venice(true, false);
        send(venice.session, "genoa", Mission::Collect).unwrap();
        send(venice.session, "genoa", Mission::Steer).unwrap();
        // The merchant steering in venice turns to collecting there.
        send(venice.session, "venice", Mission::Collect).unwrap();
        recall(venice.session, "venice").unwrap();
        steer_to(venice.session, args::SteerTo { node: "venice".into(), to: "genoa".into() }).unwrap();
        set_policy(venice.session, args::SetPolicy { node: "venice".into(), policy: "price".into() }).unwrap();
        assert_eq!(
            record::posted(),
            [
                r#"send_merchant={ province=2 country="VEN" name=7 type=0 }"#,
                r#"send_merchant={ province=2 country="VEN" name=7 type=1 }"#,
                r#"send_merchant={ province=1 country="VEN" name=5 type=0 }"#,
                r#"cancel_merchant={ node=1 country="VEN" }"#,
                r#"steer_command={ country="VEN" to=0 trade=1 }"#,
                r#"select_trading_policy={ members={ dummy="VEN" dummy=1 dummy=1 } }"#
            ]
        );
    }

    #[test]
    fn what_the_trade_view_does_not_offer_is_refused() {
        let rejected = |why: &str| (Status::Rejected, why.to_string());
        let venice = venice(false, true);
        assert_eq!(refused(send(venice.session, "lisbon", Mission::Collect)), (Status::NotFound, "no trade node is named lisbon".into()));
        // Every merchant is away: the button sends a free one.
        assert_eq!(refused(send(venice.session, "venice", Mission::Steer)), rejected("TRADE_MERCHANT_"));
        assert_eq!(refused(recall(venice.session, "genoa")), rejected("the player's merchant is still on its way there"));
        assert_eq!(
            refused(steer_to(venice.session, args::SteerTo { node: "venice".into(), to: "lisbon".into() })),
            (Status::NotFound, "no trade node downstream is named lisbon".into())
        );
        let policy = |node: &str, policy: &str| refused(set_policy(venice.session, args::SetPolicy { node: node.into(), policy: policy.into() }));
        assert_eq!(policy("venice", "trade_power"), rejected("the player already follows trade_power there"));
        assert_eq!(policy("venice", "navy"), rejected("no_navy"));
        assert_eq!(policy("genoa", "price"), rejected("the player has no merchant there"));
        assert_eq!(policy("venice", "plunder"), (Status::NotFound, "no trading policy is named plunder".into()));
        assert!(record::posted().is_empty());
    }

    #[test]
    fn a_merchant_where_the_game_bars_it_or_with_none_free_is_refused() {
        {
            let venice = venice(false, false);
            assert_eq!(refused(send(venice.session, "genoa", Mission::Collect)), (Status::Rejected, "TRADER_FREE_ENV".into()));
            assert_eq!(refused(recall(venice.session, "genoa")), (Status::Rejected, "the player has no merchant there".into()));
        }
        // One world of provinces at a time.
        let venice = venice(true, false);
        venice._provinces.u8(2, BARRED, 1);
        assert_eq!(refused(send(venice.session, "genoa", Mission::Collect)), (Status::Rejected, "too far".into()));
        assert!(record::posted().is_empty());
    }
}
