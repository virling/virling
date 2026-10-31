# Contributing to Virling

Thanks for helping. Virling is early, so the most useful contributions right now are bug reports, payment-provider edge cases, and small focused fixes.

## Before you start

- For anything bigger than a small fix, open an issue first so we can agree on the approach.
- Look for issues labelled `good first issue` or `help wanted`.
- Security problems: do not open a public issue. Email security@virling.com instead.

## Development setup

You need Rust (stable, 1.99 or newer). The first build takes a while because it compiles DuckDB.

```sh
git clone https://github.com/virling/virling
cd virling
cargo test
cargo run -- add-site localhost.test --owner you@example.com
cargo run -- serve --app
```

Sign-in links are printed to the terminal when no SMTP server is configured.

The tracking script source is `assets/v.src.js`. Run `scripts/build-tracker.sh` after editing it; it writes the minified `assets/v.js`, which must stay under 1 KB gzipped (CI checks).

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

## Sign off your commits

Contributions are accepted under the [Developer Certificate of Origin](https://developercertificate.org/): by signing off, you confirm you wrote the change or have the right to submit it under the project's license (AGPL-3.0). Add the sign-off with `git commit -s`, which appends a line like:

```
Signed-off-by: Your Name <you@example.com>
```

Pull requests from forks are checked for it on every commit. To fix a missing one, run `git commit --amend -s` for the last commit or `git rebase --signoff main` for all of them, then push again.

## Releases

Maintainers cut releases from the Actions tab with the "Release PR" workflow. Version bumps and `CHANGELOG.md` are generated from commit messages, so a clear first word (Add, Fix, Update, Remove) puts your change in the right section.
