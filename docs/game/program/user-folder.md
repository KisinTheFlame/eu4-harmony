# The user folder: how the game reads `userdir.txt`

## Reading `userdir.txt`

`0x1714b40(out)` (`virtualfilesystem.cpp`) returns the folder as a `std::string`, empty when there is no `userdir.txt` or it is empty:

- It opens `userdir.txt` through PhysFS (`0x1718300`), and reads at most 511 bytes of it, up to the file's length, into a NUL-ended buffer.
- At `0x1714d77` it calls `0x17043c0`, which makes a copy with the bytes at `0x1dbf598` (LF, TAB, CR, space) trimmed from both ends; at `0x1714d83` it destroys that copy unused. **The text is used untrimmed**: a line break after the path stays in it. Nor is a byte order mark taken off.
- A text that begins with `.` is taken as relative to the folder PhysFS found `userdir.txt` in (`0x17181d0`), with the rest of the text after the dot appended to it. Any other text goes on as it is, so a relative path without a leading dot is relative to the process's current directory.
- The text is UTF-8: `MultiByteToWideChar(CP_UTF8)` into 260 wide characters (`MAX_PATH`), then `CreateDirectoryW`, then `GetFullPathNameW`, and the full path back in UTF-8 is the result. When `CreateDirectoryW` fails with `ERROR_PATH_NOT_FOUND` (a folder above it is missing), the game stops with "Could not create or read directory: …". A folder that already exists is fine. Spaces in the path need no quotes, and quotes would be part of the path.

Seen 2026-10-26: with a `userdir.txt` whose path ended with a line feed, the game crashed at startup before it wrote its usual logs (Application Error 1000 in `pops_api.dll` at offset `0x5bb39`, `0xc0000005`); with the line feed taken out, the same folder worked. Why that path crashes in `pops_api.dll` was not looked at.

So whatever writes `userdir.txt` writes the path's UTF-8 bytes alone: no byte order mark, no quotes, no line break after it.
