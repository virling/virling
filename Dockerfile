FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin virling

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data virling \
    && mkdir /data && chown virling /data
COPY --from=builder /src/target/release/virling /usr/local/bin/virling
USER virling
VOLUME /data
ENV VIRLING_DATA=/data VIRLING_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["virling"]
CMD ["serve"]
