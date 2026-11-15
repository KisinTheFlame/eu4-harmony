# Text formatting and color removal

The common native color remover constructs an output string by scanning source bytes. It removes a single-byte color marker and its following selector, preserving the other bytes. Callers include mercenary-army creation and save-name paths; complete behavior of every caller has not been established.

## The native color remover treats every byte as a character

`0x17046a0` takes `(const std::string* source, std::string* out)` and returns `out`. It initializes an empty output, reads the source length into a signed 32-bit loop bound, and walks the string one byte at a time:

- At `0x1704704`, compare the byte with `0xa7`, the game's single-byte `§` color marker.
- If equal, advance the index twice, dropping that byte and the next byte without examining them.
- Otherwise append exactly that byte. It builds a one-byte temporary with string repetition `0x90320` (count 1), appends it with `0x932f0`, then destroys it, including its heap storage if needed.
- Return the output after the whole loop; there is no UTF-8 or legacy-escape decoding and no other formatting-code removal in this function.

In vanilla's single-byte strings this removes a color marker and its selector. In the Unicode module's game text, `0xa7` can instead be a continuation byte inside a Chinese character. The source is already decoded game text here; decoding legacy escapes again cannot repair this loss.

## Scope and the repair boundary

The native remover has eight direct callers: the calls at `0x3a8b39`, `0x411d0c`, `0x844ce0`, `0x852f9c`, `0x853482`, `0xe7c8eb`, `0xe7d3d0` and `0x151c624`. Each passes a source string in RCX and a separate output slot in RDX. Where used, RAX supplies the constructed output for a copy, move or further text operation. The calls at `0x844ce0`, `0x852f9c` and `0x853482` sit in save-name paths; the first passes the result to the existing Unicode save-name replacement. The other callers' complete effects remain unknown; only mercenary hiring was followed as a game operation.


Hiring’s naming sequence is described in [mercenary names](../military/mercenary-names.md). Character-boundary repair and the before/after byte evidence are [separate from the native rule](../../research/mercenary-company-names.md).
