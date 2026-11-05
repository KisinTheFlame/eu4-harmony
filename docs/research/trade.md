# Trade nodes (for `trade.nodes` and `trade.node`)

Baseline 1.37.5. All addresses are RVAs. Read from eu4.exe on 2026-11-05 with `core/examples/disasm.rs` and a scan of the image for the token table, absolute pointers and instructions; nothing here was tried in the running game. "Inferred" marks what comes from the shape of the code rather than from code read for that purpose. Numbers are i32 thousandths (1.000 is 1000) unless said otherwise. What a province holds of trade (its node pointer at `+0xe8`, center of trade, trade company) is in [provinces.md](provinces.md); the country array and tags are in [countries.md](countries.md).

## Where the keys come from

The save writes the trade nodes under `trade={ node={ ... } ... }`: the trade manager's writer `0xb4c400` (slot `+8` of its vtable `0x1cb0088`) walks the nodes and, before each, writes token `0x15e` (`node`), then calls the node's own writer `0xb5a5b0` (slot `+0x10` of the node vtable `0x1ce9198`). Each field is written by `call 0x1709280`(writer, token) with the token in `edx`, then its value: `0x246bc0` writes an i32 as a fixed-point number, `0x1707c00`(writer, token, int) a plain integer. The token table (an u32 token followed by its key, NUL-ended, in `.data`) names them. The per-country entry's writer is `0xb5e320` (vtable `0x1cafba8`, slot `+8`), an incoming flow's `0xb60260` (vtable `0x1ce9258`, slot `+0x10`); the reader of a node is `0xb59bb0` (node vtable `+0x20`), of an incoming flow `0xb60180`.

## The list of all nodes

- **The trade manager is inside the world** (`[0x233fe78]`, `CGameState`) at `+0x2198`: the world's constructor `0x74c8b0` puts the manager's vtable `0x1cb0088` at `world+0x2198` (`0x74d0d5`–`0x74d0dc`) and an array object (vtable `0x1cb0070`) at `+0x21a8`. Its nodes are **in place in that array, `0x138` bytes each**: data at manager `+0x18` (world `+0x21b0`), count (i32) at manager `+0x24` (world `+0x21bc`); in this array type the capacity is the i32 before the count (manager `+0x20`), as in the country array.
  - The writer: `0xb4c40f` `48 8b 59 18` (data), `0xb4c416` `48 63 41 24` (count), `0xb4c41a` `48 69 f8 38 01 00 00` (stride).
  - From the world, setting a province's node (`0x8bd173`–`0x8bd19b`): `movsxd rax, [rbx+0xd8]` (the definition's index), `0x8bd17a` `48 8b 0d f7 2c a8 01 85 c0 7e 18 3b 81 bc 21 00 00 7d 10 48 69 c0 38 01 00 00 48 03 81 b0 21 00 00` (load the world; index above 0 and below `+0x21bc`; × `0x138`; plus `+0x21b0`), then `mov [rsi+0xe8], rax` into the province. The same chain is at `0x13d5600`, `0x13d57a1` (the interface), `0x25c142` and `0xb54c01` (the push to a downstream node, below).
- **A node's index is its definition's index** (`definition+0xd8`), and the array is indexed by it directly. Index 0 is refused by every lookup (`test eax, eax; jle`), which falls back to a static empty node (`0xb60b80`, the node vtable `0x1ce9198` with the definition `0xb60a80` makes). The definitions are counted from 1: `0xb65dcf` `mov dword ptr [0x233ff9c], 1` resets the counter `0xb66553`–`0xb6655e` hands each new definition (so provinces.md's "from 0" is wrong), after `0xb60a80` has made the definition called `Null` (`0x6c6c754e`), index 0, and pushed it first into the database's vector (`0xb65dd9`–`0xb65e03`). _Inferred_: element 0 of the world's array is the node of that `Null` definition, and nodes 1 to count−1 are the real ones in load order. `0xb4c620`(manager, database), which builds the array from the definitions (`0x74beec`), sizes it to the number of definitions (`0xb4c6c5`–`0xb4c732`); it was not read to its end.
- **Which nodes are real**: the save writer writes a node only when its definition's method `+0x40` answers true (`0xb4c430`–`0xb4c43f`), but the trade node definition's `+0x40` is `0x8f440` (`mov al, 1; ret`), so that test drops nothing. The framework should skip index 0 and take the rest.
- **The definition database** is the pointer at `0x242be48` (set by `0xb682f0`, `0x88` bytes, freed there): a vector of definition pointers, begin at `+0`, end at `+8` (`0xb601dc` `48 8b 35 65 bc 8c 01`, `0xb601ec` `48 8b 16`, `0xb601ef` `48 8b 4e 08`), indexed by the definition's index (`0xb601fe`–`0xb60205`, reading `from=`), and a hash map by key at `+0x38`–`+0x60` (`0xb68f68`–`0xb68f7d`, FNV-1a of the key).

## The definition (`common/tradenodes`, `0xe8` bytes)

- **Key**: `std::string` at `+0x10`; the node writer writes it as `definitions` (token `0x2835`): `0xb5a5dd` `48 8b 99 a8 00 00 00` (node `+0xa8`, the definition) then `add rbx, 0x10`.
- **Localised name**: `std::string` at `+0x30`, set by the constructor `0xb68be0` from the localisation of the key (`0xb68d32`, `0x14b34a0`); the interface reads it at `0x25c175` `4c 8b a0 a8 00 00 00` / `0x25c17c` `49 83 c4 30` (node `+0xa8`, then `+0x30`, for `NAME`).
- **Index**: i32 at `+0xd8` (`0xb68cf9` `89 ab d8 00 00 00`), see above. `+0xdc` is the location province (token `0x2870` location, `0xb68e34`), which the trade node view uses for its map position (`0x13d1e02`–`0x13d1e2c`).
- **Outgoing links**: a vector at `+0x98` (begin), `+0xa0` (end), `+0xa8` (capacity) of `0x78`-byte items (each token `0x2aa8` outgoing appends one, `0xb6930a`–`0xb69340`; size from `imul rax, 0xeeeeeeeeeeeeeeef` after `sar 3`). An item (vtable `0x1c439d0`, copy `0x1bc0b0`): `+0x10` the target's key (`std::string`), **`+0x30` the target's definition** (pointer), `+0x38` an i32 (set to the link's own place in the list when read, `0xb68ef7`), `+0x40` and `+0x58` the path (not read). The push to downstream nodes reads the target so: `0xb54bef` `48 6b 44 24 50 78` (link index × `0x78`), `add rax, [r15]` (the vector's begin, `0xb54745` `4d 8b bd a8 00 00 00` / `0xb5474c` `49 81 c7 98 00 00 00`), `0xb54bfd` `48 8b 40 30`, `0xb54c01` `48 63 88 d8 00 00 00`, then the world's array. The link's place in this list is what a node's `steer_power` and a merchant's steer target count by (below).

## The node (`0x138` bytes)

| Offset | Type | Save key (token) | Meaning, and the instruction that reads it |
|---|---|---|---|
| `+0x18`, `+0x24` | array, `0xc0`-byte entries | the `<TAG>={...}` blocks | The countries in the node, **indexed by the country's index** (below). |
| `+0x28`, `+0x40` | vectors of `0xc`-byte items (a 4-letter tag, then an i32 at `+8`) | `top_power`/`top_power_values`, `top_provinces`/`top_provinces_values` (`0x2b3e`–`0x2b3f`, `0x2b3c`–`0x2b3d`) | The strongest countries by power and by province power (_inferred_ from the keys; `0xb5acd1`–`0xb5b0ce`). |
| `+0x88`, `+0x90` | vector of i32 | `steer_power` (`0x2da7`), one per link | The fraction (thousandths of 1) of the outgoing value sent down each outgoing link, in the definition's link order: `0xb547f2` `49 8b 85 88 00 00 00`, `0xb547f9` `48 63 0c b8` (× value_added_outgoing / 1000 is the base push down link `rdi`). |
| `+0xa8` | pointer | `definitions` | The definition: `0xb5a5dd` `48 8b 99 a8 00 00 00`. |
| `+0xb0` | i32 | `current` (`0x2cad`) | The node's trade value. The trigger trade_node_value (vtable `0x1cf8c90`, evaluate `0xcc31e0`) compares it: `0xcc34fd` `39 82 b0 00 00 00` (`cmp [node+0xb0], value`), the node from the province's `+0xe8` (`0xcc34f2`). Writer `0xb5a610` `41 8b 9e b0 00 00 00`. |
| `+0xb4` | i32 | `local_value` (`0x2da5`) | The value from the node's own provinces; the view's `local_value` line: `0x13d37db` `8b 88 b4 00 00 00`. Writer `0xb5a645`. |
| `+0xb8` | i32 | `retention` (`0x2da6`) | Written always (`0xb5a6e4` `41 8b 9e b8 00 00 00`); _inferred_ from the key: the part kept in the node. Not checked against a reader. |
| `+0xbc` | i32 | `outgoing` (`0x2aa8`) | The value leaving the node; the view's `outgoing_value`: `0x13d3ddc` `8b 88 bc 00 00 00`. The push takes it off `+0xb0` when any `steer_power` is not zero (`0xb54699`–`0xb546d0`). |
| `+0xc0` | i32 | `value_added_outgoing` (`0x2b20`) | The outgoing value with what steering adds, which the push splits over the links: `0xb54738` `49 63 b5 c0 00 00 00`. |
| `+0xc4` | i32 | `pirate_hunt` (`0x313c`) | Not read further. |
| `+0xc8` | i32 | `total` (`0x2a15`) | The total trade power in the node; a country's share divides by it: `0xb573c9` `48 63 89 c8 00 00 00`. |
| `+0xcc` | i32 | `p_pow` (`0x30e6`) | Not read further. |
| `+0xd0`, `+0xd4`, `+0xd8` | i32 | `collector_power_including_pirates` (`0x3e6a`), `collector_power` (`0x2afb`), `max` (`0x2a4`) | Not read further. |
| `+0xdc`, `+0xe0`, `+0xe4` | i32 | `pull_power` (`0x2da8`), `retain_power` (`0x2da9`), `highest_power` (`0x2daa`) | The view's `TRADE_PULL` and `TRADE_RETENTION` lines use the pull and retain powers (_inferred_, not traced). |
| `+0xe8` | i32 | `total_privateer_power` (`0x372d`) | |
| `+0xa0`, `+0xa4` | i32 (plain) | `num_collectors` (`0x37b0`), `num_collectors_including_pirates` (`0x3e6b`) | Written as `%d` (`0xb5a77f`–`0xb5a81a`). |
| `+0xf0`, `+0xf8`, `+0x100` | vector of `0x20`-byte items | `incoming` (`0x2aa7`) | The incoming flows (below): `0xb56fc9` `48 8b 81 f8 00 00 00`, `0xb56fd0` `48 2b 81 f0 00 00 00`. |
| `+0x108` | | `trade_goods_size` (`0x2b06`) | Per-goods amounts, written by `0xb5aca4`–`0xb5acb0`; not read further. |
| `+0x130` | byte | `trade_company_region` (`0x2f92`) | |
| `+0x134` | date | `most_recent_treasure_ship_passage` (`0x3925`) | |

- **The view's three values** (the trade node view's update, `0x13d1d90`, with the node at its `+0xf8`): `incoming_value` is `0xb56fc0`(node, &out), the sum of every incoming flow's value (`+0x10`, `0xb56ff1` `8b 4c 01 10`); `local_value` is `+0xb4`; `total_value` is incoming + `+0xb4` − `+0xbc` (`0x13d3d04`–`0x13d3d2a`: `44 8b 91 b4 00 00 00`, the call, `2b 81 bc 00 00 00`). _Inferred_: that equals `+0xb0` after the daily push, since the push takes `+0xbc` off `+0xb0` and adds each flow to the downstream node's `+0xb0` (`0xb54c3c`); `+0xb0` is what the trigger compares, so `trade.nodes` can read it as the node's value.

## Incoming flows (`0x20` bytes, vtable `0x1ce9258`)

Made by the push (`0xb54c42`–`0xb54c99`, appended to the downstream node's `+0xf0` vector) and written by `0xb60260`:

- `+0x10` i32 `value` (token `0x3ec`): the value arriving (`0xb602a7` `8b 5f 10`; `0xb54c79` `89 5a 10`).
- `+0x14` i32 `add` (`0x30e5`): what steering added on top of the base push (`0xb60272` `8b 59 14`; `0xb54c7c` `89 4a 14`, total − base).
- `+0x18` pointer `from` (`0x2992`): the **definition** of the node it comes from, written as its index (`0xb602d6` `48 8b 47 18`, `0xb602e2` `8b 98 d8 00 00 00`), read back through the database (`0xb601d0`–`0xb60209`). The push sets it to its own node's `+0xa8` (`0xb54c7f` `4c 89 42 18`).
- **Outgoing flows** are therefore not kept by the sending node: link _i_ of node N goes to node T = the node of N's definition's link _i_ `+0x30`, and how much is the `value` of the incoming flow in T whose `from` is N's definition. The push computes it as `steer_power[i]` × `+0xc0` / 1000 × (1000 + the steering bonus) / 1000 (`0xb547f2`–`0xb54818`, `0xb54bc2`–`0xb54bec`).

## The countries in a node (entries of `0xc0` bytes)

- **Indexing**: entry = node `+0x18` + the country's index (bits 32–47 of the tag, sign-extended) × `0xc0`: `0xb58e00` `48 8b 49 18`, `0xb58e13` `4c 8d 04 40`, `0xb58e1a` `49 c1 e0 06` (`lea r8, [rax+rax*2]; shl r8, 6`), also the trigger has_trader at `0xcb7b65`–`0xcb7b7b`. The count at `+0x24` is _inferred_ to be the number of countries; every country has an entry.
- **Shown or not**: the entry's tag at `+0x10` (8 bytes, valid byte `+0x17`): the view skips an entry whose tag is not valid (`0x13d28f0` `80 7b 17 00`) or whose fields are all zero (`0x13d28fa`–`0x13d29eb`, the same test the save writer makes at `0xb5aa4d`–`0xb5ab3e`), and passes `+0x10` on (`0x13d29f8` `48 8b 53 10`).

| Offset | Type | Save key (token) | Meaning |
|---|---|---|---|
| `+0x10` | tag | (the block's name) | The country. |
| `+0x18` | i32 (plain) | `light_ship` (`0x2785`) | Light ships protecting trade (_inferred_ from the key). |
| `+0x1c` | i32 | `ship_power` (`0x2b0b`) | `0xb5e488` `41 8b 5e 1c` |
| `+0x20`, `+0x24` | i32 | `privateer_mission` (`0x2f70`), `privateer_money` (`0x3732`) | |
| `+0x28` | i32 | `province_power` (`0x2dac`) | `0xb5e456` `41 8b 5e 28` |
| `+0x2c` | i32 | `power_fraction` (`0x2dad`) | **The share of the node's power**, thousandths of 1: the tooltip `0xb58e90` multiplies it by `+0xb0` for the country's part of the value and shows it ×100 as `TRADE_NODE_POWER_SHARE` (`0xb58ee0` `48 63 4e 2c`, `0xb58fd5`). |
| `+0x30` | i32 | `power_fraction_push` (`0x2dae`) | `0xb5e550` `41 8b 5e 30` |
| `+0x34` | i32 | `money` (`0x2daf`) | The income from the node (_inferred_); the getter `0xb58e60` returns it (`0xb58e76` `41 8b 40 34`). |
| `+0x38` | i32 | `total` (`0x2a15`) | `0xb5e5b4` `41 8b 5e 38` |
| `+0x3c`, `+0x40` | i32 | `potential` (`0x2cb8`), `prev` (`0x2d99`) | |
| `+0x44` | i32 | `max_demand` (`0x2846`) | |
| `+0x48` | i32 | `val` (`0x30e2`) | **The country's trade power in the node** before transfers: `0xb5e35c` `41 8b 5e 48`, `0xb57408` `45 39 41 48`. |
| `+0x4c` | i32 | `max_pow` (`0x30e1`) | |
| `+0x50`, `+0x54` | i32 | `t_out` (`0x30e0`), `t_in` (`0x30df`) | Trade power transferred away and received (_inferred_ from the keys and from `0xb573fa`–`0xb57404`). |
| `+0x58` | i32 | `add` (`0x30e5`) | The value this country's steering merchant adds to the push (`0xb54b9d` `mov [r14+0x58], r12d`). |
| `+0x5c` | i32 | `already_sent` (`0x3731`) | |
| `+0x60`, `+0x70` / `+0x78`, `+0x88` | lists of tags with counts | `t_to` (`0x30e3`), `t_from` (`0x30e4`) | Whom it transfers power to and receives from (written only when the count is not 0, `0xb5e761`, `0xb5e966`). |
| `+0x90` | list | `modifier` (`0x2968`) | |
| `+0xa8` | i32 (plain) | `steer_power` (`0x2da7`) | **The steering merchant's target**: the place, in this node's definition's outgoing links, of the link it steers down (`0xb5e5e6` `45 8b 86 a8 00 00 00`; the push gathers the merchants with `[entry+0xa8]` equal to the link index, `0xb548ad`). |
| `+0xac` | byte | `type` (`0xe1`, written when not 0) | **The merchant's mission**: 0 collect, 1 steer (`0xb58e38` `0f b6 81 ac 00 00 00`; `0xb54a90` `41 80 be ac 00 00 00 01`). |
| `+0xad` | byte | `has_capital` (`0x2c94`) | The country's home node (_inferred_: the node it collects in without a merchant): `0xb58e4d` `80 b9 ad 00 00 00 00`. |
| `+0xae` | byte | `has_trader` (`0x2c09`) | **A merchant of the country is in the node**: `0xb58e30` `38 91 ae 00 00 00`; the trigger has_trader (vtable `0x1cffd80`, evaluate `0xcb7830`) tests it at `0xcb7b90` `80 ba ae 00 00 00 00`, and has_merchant (vtable `0x1cf8a90`, evaluate `0xc46b20`) at `0xc46e84`. |
| `+0xaf` | byte | `has_subject` (`0x2f23`) | |
| `+0xb0` | pointer | `trading_policy` (`0x3747`) | Its key a `std::string` at `+0x10` of the policy (`0xb5eba5`–`0xb5ebd5`). |
| `+0xb8` | date | `trading_policy_date` (`0x3748`) | |

- **Collects here**: `0xb58e00`(node, tag) answers whether the country collects in the node: true when the country is the pirates (its index equals the i32 at `0x233d8d8`, set at `0x2821e3` to the index of the tag `PIR`, `0xb58e21` `3b 05 b1 4a 7e 01`); or it has a merchant (`+0xae`) with mission 0 (`+0xac`); or it has no merchant steering there and `+0xad` is set. Both the triggers above treat the pirates as always having a merchant.
- **Trade power and share as the view shows them** (`0xb573a0`(node, &out, tag) for the share; the view's list at `0x13d2a23`–`0x13d2a82` inline for the power): power = min(`val` `+0x48`, max(0, `max_demand` `+0x44` × `max_pow` `+0x4c` / 1000)) + `t_in` `+0x54` − `t_out` `+0x50` (`0xb573d0` `49 63 41 44`, `0xb573d4` `4d 63 41 4c`, `0xb573fa` `41 8b 41 54`, `0xb57404` `41 2b 41 50`, `0xb57408` `45 39 41 48`); share = power × 1000 / the node's `total` `+0xc8`, clamped to 0..1000, 0 when the total is 0 (`0xb573c9`–`0xb5746d`). The view shows the share ×100 as `TRADE_POWER_TAKE_PART` (`0x13d2f42`–`0x13d2f95`). `power_fraction` (`+0x2c`) is the share as the daily update last stored it; whether the two always agree was not checked.

## The country's home node

- **The trade port** is a province ID (i32) at `country+0x14d0`, written as `trade_port` (token `0x2f73`, `0x2dd074`–`0x2dd083` in the country writer). The trigger same_home_trade_node_as (vtable `0x1d00880`, evaluate `0xbff530`) compares the trade nodes (`+0xe8`) of both countries' trade ports: `0xbff598` `48 63 81 d0 14 00 00`, then `0xbff59f` `imul rdx, rax, 0x2e10` into the world's provinces and `0xbff5c4` `cmp [rdx+r9+0xe8], rax`. So the home node is `provinces[country+0x14d0]+0xe8`.

## The province's trade power

- **An i32 at province `+0x890`**, written as `trade_power` (token `0x2da0`) when not 1000 (`0x9d8afc`–`0x9d8b28`); the trigger province_trade_power (vtable `0x1d03648`, evaluate `0xbce550`, tooltips `PROVINCE_TRADE_POWER_MORE_THAN`/`LESS_THAN`) compares it with its value: `0xbce85f` `39 88 90 08 00 00` (`cmp [province+0x890], value`), the province from the map through `0x9aa80`. _Inferred_: the value as last computed by the daily update.

## Not found or not looked at

- The meaning of `retention` (`+0xb8`), `p_pow`, `max`, the collector, pull, retain and highest powers, and `trade_goods_size`, beyond their keys; no reader of them was traced.
- Whether the world's array really holds the `Null` node at index 0 (the build `0xb4c620` was read only to its sizing), and the count of a node's country entries.
- Merchants on their way (`MERCHANT_TRAVELLING_TO_NODE`, the country's merchant list): where a country keeps its merchants and their arrival dates was not looked at. `has_trader` is set only for one that has arrived (_inferred_).
- The trade_share trigger (token `0x2f74`) and the node list window (`trade_node_list_window`) were not read; the node fields above come from the save writer, the triggers, the daily push `0xb54670` and the trade node view `0x13d1d90`.
- Other `type` values than 0 and 1; none is written by the code read.
