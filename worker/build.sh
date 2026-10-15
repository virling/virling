#!/bin/sh
# Copies the landing pages from ../assets into public/ and builds the Worker.
# demo/ is generated: cargo run --release -- demo worker/demo (from the repo root).
set -e
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
rm -rf public && mkdir -p public/assets
cp ../assets/selfhost.html public/index.html
cp ../assets/agency.html public/agency.html
cp ../assets/site.css ../assets/site.js ../assets/app.css ../assets/app.js ../assets/email-logo.png public/assets/
cp ../assets/favicon.svg ../assets/v.js ../assets/robots.txt ../assets/sitemap.xml ../scripts/install.sh public/
cp -R ../assets/art public/assets/art
cp -R ../assets/fonts public/assets/fonts
cp -R demo public/demo
# Stamp CSS and JS links with a content hash so they can be cached for a year.
for f in site.css site.js app.css app.js; do
  v=$(cksum "public/assets/$f" | cut -d' ' -f1)
  find public -name '*.html' -exec perl -pi -e "s#/assets/$f\"#/assets/$f?v=$v\"#g" {} +
done
cp _headers public/
worker-build --release
