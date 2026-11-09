ui windows: windows and pop-ups of the mod's own

An extension window is a window the mod lays out in Lua, which the
framework builds in the game's own interface, in the look of the game's
panels: its background, title, close button, text, buttons and sprites are
the game's. A pop-up is a window with a title, a text and buttons, in the
look of the game's dialogs, that a handler waits on. Both exist only in a
game, and close when it ends. No .gui file is written for either.

steps:

1. Lay the windows out in the table main.lua returns, under windows, each
   by its name (lowercase letters, digits and _):

     windows = {
       reforms = {
         size = { 460, 560 }, title = "MYMOD_REFORMS_TITLE",
         entry = { sprite = "GFX_my_entry", frame = 1, tooltip = "MYMOD_REFORMS_ENTRY" },
         items = {
           { type = "text", name = "status", size = { 412, 40 }, text = "MYMOD_STATUS" },
           { type = "row", spacing = 10, items = {
             { type = "button", name = "pick", text = "MYMOD_PICK" },
             { type = "icon", name = "seal", sprite = "GFX_my_seal", size = { 40, 40 } },
           }},
           { type = "progress", name = "authority", value = 0.5 },
           { type = "list", name = "electors", size = { 412, 220 }, row = { size = { 396, 66 }, items = {
             { type = "row", items = { { type = "flag", name = "flag" }, { type = "text", name = "name", size = { 200, 20 } } } },
           }}},
         },
       },
     }

   A window is a column of its items under its title; column and row lay
   theirs out with spacing and align; any item may stand at at = { x, y }
   from the window's top left corner instead. Window options: close
   (true: the game's long Close button centered at the bottom, which Esc
   presses), moveable (true), panel (true: one of the game's big panels,
   which close one another), ribbon ("red" or "blue", the title's; a window
   with no title has none), anchor ("center", "top_left", ..., "right",
   "bottom_right") with offset = { x, y }, entry (a button on the panel
   button strip left of the minimap, beside the papacy's and the Empire's).
2. Game widgets: text (size, text = key, font "body" or "heading", align),
   icon (sprite, frame, size), button (text = key for the game's text
   button, which comes 79, 109, 149, 189 or 224 wide and 31 high; or
   sprite and frame for an icon button, with its size), progress (a
   progressbartype sprite, GFX_wargoal_progressbar 288 by 18 unless named,
   value 0 to 1), flag (a maskedShieldType, GFX_shield_thin 92 by 92
   unless named, country = tag), list (size, row = { size, items }, which
   scrolls). Buttons, icons, progress bars and flags take tooltip = key;
   texts take none. Every key is the mod's; a layout that is wrong, or a key
   the mod lacks, keeps the extension from loading, with the reason.
3. From a handler, open and close: game.window.open("reforms"),
   game.window.close("reforms"). Set what shows, never read it:
     game.window.reforms.status.text = text.get("MYMOD_DONE")
     game.window.reforms.pick.enabled = false
     game.window.reforms.authority.value = 0.8
     game.window.reforms.entry.visible = false
     game.window.reforms.electors.rows = { { flag = "FRA", name = "France" }, ... }
   A row's entry is words for a text, a number for a progress bar or an
   icon's frame, a tag for a flag, true or false for whether a text button
   is enabled, or a table of properties. What is set while a window is
   closed shows when it opens. A text button's words and a text's tooltip
   do not change once laid out; the entry button shows no number.
4. Hear it: on_button(game, "reforms.pick", click), click.row set for a
   button in a list's row; on_window(game, "reforms", open) whenever it
   opens or closes, whoever did it: the mod, its entry button, its close
   button, Esc, or another of the game's big panels opening.
5. A pop-up, in a handler or a task, which waits for the player:
     local answer = game.popup { header = "MYMOD_STEWARD", title = "MYMOD_ASK",
       text = "MYMOD_ASK_TEXT", values = { N = 3 }, agree = "MYMOD_YES", decline = "MYMOD_NO",
       shields = { "FRA", "ENG" }, id = "ask" }
   It is the game's message pop-up: a header line, the title on the green
   band, a shield at each corner (the player's country unless named), the
   text, and one or two buttons. agree stands on the right, pressed by c
   and Return, the game's own OK when left out; decline on the left, pressed
   by z and Esc. In place of decline, province = 151 puts the game's Go to
   on the left, which glides the camera there and leaves the pop-up open,
   as the game's Go to does. A click on a shield opens the diplomacy view
   on its country. answer is "agree", "decline", or "closed" when it
   closed with neither: game.popup.close("ask"), the session's end, or the
   mod stopped. Several may be open at once, and
   remote clients list and answer them (popup.list, popup.answer).

Done when eu4-harmony test passes with mod:window, mod:press, mod:entry,
mod:close, mod:popups and mod:answer_popup (eu4-harmony skills testing), and
each window shows in the game where it should.
