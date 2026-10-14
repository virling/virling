# Data, backups, upgrades

## The data folder

Set with `--data-dir` or `VIRLING_DATA`. Default `./virling-data`, and `/data` in the Docker image.

| File | Holds | Lose it and |
| --- | --- | --- |
| `virling.sqlite` (with `-wal` and `-shm`) | Accounts, sites, settings, encrypted provider keys, sync cursors | You lose sites and connections |
| `events.duckdb` | Events, imported payments, attribution | You lose tracking history. Payments can be imported again, but only 90 days back |
| `secret.key` | The 32-byte key for provider keys, mode `0600` | Every provider has to be reconnected |

## The secret key

On first start, Virling creates `secret.key` with random bytes. Provider keys in `virling.sqlite` are encrypted with it using AES-256-GCM.

To manage the key yourself, set `VIRLING_SECRET_KEY` or `--secret-key` to 64 hex characters:

```sh
openssl rand -hex 32
```

When it is set, `secret.key` is not read or created. Keep the value in a secret store. If it changes, stored keys cannot be decrypted and sync fails with "decryption failed, wrong secret key?". Reconnect each provider to fix that.

> [!WARNING]
> **Back up the key with the data.** A backup of `virling.sqlite` without the matching key cannot sync payments.

## Backups

The simplest safe backup stops the server, copies the folder and starts it again:

```sh
sudo systemctl stop virling
sudo tar czf virling-$(date +%F).tar.gz -C /var/lib virling
sudo systemctl start virling
```

The tracking script keeps sending events while the server is down, and those events are lost, so keep the stop short.

A documented procedure for backing up DuckDB while the server runs is <span class="na">Not available yet</span>.

## Retention and export

Events are kept forever. A retention setting, deleting old events and exporting data are <span class="na">Not available yet</span>. The files are standard SQLite and DuckDB databases, so you can open copies with the `sqlite3` and `duckdb` command-line tools.

## Upgrades

1. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md). Virling is in early development and releases can break things.
2. Back up the data folder.
3. Replace the binary, or change the image tag.
4. Start it. SQLite migrations run on start.

Downgrading after a migration ran is not supported. Restore the backup instead.

## First start

On first start, DuckDB downloads its ICU extension into `~/.duckdb/extensions` of the user running Virling, from `extensions.duckdb.org`. In the Docker image that is under `/data`. Allow that connection once, or the server cannot start. See [Outbound connections](../reference/outbound.md).
