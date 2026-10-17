#!/bin/sh
# Minifies assets/v.src.js into assets/v.js; run after editing the tracker.
set -e
cd "$(dirname "$0")/.."
npx -y terser@5.31.6 assets/v.src.js -c passes=5,unsafe=true,unsafe_arrows=true,ecma=2020 -f ecma=2020 -m --toplevel --comments '/^!/' -o assets/v.js
printf '%s bytes gzipped\n' "$(gzip -9c assets/v.js | wc -c | tr -d ' ')"
