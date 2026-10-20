# Contributing

Virling is early, so the most useful contributions are bug reports, payment-provider edge cases and small focused fixes. For anything bigger than a small fix, open an [issue](https://github.com/virling/virling/issues) first to agree on the approach.

Security problems: do not open a public issue. Email security@virling.com.

## Development setup

You need Rust 1.99 or newer. The first build compiles DuckDB and takes a while.

```sh
git clone https://github.com/virling/virling
cd virling
cargo test
cargo run -- add-site localhost.test --owner you@example.com
cargo run -- serve --app
```

Sign-in links are printed to the terminal when no SMTP server is configured.

## Checks

CI runs the same checks. Run them before pushing:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## The tracker

The tracker's source is `assets/v.src.js`. After editing it, run `scripts/build-tracker.sh`, which writes the minified `assets/v.js`. It must stay at or under 1,310 bytes with `gzip -9`, and CI checks that. `node --test scripts/tracker.test.mjs` runs the built script in a fake browser.

## Payment providers

Providers are tested against JSON fixtures in `src/providers/fixtures/`, never against live APIs. Add a fixture for each new edge case.

## These docs

The docs are an mdBook in `docs/`. Pages are Markdown in `docs/src/`, and the theme is in `docs/theme/`. Preview them with:

```sh
cd docs
mdbook serve --open
```

Use the mdBook version pinned in `.github/workflows/deploy-docs.yml`. Pull requests that touch `docs/` build the book in CI, and broken internal links fail the build.

## Pull requests

- Branch from `main`, and keep each pull request to one change.
- Add tests for new behavior.
- Write commit messages as short imperative sentences, such as `Fix refund total for partial Stripe refunds`.
- Match the style of the file you edit. No new dependencies without an issue first.
- Never store visitor IP addresses or personal data from visitors.

## License agreement

Virling is licensed under AGPL-3.0. Before your first pull request is merged, you will be asked to sign a Contributor License Agreement. It lets Stacklance Infotech LLP keep offering the hosted version and other licenses while the code stays open source. You keep the copyright to your work.
