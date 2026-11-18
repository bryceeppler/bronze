# bronze-sync

A self-hosted sync server for Bronze.
It stores one sealed board per vault in SQLite and never reads a note.

## Run

```sh
cargo build --release --manifest-path server/Cargo.toml
BRONZE_SYNC_DATA=./sync-data ./server/target/release/bronze-sync
```

```sh
docker build -t bronze-sync .
docker run -d -p 127.0.0.1:8787:8787 -v bronze-sync:/data bronze-sync
```

`bronze-sync health` exits 0 when the server configured by the same env vars answers `GET /health`.

## Env vars

| Var | Default | Meaning |
| --- | --- | --- |
| `BRONZE_SYNC_DATA` | required | Folder for `bronze-sync.sqlite3`, created if missing |
| `BRONZE_SYNC_LISTEN` | `127.0.0.1:8787` | Address and port to listen on |
| `BRONZE_SYNC_MAX_BYTES` | `8388608` | Largest sealed board, in bytes |
| `BRONZE_SYNC_MAX_VAULTS` | `100` | Most vaults this server creates |
| `BRONZE_SYNC_SETUP_TOKEN` | unset | When set, creating a vault needs it in the `Bronze-Setup-Token` header |

## Check

```sh
./scripts/server-check.sh binary         # builds the binary and drives it with curl
./scripts/server-check.sh docker <image> # the same cases against a built image
```

The server speaks plain HTTP.
Put it behind a reverse proxy for TLS.
