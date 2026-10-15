# Install

Virling ships as a Docker image and as release binaries. You can also build it from source with Rust.

| Option | Platforms | Best for |
| --- | --- | --- |
| [Docker](docker.md) | linux/amd64, linux/arm64 | Servers that already run containers |
| [Release binary](binary.md) | Linux x86_64, Linux arm64, macOS on Intel and Apple silicon | A plain VM or a systemd service |
| [Build from source](source.md) | Anywhere Rust 1.99 builds | Other platforms and contributors |

The quickest way to get the binary is `curl -fsSL https://virling.com/install.sh | sh`, see [Install script](binary.md#install-script).

Every option gives you the same `virling` binary. It runs `serve` by default and keeps its files in one [data folder](../self-host/data.md).

There is no release build for Windows <span class="na">Not available yet</span>. On Windows, use [Docker](docker.md) or [build from source](source.md).

After installing, continue with [Self-host](../self-host/docker.md) for running it in production, or the [Quickstart](../quickstart.md) for the whole setup.
