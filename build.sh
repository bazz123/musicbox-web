#!/bin/sh
# Builds the engine to WebAssembly and inlines it into a single, self-contained index.html.
# Needs Rust with the wasm32 target:  rustup target add wasm32-unknown-unknown
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown --manifest-path wasm/Cargo.toml
WASM=wasm/target/wasm32-unknown-unknown/release/musicbox_wasm.wasm
python3 - "$WASM" <<'PY'
import base64, sys
b64 = base64.b64encode(open(sys.argv[1], "rb").read()).decode()
page = open("src/page.html", encoding="utf-8").read().replace("__WASM__", b64)
head = '<!doctype html>\n<html lang="en">\n<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
open("index.html", "w", encoding="utf-8").write(head + page)
print(f"index.html written ({len(b64)//1024} KB of inlined wasm)")
PY
