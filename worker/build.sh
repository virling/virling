#!/bin/sh
# Copies the landing pages from ../assets into public/ and builds the Worker.
# demo/ is generated: cargo run --release -- demo worker/demo (from the repo root).
set -e
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
rm -rf public && mkdir -p public/assets
cp ../assets/selfhost.html public/index.html
cp ../assets/agency.html public/agency.html
cp ../assets/pricing.html ../assets/self-host.html ../assets/demo.html public/
cp ../assets/integrations.html ../assets/blog.html public/
cp -R ../assets/integrations ../assets/blog ../assets/vs ../assets/tools public/
cp ../assets/vs.html ../assets/about.html ../assets/privacy.html ../assets/terms.html ../assets/refunds.html ../assets/what-we-dont-do.html ../assets/tools.html public/
# The changelog is rendered from the GitHub releases at build time.
curl -fsSL https://api.github.com/repos/virling/virling/releases | perl ../scripts/changelog.pl ../assets/changelog.html > public/changelog.html
cp ../assets/site.css ../assets/site.js ../assets/app.css ../assets/app.js ../assets/email-logo.png public/assets/
cp ../assets/auth.md public/auth.md
cp ../assets/favicon.svg ../assets/favicon.ico ../assets/v.js ../assets/v-compat.js ../assets/robots.txt ../assets/sitemap.xml ../assets/llms.txt ../assets/llms-full.txt ../scripts/install.sh public/
cp -R ../assets/art public/assets/art
mkdir -p public/assets/icons && cp ../assets/icons/stripe.svg ../assets/icons/dodo.svg ../assets/icons/polar.svg ../assets/icons/lemonsqueezy.svg ../assets/icons/revenuecat.svg public/assets/icons/
cp -R ../assets/fonts public/assets/fonts
cp -R ../assets/dia public/assets/dia
cp -R demo public/demo
# Inline the landing stylesheet so the first paint needs no extra request.
perl -0pi -e 'BEGIN { local $/; open F, "<", "public/assets/site.css"; $css = <F>; close F } s#<link rel="stylesheet" href="/assets/site.css">#<style>$css</style>#' public/index.html public/agency.html public/pricing.html public/self-host.html public/demo.html public/changelog.html public/integrations.html public/blog.html public/integrations/*.html public/blog/*.html public/vs.html public/about.html public/privacy.html public/terms.html public/refunds.html public/what-we-dont-do.html public/tools.html public/tools/*.html public/vs/*.html
# Stamp CSS and JS links with a content hash so they can be cached for a year.
for f in site.css site.js app.css app.js; do
  v=$(cksum "public/assets/$f" | cut -d' ' -f1)
  find public -name '*.html' -exec perl -pi -e "s#/assets/$f\"#/assets/$f?v=$v\"#g" {} +
done
# A Markdown copy of every page for AI crawlers, at the page's URL plus .md.
rustc -O --edition 2024 ../scripts/md.rs -o "${TMPDIR:-/tmp}/virling-md"
"${TMPDIR:-/tmp}/virling-md" public
cp -R ../assets/well-known public/.well-known
mkdir -p public/.well-known/agent-skills/virling && cp ../plugins/virling/skills/virling/*.md public/.well-known/agent-skills/virling/
d=$(shasum -a 256 public/.well-known/agent-skills/virling/SKILL.md | cut -d' ' -f1)
printf '{\n  "$schema": "https://schemas.agentskills.io/discovery/0.2.0/schema.json",\n  "skills": [\n    {\n      "name": "virling",\n      "type": "skill-md",\n      "description": "Answer revenue attribution questions from a Virling server: which channels, referrers, pages and AI assistants bring paying customers.",\n      "url": "/.well-known/agent-skills/virling/SKILL.md",\n      "digest": "sha256:%s"\n    }\n  ]\n}\n' "$d" > public/.well-known/agent-skills/index.json
cp _headers public/
worker-build --release
