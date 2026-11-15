# Reformation centers: ownership, targeting and removal

A religion manager owns its centers. A center names a source province, a religion and an optional target; the source and target provinces keep reciprocal references. Daily work advances conversion, and target selection, center creation and removal must maintain those references. Creating a center in a province already being converted clears the old center's target but leaves the province's converting-center reference intact. The paths read below establish that omission; they do not establish the history of the separately reported crash.

## What the icon update reads

The function at `0x132f1c0` was read through its end. It places and projects a map icon through the existing common paths (`0x132e300` and `0x132e210`), then updates elements named `reformation_conversion_mapicon`, `reformation_conversion_progress`, `reformation_conversion_progressframe`, `religious_conversion_center_frames` and `reformation_conversion_icon`.

It reads both province pointers `+0x840` and `+0x838`. The former identifies the center located in this province; the latter identifies the center currently converting this province. For each nonzero pointer it interprets the center's `+0x10` as its type definition, then reads a string at that definition's `+0x90`. It also uses the center's religion at `+0x28` to obtain the icon frame through a field at `+0x570`. The `+0x838` branch uses the province's progress at `+0x830`. These field meanings follow from the creation, daily update and persistence paths below.

The caller at `0x132b1b0`, also read through its end, checks its display arguments and holder state. A nonzero province `+0x838`, or a bit in province `+0x959`, permits the icon update. It obtains the map position through the province's `+0x28` and passes the holder's icon to the callee. Its call returns at `0x132b23e`. This caller does not support an explanation in which the immediate argument was an unrelated non-province object.

## The real center and its owner

The real center has vtable `0x1c645c0` and is allocated with size 72 bytes. Its field reader at `0x44a7b0` and writer at `0x44aae0`, both read through their ends, identify the fields using the token table:

| Center offset | Saved key | Meaning |
| --- | --- | --- |
| `+0x10` | `type` | Center type definition |
| `+0x18` | `province_id` | Province where this center is located |
| `+0x20` | `any_target_province` | Province currently being converted, or null |
| `+0x28` | `religion` | Religion being spread |
| `+0x30` | `max_speed` | Stored conversion speed |
| `+0x34` | `missionary_progress` | Stored conversion progress |
| `+0x38` | `owner` | Saved country tag |

The religion manager's per-religion record owns its centers in an array: data at `+0x20`, capacity at `+0x28`, count at `+0x2c`. The reader at `0xdad740` allocates a center for each `reformation_center` block, reads it, and retains it only if its source province, religion and type are nonnull. The writer at `0xdadad0` writes the array under that key. Both functions were read through their ends.

The post-read fixup at `0x44ac30` writes the source province's `+0x840` and sets bit 0 at `+0x959`. For a target it calls `0xa16a50`, which writes the supplied progress to province `+0x830` and the center to `+0x838`, marking the map state stale when needed. This setter does not change the center's target field or detach another center. Both short functions were read through their returns.

The daily conversion update at `0x448000`, read through its return and cold exits, keeps the target's progress and `+0x838` up to date. When the target already has the center's religion, it clears that target's progress and pointer before choosing another. Target selection at `0x449990` builds candidates and delegates their filtering and ranking to `0xa81770`; both were read through their ends. The latter rejects a candidate with a nonzero province `+0x838` at `a818f7`, so a forgotten nonzero pointer does not become available for selection merely because its center has moved on.

## A creation path leaves the old target's reference behind

Two center-creation functions have the same asymmetric detach:

- `0xdae300`, the per-religion manager's creation operation, allocates the new center with no target and records the supplied province as its source. At `dae383` it loads that province's `+0x838`; if nonnull, at `dae38f` it sets the **old center's** target `+0x20` to null. It then sets the source province's center flag and `+0x840` to the new center and adds the new center to the owning array.
- `0xd37940`, the `propagate_religion_policy` path, performs the same sequence at `d37b0b` / `d37b17` before setting the province's `+0x840` and inserting the new center.

Both were read through their ends. Neither clears the province's `+0x838` or progress when it discards the old center's target. In the manager path, `rcx` is explicitly zero before the write at `dae38f`; in the policy path, `rdx` is explicitly zero before `d37b17`.

The critical manager-path instructions are:

```text
dae383  mov rax, [rbp+838h]  ; old center converting the new source province
dae38a  test rax, rax
dae38d  je ...
dae38f  mov [rax+20h], rcx   ; rcx = 0: old center forgets its target
...
dae3cd  or byte ptr [rax+959h], 1
dae3d4  mov [rax+840h], rdi  ; source province points to the new center
```

This is not solely a field-reader artifact. An effect implementation at `0x5ff320` calls the manager operation at `5ffa8d`; its province branch tests the source-center flag and the religion's check, but does not require province `+0x838` to be null. A console handler at `0xe5fbf0` resolves a province id and center type and also calls it at `e5fd30` without that check. Both were read through their ends. The installed game's religion definitions use `change_religion` followed by `add_reform_center`, and its Bohemian events use `remove_reform_center`. The isolated reproduction used those native effects. The wider dispatch and all religion-check callees were not recovered, so this does not enumerate every allowed game scenario.

A further caller at `0xa3bef0` changes the province's religion first, at `a3bf81`, then calls center creation by a tail jump at `a3bf9f`. Changing religion is not an unconditional guard: the concluding logic of `0xa15ff0` clears an incomplete conversion only when the new province religion equals the old center's religion. A change to a different religion can retain the old pointer. The tail at `0xa16720` refreshes scripted state through virtual callbacks; those callbacks were not all resolved and may have scenario-specific effects. This caller was read through its return/tail exits, and the tail through its return.

## Normal removal cannot find the forgotten province

The cleanup at `0x449f10`, read through its return and cold exits, clears the source province's center flag and `+0x840`. It then loads the center's **current** target at `449f76`. If null, `449f7d` jumps over target cleanup. If nonnull, it clears only that target's progress and `+0x838`. It does not search all provinces for references to this center.

```text
449f76  mov rdx, [rsi+20h]
449f7a  test rdx, rdx
449f7d  je 449feeh
449f7f  mov [rdx+830h], r12d  ; r12 = 0
449f86  mov [rdx+838h], r12
```

The normal manager removal by source province at `0xdae450` calls that cleanup at `dae4ba`, then invokes the deleting destructor during array removal at `dae52e` or `dae575`. Removal by array index at `0xdae5a0` does the same at `dae5cb`, with deletion at `dae641` or `dae685`. These two removal functions were read through their ends in the parallel investigation; their critical cleanup and deletion branches were checked again against the executable. The real center's deleting destructor at `0x2dc670`, read through its return, only tests the deleting flag and frees 72 bytes. It performs no additional detach.

The daily religion update at `0x75ae70` was read through its end in the parallel investigation. It checks each center's source province, religion/type and relevant game conditions, calling the conversion update when valid and the indexed removal when invalid. Thus normal daily removal can complete the dangling-reference path; a world reset or framework intervention is not required by this path.

The state transitions established by these instructions are:

```text
A is converting P:
    A.target = P
    P.converting_center = A

create center B in P:
    A.target = null
    P.located_center = B
    P.converting_center = A       # retained

A chooses another target or has no target; later remove A:
    cleanup clears A's current target, if any
    free A
    P.converting_center = old A   # retained unless another operation cleared it
```

This is a concrete static dangling-reference path in the game baseline. Subsequent use can read freed memory. If an allocation later occupies that address, the icon can interpret its fields as a center. The actual center and captured `building_prov` record have different requested sizes, 72 and 64 bytes; no allocator history, size-class equivalence, or reuse of this particular block is established by their layouts.

Other lifecycle observations do not close the gap:

- The province reset at `0x9dcc10` clears progress and both center pointers. A later reset could remove a stale reference, but the dump does not establish that one ran for this province.
- The per-religion record's destructor at `0xdad6e0` uses the owning-array cleanup at `0x116050` to delete its centers directly. This whole-container destruction does not itself detach province references. World teardown ordering was not recovered completely, so it is not labeled an additional defect.
- The inspected manager-array growth transfers center ownership and zeroes the old slots before deleting the old records; it did not reveal a second deletion of transferred centers.
- Matching offsets in another persistent object's move operation were excluded because its vtable differs from the province's. Shared base vtables likewise do not identify a center type.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x769fb7` | code | Referenced while examining fault and complete local pointer chain; complete role not established |
| `0xd37b17` | code | policy center creation forgets the source's old converting center |
| `0xdae38f` | code | center creation forgets the source's old converting center |
| `0x132f43b` | code | reformation-center icon string-capacity read at the reported fault |
| `0x1c415a0` | data | building_prov record vtable |
| `0x1cd6640` | data | Referenced while examining fault and complete local pointer chain; complete role not established |


The unrelated `building_prov` object found at the captured address is described under [province persistence records](../program/province-records.md). [Crash evidence](../../research/reformation-center-crash.md) preserves the captured registers, object identification and causal limits.
