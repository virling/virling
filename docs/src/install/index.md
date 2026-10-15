# Install

Virling ships as a Docker image and as release binaries. You can also build it from source with Rust.

| Option | Platforms | Best for |
| --- | --- | --- |
| [Docker](docker.md) | linux/amd64 | Servers that already run containers |
| [Release binary](binary.md) | Linux x86_64, Linux arm64, macOS on Apple silicon | A plain VM or a systemd service |
| [Build from source](source.md) | Anywhere Rust 1.99 builds | Other platforms and contributors |

> [!WARNING]
> **Payments need the next release.** v0.1.0, the current release, has tracking, sign-in and onboarding. Payment sync, attribution, the dashboard, `VIRLING_BASE_URL` and `VIRLING_SECRET_KEY` ship in the next release. Until it is out, [build from source](source.md) from `main` to use them.

Every option gives you the same `virling` binary. It runs `serve` by default and keeps its files in one [data folder](../self-host/data.md).

There is no arm64 Docker image, and no release build for Intel macOS or Windows <span class="na">Not available yet</span>. On those platforms, [build from source](source.md).

After installing, continue with [Self-host](../self-host/docker.md) for running it in production, or the [Quickstart](../quickstart.md) for the whole setup.
