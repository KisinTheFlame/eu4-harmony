# Address index plan

Status: decided (2026-11-11, the owner). Built the same day, I1 to I4: 6,126 addresses indexed, 1,126 named from the dependency points' own names.

## Goal and scope

- **Goal**: what is known of an address of the game's is found by its address, at once, and the tools that show the game's code show it too; what research finds is kept where the next search finds it. The research notes name some 6,100 addresses of `eu4.exe`, each in a paragraph of a note on its topic, and nothing lists them: on 2026-11-11 the map mode's colors, the pdxmap's drawing and its borders were read again from the code although three notes had them.
- **Leaves out**: the fields of the game's objects (`+0x1270`), which are only known together with the object they are in; a second copy of what the notes say. The index names an address and points to the notes, which say what is known.

## Behavior

- **The index** (`docs/research/addresses.toml`): one entry for each address of `eu4.exe` the notes and the game's pages name, by its RVA: what it is (a function, data, a vtable), a short name where one is known, and the notes that name it. Names are written as the notes write things, in the game's terms ("the map's color fill"); an entry with no name yet says only where it is named.
- **Kept whole by a test**: every address the notes and the game's pages name, as an RVA in backticks, is in the index, and every note the index points to names it. `cargo xtask addresses` writes the index again from the notes, keeping each name, so that a note changed is followed by one command.
- **`cargo xtask what <rva>`**: the index's entry, and each paragraph of the notes, the game's pages and the code's comments that names the address, with its file and line; an address inside a function is looked up by the function it is in.
- **The disassembler names what it reaches**: `cargo xtask disasm` writes the index's name after each address an instruction calls, jumps to or reads, as it now writes the strings it reads.
- **A rule** in [AGENTS.md](../../AGENTS.md): before reading a function of the game's, look it up; a name learned is written into the index with the note that says it.

## Parts

- **I1**: the index made from the notes as they are, with the names the code's dependency points already give (each point's `what`); the test that keeps it whole; `cargo xtask addresses`.
- **I2**: `cargo xtask what`.
- **I3**: the disassembler's names.
- **I4**: the rule, in AGENTS.md.

Every part is done when its test passes and [dev-tools.md](../dev-tools.md) describes it.
