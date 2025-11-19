# Deimos Rising Rust port

The goal of this project is to port the Deimos Rising game to Rust.

## Usage

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [cross](https://github.com/cross-rs/cross) (unless compiling on Windows i686)

### Building

```bash
cross build --release --target i686-pc-windows-gnu
```

### Running

Copy the resulting executable from `target/i686-pc-windows-gnu/release/deimos_rising.dll` to the same directory as the game files, and name it `COMCTL32.dll`. This will cause the game to load our Rust version at startup, where we can intercept and modify the game's behavior.

If running via Wine, make sure to set the `COMCTL32` override to `native`.
