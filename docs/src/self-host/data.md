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

### Docker volume

These commands use the container and volume names from [Run with Docker](docker.md). Compose adds the project name to both, so look them up with `docker compose ps` and `docker volume ls`.

```sh
docker stop -s INT -t 60 virling
docker run --rm --user root --entrypoint tar \
  -v virling-data:/data:ro -v "$PWD":/backup \
  ghcr.io/virling/virling:VIRLING_VERSION \
  czf /backup/virling-$(date +%F).tar.gz -C /data .
docker start virling
```

`-s INT` asks for a clean stop, and an open dashboard can hold it up for about 25 seconds. A plain `docker stop` sends a signal Virling does not handle, so Docker kills it after its timeout. A backup taken then still restores, as long as it has every file in the folder, including the `-wal` ones.

Keep the archive private and off the server: it holds `secret.key`. If you set `VIRLING_SECRET_KEY`, the key is not in the archive, so back it up as well.

### Restore a Docker volume

Restore into a new volume, so the old one stays as it is:

```sh
docker volume create virling-restored
docker run --rm --user root --entrypoint tar \
  -v virling-restored:/data -v "$PWD":/backup:ro \
  ghcr.io/virling/virling:VIRLING_VERSION \
  xzf /backup/virling-YYYY-MM-DD.tar.gz -C /data
```

Replace the container with one that uses it: `docker rm -f virling`, then your usual `docker run` with `-v virling-restored:/data`. Use the image tag the backup was made with, or a newer one.

### Restore the binary's data folder

This puts back the archive from the `tar` command under [Backups](#backups), as the [systemd](systemd.md) example runs it. The old folder is kept beside it.

```sh
sudo systemctl stop virling
sudo mv /var/lib/virling /var/lib/virling.old
sudo tar xzpf virling-YYYY-MM-DD.tar.gz -C /var/lib
sudo chown -R virling: /var/lib/virling
sudo systemctl start virling
curl -fsS http://127.0.0.1:8080/health
```

On a new server, install the binary and create the user first. The settings in `/etc/virling/virling.env` are not in the archive.

### Test a backup

Extract it somewhere else and start it without `--app`:

```sh
mkdir restore-test
tar xzf virling-YYYY-MM-DD.tar.gz -C restore-test --strip-components=1
virling serve --data-dir restore-test --addr 127.0.0.1:8099
```

If `curl -fsS http://127.0.0.1:8099/health` prints `ok`, both databases opened. Without `--app` the copy does not sync payments or send reports, alerts and webhooks, which would otherwise go out twice. Clear `VIRLING_APP` from the environment if it is set there.

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

## Common questions

### How much disk does it use?

Check the folder, or the Docker volumes:

```sh
sudo du -sh /var/lib/virling
docker system df -v
```

`events.duckdb` grows with every event and is never trimmed. Deleting a site from the [admin panel](admin.md) removes its events, but the file may stay the same size: DuckDB reuses the free space for new events. The `-wal` and `.wal` files are normal while the server runs and are merged into the databases on a clean stop. With Docker, container logs are kept outside the volume and are not rotated by default, so set a limit, for example `--log-opt max-size=10m --log-opt max-file=3` on `docker run`.

### How do I move to another server?

Back up, stop the old server for good, restore on the new one, start it, then move the DNS. Follow the [Docker steps](docker.md#how-do-i-move-to-another-server), or the backup and [restore](#restore-the-binarys-data-folder) commands for the binary. Do not run both copies with `--app`: each would send the reports and webhooks. Two servers cannot share one data folder either, because DuckDB locks `events.duckdb` to one process.

### I lost secret.key. What now?

Your events, payments, sites and sign-ins are not encrypted with it and still work. If the file is gone, Virling creates a new one on start, and everything saved with the old one has to be entered again:

- **Payment providers:** use **Replace key** in each site's settings. The sync starts over from the backfill, and payments already stored are not duplicated. Until then the sync fails with "decryption failed, wrong secret key?", and provider webhooks are refused.
- **AI answers:** replace the keys saved in each site's settings. Keys set as environment variables are not affected.
- **Slack and webhooks:** save the Slack address again, and the [webhook](../reference/webhooks.md) too, which gets a new signing secret.

If you find the old key, stop Virling, put the file back and start it. Keys saved in between then have to be replaced. "secret key must be 32 bytes" on start means the file is damaged: restore it from a backup.

### I lost events.duckdb. Do payments come back?

Not on their own. Virling starts with an empty event database, but `virling.sqlite` still remembers how far each provider was synced. Use **Replace key** for each provider to [import the backfill again](docker.md#how-do-i-import-payments-again). Visits are gone, so those payments show as Unattributed. Restore a backup if you have one.
