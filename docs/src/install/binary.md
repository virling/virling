# Release binary

Each release on [GitHub](https://github.com/virling/virling/releases) has a tarball per platform and a SHA-256 checksum next to it.

| Target | Platform | File |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Linux x86_64 | `virling-vVIRLING_VERSION-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | Linux arm64 | `virling-vVIRLING_VERSION-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` | macOS on Intel | `virling-vVIRLING_VERSION-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | macOS on Apple silicon | `virling-vVIRLING_VERSION-aarch64-apple-darwin.tar.gz` |

The name is always `virling-vX.Y.Z-<target>.tar.gz`, with the checksum at the same name plus `.sha256`. Each tarball holds one folder with the `virling` binary and the `LICENSE` file.

Builds for Windows are <span class="na">Not available yet</span>. Use [Docker](docker.md) or [build from source](source.md).

## Install script

```sh
curl -fsSL https://virling.com/install.sh | sh
```

The script picks the right tarball for your system and checks its SHA-256. On Linux with systemd it installs `virling` to `/usr/local/bin`, creates a `virling` user, the data folder `/var/lib/virling` and `/etc/virling/virling.env`, sets up the [systemd service](../self-host/systemd.md), starts it, waits for it to answer and prints the URL. It uses sudo only when you are not root. Running it again upgrades the binary and restarts the service, and keeps your environment file. Elsewhere, such as macOS, it installs `virling` to `~/.local/bin`. It sends nothing anywhere except the downloads from GitHub.

Pass options after `sh -s --`:

| Option | Effect |
| --- | --- |
| `--no-systemd` | On Linux, install the binary to `~/.local/bin` only, without the service |
| `--system` | Install to `/usr/local/bin` with sudo |
| `--version X.Y.Z` | Install a specific release instead of the latest |
| `--dir PATH` | Install to another folder, without the service |
| `--dry-run` | Print what it would do and change nothing |

## Download and verify

```sh
V=VIRLING_VERSION
T=x86_64-unknown-linux-gnu
curl -LO https://github.com/virling/virling/releases/download/v$V/virling-v$V-$T.tar.gz
curl -LO https://github.com/virling/virling/releases/download/v$V/virling-v$V-$T.tar.gz.sha256
shasum -a 256 -c virling-v$V-$T.tar.gz.sha256
tar xzf virling-v$V-$T.tar.gz
sudo install virling-v$V-$T/virling /usr/local/bin/virling
```

`shasum -a 256 -c` prints `OK` when the file matches. On Linux without `shasum`, use `sha256sum -c`.

The Linux builds link against glibc 2.35 or newer, built on Ubuntu 22.04. They run on Debian 12, Ubuntu 22.04 and newer, Fedora and similar distributions.

## Check it runs

```sh
$ virling --version
virling VIRLING_VERSION
```

Next: [run the binary in production](../self-host/binary.md), optionally as a [systemd service](../self-host/systemd.md).
