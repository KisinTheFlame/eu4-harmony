# Remote control separates the engine from transports; the first transport is a named pipe carrying JSON lines

The remote control engine takes typed requests and gives typed responses: Rust values, one variant per capability, grouped by area, with no bytes or text format in them. It queues them, runs them at safe points, keeps the numbered event buffer and holds back the response to a request that waits, such as advancing time. It has no idea of connections.

A transport carries requests and responses between remote clients and the engine: it frames bytes, decodes them into requests, encodes the responses, and never touches game state. The first transport is a named pipe, `\\.\pipe\eu4-harmony`, open only to the Windows user running the game, with one JSON request per line and one JSON response per line. In JSON a country is its tag and an army ID is a decimal string, since a 64-bit integer does not survive a JavaScript number; binary results, such as a screenshot, are base64.

The command line client is part of `eu4-harmony.exe`: `eu4-harmony game <capability> --<argument> <value>` sends one request and prints the response as JSON, with a non-zero exit code on an error; `eu4-harmony game describe` lists the capabilities and their arguments. The request and response types and their descriptions live in a crate shared by the core and the command line tool, so the tool knows every capability without one written out by hand for each.

## Considered Options

- **TCP on localhost**: opens a port, meets firewalls, and is reachable by every user on the machine unless more is done.
- **The engine reading and writing JSON itself**: one transport less to write today, but every later transport (WebSocket, MCP, a binary format) would have to speak JSON to the engine.
- **A hand-written command per capability in the command line tool**: every capability would be declared a third time, after the ABI and the engine.

## Consequences

- A new transport is new code at the edge only; the engine and the capabilities do not change.
- Each call of the command line tool is a short connection, so remote control has no notion of a client being "connected"; that belongs to transports that keep connections.
- The engine's tests drive it with typed requests and need no pipe.
