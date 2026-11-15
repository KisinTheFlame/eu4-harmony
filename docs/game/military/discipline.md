# Discipline: battle overview and regiment calculation

The battle panel displays a representative country/leader value. A regiment separately adds qualifying national mercenary discipline and its local modifiers before the combat routine applies the result. Display and damage therefore have different inputs.

## The panel's number is a side overview

The base game's `interface/combat.gui` names both sides' discipline field **`dicipline_value`**, with the same spelling in the binary. The battle-panel update **`0x1051fd0`** finds that field at `0x1053a9f` and obtains its value by calling **`0x21d880(side, out_value, optional_text)`** at `0x1053abc`, with the text argument null. The tooltip path in **`0x104d2c0`** calls the same function at `0x104e2f3`, then adds the localisation `DISC_ADD_DESC`.

`0x21d880` does the following:

1. **`0x21fd90(side, out_country)`** chooses the country for the overview: a valid side leader's associated army's country, otherwise the first participating country; it has a special-country fallback through the side's units.
2. **`0x341820(country, out_value, optional_text)`** starts at 1000 (100%) and adds the country's ordinary discipline, modifier **`0x77`**. It has a minimum of 10 in the game's thousandths.
3. It adds the side leader's ordinary discipline modifier `0x77` through `0x8685a0`, if applicable.
4. When text is requested it formats the country's and leader's ordinary-discipline explanation. It does not iterate the side's regiments, calculate a mercenary share, add company discipline, or read mercenary discipline **`0x130`**.

The default explanation `DISC_ADD_DESC`, observed in the Chinese test, explicitly says that this is the leading army's discipline as a guide and that each unit uses its own value in combat. The base English text says: “This is the Discipline of the combat leader and should be seen as representative. Every unit still uses its own values during combat.” This is not a weighted discipline average of the side. An all-mercenary force need not make this overview show its mercenary discipline.

## The regiment's combat value includes the mercenary addition

The actual regiment discipline reader is **`0xae6580(regiment, out_value)`**, not `0xadf180` (the latter is unit-kind combat ability). Its complete body was read:

- It resolves the regiment's military country with `0xadf040` and calls `0x341820` for ordinary national discipline.
- If `regiment+0x64` is category 1 **or** its army (`regiment+0x40`) has a company at `army+0x48`, it reads the regiment owner's country modifier **`0x130`**, `mercenary_discipline`, and **adds** it to the result. The eligibility branch is at `0xae65ce`–`0xae65ec`; the addition is at `0xae667d`. The modifier registration at `0x8dfef4`–`0x8dff24` identifies index `0x130` with `MODIFIER_MERCENARY_DISCIPLINE`.
- It builds the regiment-local set with **`0xae7190`**, reads its ordinary discipline **`0x77`**, and adds that at `0xae66f4`. That set includes company ordinary discipline and the regiment's leader's modifiers. It also includes unit-category and applicable unit-conditional modifiers through **`0xae7600`**; this is an additional possible source, not a value shown by the side overview.

These are additions in percentage points to the discipline factor, not multiplication of national and mercenary discipline. A company with `discipline=0.05` adds another five percentage points through the local `0x77` term; the country `mercenary_discipline=0.298` is separately added only for qualifying regiments.


## It reaches the damage calculation

In the land-regiment attack routine **`0x225860`**, the acting regiment is taken from the attack context's `+8`. Its discipline is read through `0xae6580` at **`0x2262ee`** for strength damage and at **`0x226b0b`** for morale damage. The resulting factor is multiplied into those calculations. No later mercenary branch replaces it with the side overview's 105%.

The shared pending-damage accumulation and daily subtraction are described once in [the land-loss path](drill.md#from-the-factor-to-losses). This establishes the native route for the mercenary discipline addition; a controlled battle-loss comparison remains unmeasured.
