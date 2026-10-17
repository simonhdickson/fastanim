# Build the browser playground and serve it on http://localhost:8080, rebuilding on change
web:
    cd fastanim-web && trunk serve

# Build the scripting docs and serve them on http://localhost:3000, rebuilding on change
docs:
    mdbook serve docs/book

# Write the Rhai definition file that gives editors running rhai-lsp completion for scenes
rhai-defs:
    cargo run -q -p fastanim-cli -- script-api --definitions > fastanim.d.rhai
