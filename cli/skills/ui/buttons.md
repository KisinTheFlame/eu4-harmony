ui buttons: a button in the button area

The army panel's button area is a frame beside the panel, as high as its
own button columns, that the framework adds. A fragment in
eu4-harmony/ui/single_unitpanel/buttons/ holds one guiButtonType, which the
framework places in the next slot: 48 by 48, 46 apart, eight to a column,
a new column to the right after them, the frame growing with it. Give it a
name and a sprite, and leave its position out:

  guiButtonType = {
    name = "button"
    quadTextureSprite = "GFX_unitpanel_button_hunt_rebels"
  }

- The slots follow the playset's order of mods, then each mod's file
  names in order. A hidden button keeps its slot empty; the frame hides
  when every button and icon in it is hidden.
- A click calls on_button(game, name, click) with the name as written and
  click = { right, shift, ctrl }.
- Show it only when it applies, from on_frame, as the template does with
  game.army.selected().

Done when the button shows in the army panel, its tooltip reads in the
player language, and a click reaches on_button (the mod's log shows it).
