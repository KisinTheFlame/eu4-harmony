---
name: game-analysis
description: Look up known EU4 addresses, inspect disassembly/vtables/call graphs, and decompile functions through the repository's persistent Ghidra project. Use before reading game code, finding dependency points, or calling external decompilers on eu4.exe.
---

# Analyzing the game binary

Use the repository's own tools (`what`, `disasm`, `decompile`) to study `eu4.exe` and the DLLs in the game folder. They keep all addresses in RVAs, share `docs/game/addresses.toml`, and cache Ghidra's analysis locally instead of repeating cold imports.

Do **not** open `eu4.exe` through REA's ephemeral Ghidra session: REA runs a fresh `-import -deleteProject` with full auto-analysis each session and hits its 330-second startup timeout. Use `cargo xtask decompile` instead.

## Workflow

1. **Check prior knowledge first (`what`)**:
   - By RVA: `cargo xtask what 0x8fa80` — shows the index entry (and containing function), matching paragraphs from `docs/game/`, and references in `analysis/src`, `core/src`, and `xtask/src`.
   - By keywords: `cargo xtask what color fill` or `cargo xtask what advisor` — case-insensitive AND search over `name` and `notes` in `docs/game/addresses.toml`. Single hits expand automatically.
2. **Locate addresses, callers, vtables, strings, and bytes (`disasm`)**:
   - Fast pure-Rust lookup (`--module <file>` switches from `eu4.exe` to another game DLL such as `pops_api.dll`; non-`eu4.exe` modules do not apply `eu4.exe` names):
     - `str:TEXT` / `stri:TEXT` / `wstr:TEXT`: ASCII, case-insensitive ASCII, and UTF-16LE string searches with code references.
     - `callers:RVA[:N]` / `callees:RVA[:depth]`: direct callers/vtables and outgoing call trees (default depth 1, max 5).
     - `vtable:RVA[:N]` / `slots:VTABLE:OFF...`: inspect vtable slots (auto-stops at non-code pointers or the next code-referenced table, capped at 128).
     - `data:RVA[:N]`: hex/ASCII dump with relocated image pointers and string previews.
     - `uses:RVA:VALUE...`, `refs:VALUE[:N]`, `chain:GLOBAL:OFFSET[:N]`, `bytes:RVA:N`, `args:RVA`: dependency-point bytes, argument counts for observers, and instruction scans.
3. **Read function logic (`decompile`)**:
   - One-time project initialization per binary SHA-256 (stored under `%LOCALAPPDATA%\eu4-harmony-dev\ghidra\` or `EU4_HARMONY_DEV_DIR`):
     ```powershell
     cargo xtask decompile --init
     cargo xtask decompile --init --module pops_api.dll
     ```
   - Query one or more RVAs (functions or interior addresses):
     ```powershell
     cargo xtask decompile 0x8fa80 0x8f450
     cargo xtask decompile --module pops_api.dll 0x6bd70
     ```
   - The first query auto-starts a read-only warm Ghidra server on `127.0.0.1` (15-minute idle exit) so consecutive queries finish in under a second. Use `--oneshot` to avoid leaving a background server, `--timeout <seconds>` to override the 60-second per-function limit, `--status` to inspect cached projects and live servers, `--stop [--module <file>]` to stop warm servers, and `--rebuild` to re-run full auto-analysis.
   - For `eu4.exe`, Ghidra default symbols (`FUN_14...`, `DAT_14...`, `LAB_14...`, `PTR_..._14...`) are rewritten to RVAs and `docs/game/addresses.toml` names/notes are overlaid live.
4. **Record what you establish**:
   - Document verified behavior, RVAs, callers, state ownership, and limits in the relevant topic under `docs/game/`.
   - Run `cargo xtask addresses` to refresh `docs/game/addresses.toml`, and fill in learned `name` entries by hand.

## Toolchain configuration

If `cargo xtask decompile` reports Ghidra missing, configure either:
- Environment variables `GHIDRA_INSTALL_DIR` and `JAVA_HOME` (or `PATH`), or
- `%LOCALAPPDATA%\eu4-harmony-dev\ghidra.toml` (outside the repository) with `ghidra_dir`, optional `java_home`, and optional `max_mem` (otherwise `GHIDRA_HEADLESS_MAXMEM`, `GHIDRA_MAXMEM`, or half of physical RAM clamped to `2G..=8G`).
