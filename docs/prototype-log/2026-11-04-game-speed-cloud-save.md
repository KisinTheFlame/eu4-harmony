# 2026-11-04 Game speed: a cloud save (tested by me, with the owner's leave)

The same branch and install as the run before, the player's playset, a new game as Portugal (`menu.start`), driven through remote control's input and screenshots; the game claimed with the game-lock skill.

- **Steam**: started as `eu4.exe` alone, the game had no cloud: the save window's 云 tab did not open. Started again with `SteamAppId` and `SteamGameId` set to 236850 in its environment, as Steam starts it, the tab opened, with Steam's quota (954 MB, 948 MB free).
- **The save**: the save window's 云 tab, the name `eu4harmony_cloud_test`. Steam's folder for the game's cloud (`userdata\<account>\236850\remote\save games`) had the file at once, 3.3 MB, and Steam's index of it (`remotecache.vdf`) listed it to be uploaded. Its three entries were deflated as the game deflates them (`gamestate` 33.7 MB to 3.3 MB, each beginning `EU4bin`, `zipfile` found it whole), and the framework's log had no line of the save thread: the local saves' points left it alone.
- **Loading it back**: the load window's 云 tab listed it with the cloud's mark of a synchronized file (and 944 MB free); loading it played it (session 3, "from a loaded save"), the framework's data of the save's extension read too.
- **Upload**: after the game's own exit (its menu, 退出游戏), Steam's index gave the file a remote time and the synchronized state, as it has for the player's older cloud saves.
- **Not verified**: the file on another machine. The test save is still in the player's Steam cloud, for the player to delete (the in-game load window, or Steam).
- **Cleanup**: the framework files that were in the game folder were put back and compared equal. Releasing the game restored the player's files and found every file as the backup had it.
