# Installing the CLI

`virling query` ships in the same `virling` binary as the server. Install it on the machine the agent runs on; it does not need a data folder.

- Release binary: download the archive for the platform from https://github.com/virling/virling/releases, unpack it and put `virling` on the `PATH`.
- From source, with Rust installed: `cargo install --git https://github.com/virling/virling virling`.
- Docker, without installing: `docker run --rm -e VIRLING_URL -e VIRLING_API_KEY ghcr.io/virling/virling query sites`.

Then check it with `virling query --help`. Releases older than the CLI do not have the `query` command; use the HTTP API below with them.

## The API key

The user creates it in the Virling app under **Settings > API keys**. It is shown once. It reads every site of their account and cannot change anything. If a command fails with `401`, the key is wrong or was revoked; ask the user for a new one.

## No CLI?

The same data is a plain HTTP API, documented at https://docs.virling.com/reference/api.html. Send `Authorization: Bearer $VIRLING_API_KEY` to:

- `GET $VIRLING_URL/api/v1/sites`
- `GET $VIRLING_URL/api/v1/sites/<domain>/overview?range=30d`
- `GET $VIRLING_URL/api/v1/sites/<domain>/breakdown/<channel|referrer|campaign|landing_page|page|country|device>?from=2026-09-01&to=2026-09-30`
- `GET $VIRLING_URL/api/v1/sites/<domain>/payments?limit=50&offset=0&filter=attributed`
- `GET $VIRLING_URL/api/v1/sites/<domain>/visitors/<visitor_id>`
