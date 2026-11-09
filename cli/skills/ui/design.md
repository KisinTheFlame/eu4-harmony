ui design: an interface the player takes for the game's own

What the player sees of a mod should look and behave as the game's own
panels, buttons and dialogs do. These are the rules that make it so; each
comes from something that looked or felt wrong in a mod.

Looks:
- Borrow the game's pieces. Find a panel of the game's that does what yours
  does and take its sprites, fonts and buttons, by their names in the
  game's interface/*.gfx: ribbons, gilded rules, the top bar's icons, the
  technology view's bars. A window's background, title and Close are the
  framework's, already in the game's look.
- A sprite is drawn at its own size. An icon's size lays it out, but does
  not stretch the sprite: read the texture's size before using it, or a
  rule 362 wide runs out of a window 300 wide. Only a corner-tiled sprite,
  as the windows' backgrounds are, stretches to its size.
- What only shows does not answer the pointer: backgrounds, frames,
  ribbons and rules are icons, never buttons. A button lights up under the
  pointer and tells the player it does something.
- Name a figure by its icon, as the top bar does, with the name in the
  icon's tooltip, where room is short; leave the windows' margins (24) and
  keep things on one grid.

Buttons:
- A button is its words and what it does; it is no kind of button.
- Show a button only when it applies, as the game hides its own: hide it
  when nothing it acts on is selected.
- On a pop-up, two buttons at most. The keys go with the places, as on the
  game's dialogs: c and Return the right one, z and Esc the left one. Put
  the choice that goes ahead on the right, the one that holds back or looks
  elsewhere on the left.
- A Go to glides the camera there (camera.look_at with glide = true) and
  leaves the pop-up open (its action answers { close = false }), as the
  game's own: the player looks, then decides.

Panels of a selected thing (an army's settings, say):
- Stand it beside what it belongs to, not moveable; the same button opens
  and closes it, and it closes when the selection changes.
- A change takes effect at once, shown where it acts (the range on the
  map), rather than through Apply and Cancel.

Texts: every word through the mod's localisation, in the player language
(eu4-harmony skills localisation). No capability gives a country's name:
a mod that names countries keeps their names in its own texts.

Done when every window and pop-up has been seen in the game beside the
game's own panels, at the player's usual resolution and at another, and
nothing in it lights up, spills out or moves that the game's would not.
