# Sensitive capabilities are declared and shown, not granted

Some capabilities reach beyond the game: the real date and time (`clock`), files in the extension's own data folder (`files`), and HTTP requests to named hosts (`network`). They are worth having (a mod's settings outside saves, statistics sent to a server of the author's own), and a player may still want to know about them before playing. An extension can use one only when its manifest declares it, and using one it did not declare raises an error, so what the manifest declares is everything the extension can do beyond the game. The player sees each mod's declarations in the mods window. The player cannot refuse them in this version: enabled in the playset still means loaded, as the first version design has it.

What a mod has no imaginable use for, such as the operating system's details, the process, environment variables or native code, is not a capability at all, declared or not.

## Considered Options

- **Ask the player at startup and remember the answer**: what an approval prompt would do, but a mod refused what it needs either breaks or needs a second way of working, and the player cannot judge a request better than by the mods window's list; it may come later, on top of the declarations.
- **HTTPS only**: what a mod sends could then not be read or changed on the way, but authors running their own servers, often without certificates, would be shut out. Plain HTTP stays allowed until a better answer is found.
- **Any host**: a manifest that names its hosts tells the player where a mod talks to, and keeps a mod from sending data elsewhere.
- **Files anywhere the player could reach**: a mod could read saves, settings and other mods' files, or fill the disk anywhere; one folder per extension holds everything a mod needs to keep.

## Consequences

- The mods window shows what every Harmony mod declares, each sensitive capability in words a player understands, with the hosts for `network`.
- Declaring is cheap, so the list says what a mod can do, not what it does; nothing checks what it sends.
- A new sensitive capability is added to the list, the manifest's `capabilities` and the mods window together.
