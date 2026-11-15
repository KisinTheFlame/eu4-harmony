# The AI


- The AI runs inside the day (`0x75b4b0`), only on the host or in single player and when `world+0x7484` allows it.
- Each AI country's update (`0xb0fa0`) runs in parallel.
- A country's AI is `CAIKing` (`ai_king.cpp`, at `country+0x3d0`), with agents in `ai_army`, `ai_admiral`, `ai_economy`, `ai_foreignminister`, `ai_strategy` and others.
- It acts only by posting ordinary commands, as the player's interface does. So only the commands, not the AI itself, have to be deterministic across machines (inferred).
