ui: interface fragments

An interface fragment is Paradox .gui text the framework splices into one of
the game's windows as the game reads it. A fragment goes in
eu4-harmony/ui/<window>/<name>.gui, where <window> is the host window's
name; the army panel, single_unitpanel, is the one host there is now.
Coordinates are relative to the host window. A fragment in
eu4-harmony/ui/<window>/buttons/ goes in that window's button area instead
(eu4-harmony skills ui buttons). A window of the mod's own, laid out in
Lua, and pop-ups need no fragment (eu4-harmony skills ui windows). How
any of them should look and behave: eu4-harmony skills ui design.

- Name every element you will reach from Lua. The framework prefixes the
  names it splices, so they never clash; Lua uses the names as written.
- Sprites go in a .gfx file of the mod's own, in the mod's interface/
  folder, as in any Paradox mod.
- Texts: a localisation key where the .gui takes text (eu4-harmony skills
  localisation).

From Lua, game.ui.<name> is the element: set visible and enabled to true or
false. Text and tooltip use the same description: { key = "MYMOD_DETAILS",
values = { N = 3 } } or { text = "Ready" }. Assignment replaces the content;
nil clears it. The framework resolves keys in the player language when
presenting them. Tooltip is also readable, returning an independent description. Disabled elements keep their tooltip; hidden elements do not
show one. The tooltip belongs only to the element hit. Other properties are
set, never read.

  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

Done when every element has a name, every name used in Lua exists in a
fragment, and each element shows where it should in the game.
