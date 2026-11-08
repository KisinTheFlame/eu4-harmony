# 2026-10-28 One-character map names and keys during composition (tested by me)

Backups as above. Framework settings as the user had them: system fonts, Microsoft YaHei for text, KaiTi for the map. Everything was checked in the single player lobby, East Asia bookmark (1444), without starting a game.

- **One-character country names** (reported by the user): 明 (Ming) lay upside down, and 藏, 康 and 卫 were turned too, while names of two characters or more read normally. A one-letter name takes the curve's direction at its very end (see [research/unicode-text.md](../research/unicode-text.md)).
  - Tried first: counting the game's one-or-two-letter rule in characters. 康, 藏 and 卫 came out upright but 明 lay on its side, because that rule's direction is read from the wrong type of value and is arbitrary. Moving the single glyph to the curve's middle changed nothing visible. Both were dropped.
  - Kept: a one-character name gets a horizontal direction. 明, 康, 藏 and 卫 stand upright in the middle of their territory; 朝鲜 and the other longer names look as before.
- **Enter during composition** (reported by the user): in the lobby's Find box, with Microsoft Pinyin in Chinese mode, `ming` showed its candidates next to the box; Enter put the letters `ming` into the box and the box stayed open. Backspace during the composition `ab` removed only `b` from the composition, leaving the box's `ming` alone; Escape cancelled the composition and the box stayed open. Before the fix, Enter also closed the box.
- **Tools**: `game-input.ps1 -Keys` now presses any letter or digit, so pinyin can be typed through the input method; `-Type` sends Unicode characters past it.
