FROM ubuntu:22.04 AS build
RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential ca-certificates curl git pkg-config libfontconfig-dev libxkbcommon-dev \
  && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
WORKDIR /src
COPY rust-toolchain.toml .
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none \
  && rustup toolchain install
COPY . .
ARG SOURCE_DATE_EPOCH
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target --mount=type=cache,target=/src/server/target <<'BUILD'
set -eu
cargo build --release --locked -p bronze
cargo build --release --locked --manifest-path server/Cargo.toml
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
name=bronze-$version-linux-x86_64
mkdir -p /stage/$name/icons /out
cp target/release/bronze server/target/release/bronze-sync scripts/linux-app.sh LICENSE /stage/$name/
cp -r assets/brand/icon/linux/dark/hicolor /stage/$name/icons/
tar -C /stage --sort=name --owner=0 --group=0 --numeric-owner --mtime=@$SOURCE_DATE_EPOCH -czf /out/$name.tar.gz $name
BUILD

FROM scratch
COPY --from=build /out/ /
