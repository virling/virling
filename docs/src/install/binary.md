# Release binary

Each release on [GitHub](https://github.com/virling/virling/releases) has a tarball per platform and a SHA-256 checksum next to it.

| Target | Platform | File |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Linux x86_64 | `virling-v0.1.1-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | Linux arm64 | `virling-v0.1.1-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` | macOS on Intel | `virling-v0.1.1-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | macOS on Apple silicon | `virling-v0.1.1-aarch64-apple-darwin.tar.gz` |

The name is always `virling-vX.Y.Z-<target>.tar.gz`, with the checksum at the same name plus `.sha256`. Each tarball holds one folder with the `virling` binary and the `LICENSE` file.

Builds for Windows are <span class="na">Not available yet</span>. Use [Docker](docker.md) or [build from source](source.md).

## Install script

```sh
curl -fsSL https://virling.com/install.sh | sh
```

The script picks the right tarball for your system, checks its SHA-256 and installs `virling` to `~/.local/bin`. Pass options after `sh -s --`:

| Option | Effect |
| --- | --- |
| `--system` | Install to `/usr/local/bin` with sudo |
| `--systemd` | With `--system` on Linux, also set up the [systemd service](../self-host/systemd.md) without asking |
| `--no-systemd` | Never offer the systemd service |
| `--version X.Y.Z` | Install a specific release instead of the latest |
| `--dir PATH` | Install to another folder |
| `--dry-run` | Print what it would do and change nothing |

With `--system` on Linux and a terminal attached, it asks before setting up the service. It never sets one up silently.

## Download and verify

```sh
V=0.1.1
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
virling 0.1.1
```

Next: [run the binary in production](../self-host/binary.md), optionally as a [systemd service](../self-host/systemd.md).
