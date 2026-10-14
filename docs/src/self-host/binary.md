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
