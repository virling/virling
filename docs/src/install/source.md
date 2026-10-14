# Build from source

You need Rust 1.99 or newer and a C++ compiler. Virling bundles DuckDB, which is compiled on the first build, so that build takes several minutes and a few GB of disk.

```sh
git clone https://github.com/virling/virling
cd virling
cargo build --release --locked
./target/release/virling --version
```

The binary is `target/release/virling`. Copy it anywhere: it has the web assets and database migrations built in.

To build a specific release, check out its tag first, for example `git checkout v0.1.0`.

## Run from the checkout

During development you can run it through Cargo. Arguments after `--` go to Virling:

```sh
cargo run --release -- add-site yourdomain.com --owner you@example.com
cargo run --release -- serve --app
```

The data folder defaults to `./virling-data` in the current directory. See [Contributing](../contributing.md) for tests and the tracker build.
