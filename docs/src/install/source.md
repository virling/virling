# Build from source

You need Rust 1.99 or newer and a C++ compiler. Virling bundles DuckDB, which is compiled on the first build, so that build takes several minutes and a few GB of disk.

## What the build needs

| Tool | Version | Notes |
| --- | --- | --- |
| Rust and Cargo | 1.99 or newer | `rust-version` in `Cargo.toml`. The code uses the 2024 edition. |
| C and C++ compiler | One that supports C++17 | The DuckDB build asks for `-std=c++17`. SQLite and the TLS library are compiled as well. |
| Git | Any | Clones the repo and checks out releases |
| Crates | The exact versions in `Cargo.lock` | `--locked` fails instead of changing them. The first build downloads them from crates.io. |

You do not install these:

- **DuckDB and SQLite.** Both are bundled and compiled in. The `duckdb` crate is 1.20000.1.
- **Node.** The built tracker, `assets/v.js`, is in the repo. Node only runs the [tracker tests](../contributing.md#the-tracker).

The repo's `Dockerfile` builds in `rust:1-bookworm` and adds no packages to it, so that image has everything the build needs.

`rustc --version` and `c++ --version` print what you have.

## Build it

```sh
git clone https://github.com/virling/virling
cd virling
cargo build --release --locked
./target/release/virling --version
```

The binary is `target/release/virling`. Copy it anywhere: it has the web assets and database migrations built in. The last command prints the version from `Cargo.toml`.

| Path | What it is |
| --- | --- |
| `target/release/virling` | The program, all you need to run Virling |
| `target/` | Build cache. Keep it for faster rebuilds, or remove it with `cargo clean` |
| `virling-data/` | Not part of the build. The data folder, created where you start Virling unless you set `VIRLING_DATA` |

To install it like a release, copy it to the path the [systemd](../self-host/systemd.md) example uses:

```sh
sudo install target/release/virling /usr/local/bin/virling
```

To build a specific release, check out its tag first, for example `git checkout v0.1.0`. `main` can hold changes that are not released yet.

## Update

1. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) and [back up the data folder](../self-host/data.md#backups). Going back after a migration ran is not supported without the backup.
2. Get the new code and build it. For a release:

   ```sh
   git fetch --tags
   git checkout vVIRLING_VERSION
   cargo build --release --locked
   ./target/release/virling --version
   ```

   For the newest code on `main`, use `git checkout main && git pull` in place of the first two lines.
3. Replace the binary and restart. Migrations run on start:

   ```sh
   sudo systemctl stop virling
   sudo install target/release/virling /usr/local/bin/virling
   sudo systemctl start virling
   curl -fsS http://127.0.0.1:8080/health
   ```

   The last command prints `ok`. The service has to exist already: see [systemd](../self-host/systemd.md).

A later build reuses `target/`, so DuckDB is only compiled again when its crate, your Rust version or the build settings change.

## Run from the checkout

During development you can run it through Cargo. Arguments after `--` go to Virling:

```sh
cargo run --release -- add-site yourdomain.com --owner you@example.com
cargo run --release -- serve --app
```

The data folder defaults to `./virling-data` in the current directory. See [Contributing](../contributing.md) for tests and the tracker build.
