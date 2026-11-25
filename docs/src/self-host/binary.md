# Run the binary

This page covers running a [release binary](../install/binary.md) on a server.

## Start it

```sh
export VIRLING_DATA=/var/lib/virling
export VIRLING_BASE_URL=https://stats.example.com
virling serve --app
```

`serve` is the default command, so `virling --app` alone would fail: flags belong to `serve`. Write `virling serve --app`.

On start it logs the address and the data folder:

```sh
$ virling serve --app
INFO virling: listening on http://0.0.0.0:8080, data in /var/lib/virling
```

## The data folder

Without `--data-dir` or `VIRLING_DATA`, the data folder is `./virling-data` relative to the directory you start it from. Set it explicitly on a server so it does not depend on the working directory. The folder is created if it is missing.

Run Virling as its own user that owns the data folder:

```sh
sudo useradd --system --home-dir /var/lib/virling --shell /usr/sbin/nologin virling
sudo mkdir -p /var/lib/virling
sudo chown virling: /var/lib/virling
```

## Listen address

The default is `0.0.0.0:8080`. Behind a reverse proxy on the same machine, listen on localhost only:

```sh
virling serve --app --addr 127.0.0.1:8080 --trust-proxy
```

## Keep it running

Use a process supervisor. See the [systemd](systemd.md) example. Stop the server with `SIGINT` (Ctrl+C) for a graceful shutdown.

## Upgrade

Replace the binary and restart. Migrations run on start. Read the [changelog](https://github.com/virling/virling/blob/main/CHANGELOG.md) first.

## Check that it works

1. **The version is the one you installed:**

   ```sh
   $ virling --version
   virling VIRLING_VERSION
   ```

2. **The server answers.** With Virling running, this prints `ok`:

   ```sh
   curl -fsS http://127.0.0.1:8080/health
   ```

3. **Sign-in is on.** This prints `200`. A `404` means the server runs without `--app`:

   ```sh
   curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/login
   ```

4. **A site is added.** Use the data folder the server uses. A new site is picked up within a minute:

   ```sh
   $ virling --data-dir /var/lib/virling add-site example.com --owner you@example.com
   Added example.com. Script tag:
   <script defer src="https://YOUR-VIRLING-HOST/v.js" data-site="example.com"></script>
   ```

Run `add-site` as the user that owns the data folder. Next: sign in with the owner email, and [check the email](smtp.md#check-that-it-works). To keep the server running after you log out, set up [systemd](systemd.md).

## Common questions

### How do I change the port?

Pass `--addr` or set `VIRLING_ADDR`. Both need an address and a port, such as `127.0.0.1:9000`: the port alone is refused with "invalid socket address syntax".

```sh
virling serve --app --addr 127.0.0.1:9000
```

A flag wins over its environment variable. Point your [reverse proxy](reverse-proxy.md) at the new port, and change `VIRLING_BASE_URL` if the port is part of your public address.

### Where are the logs?

Virling logs to standard output and writes no log file. Under systemd they are in the [journal](systemd.md#where-are-the-logs). `RUST_LOG` sets the level, and the default is `info`:

```sh
RUST_LOG=debug virling serve --app
```

### Why does it not start?

It prints the reason and exits:

| What it prints | Cause and fix |
| --- | --- |
| "unexpected argument '--app' found" | Flags belong to `serve`. Run `virling serve --app`. |
| "Address already in use" | Another program, or another Virling, has the port. Change the port. |
| "Permission denied" after "binding" | Ports below 1024 need extra rights. Use `8080` behind a [reverse proxy](reverse-proxy.md). |
| "Could not set lock on file" | Another Virling has this data folder open. Stop it first. |
| "unable to open database file" | The user running Virling cannot write to the data folder. Make it the owner. |
| "was previously applied but is missing in the resolved migrations" | The data folder was used by a newer version. Install that version, or restore a backup made with this one. |
| "smtp host undefined", "invalid port number" or "relative URL without a base" | `VIRLING_SMTP_URL` is not a valid URL. See [SMTP URL format](smtp.md#smtp-url-format). |

### How do I go back to the old version?

1. Stop Virling.
2. [Restore the backup](data.md#restore-the-binarys-data-folder) you made before the upgrade. Downgrading after a migration ran is not supported, so the old binary needs the old data. Events and changes since the backup are lost.
3. [Download](../install/binary.md#download-and-verify) the old version, replace the binary and start it.
