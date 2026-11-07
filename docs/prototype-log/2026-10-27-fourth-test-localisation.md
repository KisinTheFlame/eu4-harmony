# 2026-10-27 Fourth test (localisation)

The same playset as last time (regional-combat plus the two test mods), a new game.

- **Deciding the language**: the result is `simp_chinese`. The decision happens in the deferred initialization inside `PHYSFS_init`; the Windows display language is Chinese. The game's own language is english.
- **Splicing**: the game reads `localisation/text_l_english.yml` through `PHYSFS_openRead` on the main thread, twice. It reads the copy the Chinese language mod overrides; with regional-combat's 6 keys appended it goes from 716155 to 716886 bytes.
- **Interface**:
  - The notices in the main menu are in Chinese.
  - In the game, the notice window binds to the top bar, and the army panel binds 9 elements.
  - The region button's text (from the spliced keys), the tooltips and the radius box are all in Chinese; the user confirmed they are fine.
- **Errors**: no related errors in `error.log`.
- **Conclusion**: item 11 passes.
- **Cleanup**: after the test the two test mods were deleted (their registration files under `mod\` in the user directory, and `target\notice-test\`).
