# musicbox-web

A single-page browser player for [musicbox](https://github.com/benaskins/musicbox),
Ben Askins' generative ambient engine written in Rust.

**▶ Play it: https://bazz123.github.io/musicbox-web/**

The page runs the real `musicbox-core` engine, compiled to WebAssembly and inlined
into `index.html`, inside an `AudioWorklet`. Nothing is re-implemented in JavaScript,
so it sounds the same as `cargo run`.

## What's on the page

- **Drone** and **Ambient techno** styles
- **Seeds**: the same seed always plays the same piece
- **Voice mixer** for techno: take over the arrangement, toggle the 11 voices and
  change each voice's pulse ratio
- A live scope and peak meter

## Layout

| Path | What it is |
|------|------------|
| `index.html` | The built, self-contained player (open it directly or serve it) |
| `src/page.html` | Page source; `__WASM__` is replaced with the engine at build time |
| `wasm/` | A small `extern "C"` wrapper around `musicbox-core` (no wasm-bindgen) |
| `build.sh` | Builds the wasm and writes `index.html` |

## Build

```sh
rustup target add wasm32-unknown-unknown
./build.sh
```

`musicbox-core` is pulled from GitHub at a pinned commit; see `wasm/Cargo.toml`.

## Credits and license

The synthesis engine is © 2026 Ben Askins, licensed
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). This repository
adapts it for the browser and is released under the same license. The plate reverb
is ported from [Mutable Instruments Eurorack](https://github.com/pichenettes/eurorack)
by Émilie Gillet (MIT). See `LICENSE`.
