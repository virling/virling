# Contributing to Virling

Thanks for helping. Virling is early, so the most useful contributions right now are bug reports, payment-provider edge cases, and small focused fixes.

## Before you start

- For anything bigger than a small fix, open an issue first so we can agree on the approach.
- Look for issues labelled `good first issue` or `help wanted`.
- Security problems: do not open a public issue. Email security@virling.com instead.

## Development setup

You need Rust (stable, 1.85 or newer). The first build takes a while because it compiles DuckDB.

```sh
git clone https://github.com/virling/virling
cd virling
cargo test
cargo run -- add-site localhost.test --owner you@example.com
cargo run -- serve --app
```

Sign-in links are printed to the terminal when no SMTP server is configured.

The tracking script is `assets/v.js`. Keep it under 5 KB gzipped.

`worker/` holds the Cloudflare Worker that serves virling.com during the waitlist phase.

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Keep each PR to one change. Small PRs get merged faster.
3. Run these before pushing. CI runs the same checks:
   ```sh
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
4. Add tests for new behavior. Payment providers are tested against JSON fixtures in `src/providers/fixtures/`, never against live APIs.
5. Write commit messages as short imperative sentences, for example `Fix refund total for partial Stripe refunds`.

## Code style

- Match the style of the file you are editing.
- No new dependencies without discussing them in an issue first.
- Never store visitor IP addresses or personal data from visitors.
- Payment-provider keys must stay read-only where the provider allows it, and are always encrypted at rest.

## Contributor License Agreement

Virling is licensed under AGPL-3.0. Before we can merge your first pull request, you will be asked to sign a Contributor License Agreement. It lets Stacklance Infotech LLP keep offering the hosted version and other licenses while the code stays open source. You keep the copyright to your work.

## Releases

Maintainers cut releases from the Actions tab with the "Release PR" workflow. Version bumps and `CHANGELOG.md` are generated from commit messages, so a clear first word (Add, Fix, Update, Remove) puts your change in the right section.
