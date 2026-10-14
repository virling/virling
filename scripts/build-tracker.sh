#!/bin/sh
# Minifies assets/v.src.js into assets/v.js; run after editing the tracker.
set -e
cd "$(dirname "$0")/.."
npx -y terser@5.31.6 assets/v.src.js -c passes=3,unsafe=true -m --toplevel --comments '/^!/' -o assets/v.js
printf '%s bytes gzipped\n' "$(gzip -9c assets/v.js | wc -c | tr -d ' ')"
