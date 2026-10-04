# Build the browser playground and serve it on http://localhost:8000
web:
    fastanim-web/build.sh
    python3 -m http.server -d fastanim-web/dist
