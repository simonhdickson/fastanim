# Build the browser playground and serve it on http://localhost:8080, rebuilding on change
web:
    cd fastanim-web && trunk serve

# Build the scripting docs and serve them on http://localhost:3000, rebuilding on change
docs:
    mdbook serve docs/book
