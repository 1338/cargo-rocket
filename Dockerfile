FROM rust:1.89-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY src src
COPY static static
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates openssh-client && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --create-home service && mkdir -p /data/files && chown -R service:service /data
COPY --from=build /app/target/release/cargo-rocket /usr/local/bin/cargo-rocket
USER service
ENV BIND_ADDR=0.0.0.0:3000 DATABASE_PATH=/data/upload-tool.db
WORKDIR /data
EXPOSE 3000
CMD ["cargo-rocket"]
