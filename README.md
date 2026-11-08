# musicbox-web

A single-page browser player for [musicbox](https://github.com/benaskins/musicbox),
Ben Askins' generative ambient engine written in Rust.

**▶ Play it: https://bazz123.github.io/musicbox-web/**

The page runs the real `musicbox-core` engine, compiled to WebAssembly and inlined
into `index.html`, inside an `AudioWorklet`. Nothing is re-implemented in JavaScript.
With the default key and tuning the output is sample-for-sample identical to upstream.

## What's on the page

- **Drone** and **Ambient techno** styles
- **Seeds**: the same seed always plays the same piece
- **Voice mixer** for techno: take over the arrangement, toggle the 11 voices and
  change each voice's pulse ratio
- **Key and tuning**: any key from E to D♯; original, minor pentatonic, natural minor,
  Dorian, Phrygian, In-sen or Hirajoshi scales; A4 = 432, 440 or 442 Hz; equal
  temperament or just intonation (5-limit with a 9/5 minor seventh, or 7-limit with
  the 7/4 harmonic seventh, matching the techno track's 9/5 and 7/4 pulse ratios)
- **Drone mixer** ("What you'll hear"): level, meter, mute and solo per layer; **Freeze**
  on the highs' plate reverb; **Density**, **Brightness** and **Space** macro knobs; and a
  details drawer per layer (breath, drift, warmth, sub octave, filter sweep and resonance,
  string brightness and ring, BBD delay time, feedback and wow, reverb decay, tone and mix,
  fade time, engine limiter)
- **Master section**: low cut, tilt EQ, mono bass, width, drive, loudness target,
  true-peak look-ahead limiter, and EBU R128 meters
- A live scope and peak meter, and the scale's notes under it lighting up as they sound

## Layout

| Path | What it is |
|------|------------|
| `index.html` | The built, self-contained player (open it directly or serve it) |
| `src/page.html` | Page source; `__WASM__` is replaced with the engine at build time |
| `engine/` | Ben Askins' `musicbox-core`, vendored and modified (see below) |
| `wasm/` | A small `extern "C"` wrapper around the engine (no wasm-bindgen) |
| `build.sh` | Builds the wasm and writes `index.html` |

## Build

```sh
rustup target add wasm32-unknown-unknown
./build.sh
```

Run the engine's tests with `cargo test --manifest-path engine/Cargo.toml`.

## Changes to the engine

`engine/` is a copy of `musicbox-core` from
[benaskins/musicbox@8aa47f2](https://github.com/benaskins/musicbox/tree/8aa47f2c9083d246f4cf3232c799599f2263b75f/musicbox-core).
The first commit that adds it is unmodified; the changes after it are:

- `src/tuning.rs` (new): key, scale, reference pitch and intonation. Each hard-coded
  pitch is re-read as an interval above A, moved to the same degree of the chosen scale
  (nearest note when scale sizes differ), transposed and tuned.
- `tracks/drone.rs`, `tracks/ambient_techno.rs`: `with_tuning` constructors, and every
  pitch table read through the tuning.
- `instruments/pads.rs`: `trigger_triad`, so pad chords can follow the scale.
- `lib.rs`: `MusicBox::with_tuning`.
- `tracks/drone.rs`: per-layer gains (smoothed), layer peak meters, `set_param` for the
  mixer and detail controls, optional bass saturation, switchable engine limiter.
- `effects/reverb.rs`: freeze, a 50 ms crossfade to a closed, undamped tank.
- `instruments/pluck.rs`: string damping as a field (was a fixed 0.7).
- `effects/delay.rs`: `BbdDelay::reserve_ms` for longer delay times.
- Live note display: `tuning::NoteActivity`; trigger counters on `MonoSynth`, `DubStab`
  and `SynthPad`; `Oscillator::envelope`; pluck count in `PluckEngine`. Read-only, no
  effect on the audio.

Every change leaves the defaults alone: with nothing touched, the output is identical to
upstream sample for sample.

## Credits and license

The synthesis engine is © 2026 Ben Askins, licensed
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). This repository
adapts it for the browser and is released under the same license. The plate reverb
is ported from [Mutable Instruments Eurorack](https://github.com/pichenettes/eurorack)
by Émilie Gillet (MIT). See `LICENSE`.
