#!/bin/sh
# Builds the playground into fastanim-web/dist. Needs the wasm32-unknown-unknown target and
# wasm-bindgen-cli at the version in Cargo.lock:
#   cargo install wasm-bindgen-cli --version <x.y.z> --locked
# Serve dist with any static server, e.g. `python3 -m http.server -d fastanim-web/dist`.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown -p fastanim-web
rm -rf dist
wasm-bindgen --target web --no-typescript --out-dir dist \
    ../target/wasm32-unknown-unknown/release/fastanim_web.wasm
cp index.html worker.js ../fastanim-script/scenes/*.rhai dist/
