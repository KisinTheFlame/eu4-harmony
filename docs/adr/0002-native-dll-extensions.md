# Extensions are native DLLs exporting a C ABI

Extensions come as Windows DLLs that export a C ABI. The framework loads them inside the game process; they are not put in a WASM or Lua sandbox. What the first version needs (hooking any game function, reading game memory directly) would each have to be wrapped by the framework first to work in a sandbox, and the first version could not get through all of that. A C ABI also lets extensions be written in Rust, C++ and other languages.

## Consequences

- Extensions have every right inside the process. The first version asks for no confirmation before loading: enabled in the playset means loaded. Before the framework is opened to outside developers, the security of extensions from the Steam Workshop has to be assessed again.
- The interface represents game objects by handles, not raw pointers, so adding a sandbox host later does not mean throwing the interface away.
