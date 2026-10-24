# Build the browser playground and serve it on http://localhost:8080, rebuilding on change
web:
    cd fastanim-web && trunk serve

# Build the scripting docs and serve them on http://localhost:3000, rebuilding on change
docs:
    mdbook serve docs/book

# Build the Python package for Pyodide into fastanim-web/wheels, for Python scenes in the playground
pyodide-wheel:
    #!/usr/bin/env bash
    set -euo pipefail
    # The Pyodide release pyplayer.js loads; its cross-build environment pins Emscripten and Rust.
    pyodide="uvx --python 3.14 --from pyodide-cli --with pyodide-build pyodide"
    $pyodide xbuildenv install 314.0.7
    $pyodide xbuildenv install-emscripten
    source "$($pyodide config get emscripten_dir)/../../emsdk_env.sh"
    rust="$($pyodide config get rust_toolchain)"
    rustup toolchain install "$rust" --profile minimal -t wasm32-unknown-emscripten
    rm -f fastanim-web/wheels/*.whl
    # Without the `preview` feature: Bevy doesn't build for the browser.
    cd fastanim-py && RUSTUP_TOOLCHAIN="$rust" MATURIN_PEP517_ARGS="--no-default-features" \
        $pyodide build -o ../fastanim-web/wheels
