# Province persistence records

`building_prov` records store an integer id, a key and a value. They are separate from building construction objects and from reformation centers.

The constructor at `0x194da0` installs vtable `0x1c415a0`, writes `id` at `+0x10`, constructs `key` at `+0x18`, and writes `value` at `+0x38`. Its deleting destructor at `0x194d60` destroys that string and frees a block of 64 bytes. Values captured after the first 64 bytes are not assigned to this object.

The field reader at `0xd7c080` and writer at `0xd7c100`, each read through its end, map these fields to tokens `id` (11), `key` (220) and `value` (1004). The static token-name table begins at `0x1fd8660`, with 6,648 entries of 520 bytes. The parent reader at `0x17b460` allocates these records and adds them to its list at `+0x280`; the corresponding loop in the parent writer at `0x17ca00` writes each under token `building_prov` (11500). Only the relevant part of that large parent writer was read.

The update wrapper at `0x17e8e0` passes the parent's `+0x280` list to `0x17f900`. These two functions were read through their ends. The latter looks up a record by integer id and string key, adds an integer to its value, creates a record when needed, and retains it only while the resulting value is positive; otherwise it destroys the string and frees the 64-byte record.



The record was identified during [a captured crash](../../research/reformation-center-crash.md). That identification does not establish how the bad reference arose.
