#!/bin/sh
# Copies the landing pages from ../assets into public/ and builds the Worker.
set -e
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
rm -rf public && mkdir -p public/assets
sed '/src="\/v.js"/d' ../assets/selfhost.html > public/index.html
sed '/src="\/v.js"/d' ../assets/agency.html > public/agency.html
cp ../assets/site.css ../assets/site.js ../assets/app.css ../assets/email-logo.png public/assets/
cp ../assets/favicon.svg public/
worker-build --release
