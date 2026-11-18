# Self-hosting bronze-sync

`bronze-sync` is the server that keeps Bronze's notes the same on your machines.
You run it yourself, and it stores one encrypted copy of your notes that it cannot read.
There are no accounts: a sync code made by Bronze is the only thing that opens your notes.
How the encryption and the merging work is in [sync.md](sync.md).

This guide runs the server on Linux.
The same commands work on macOS, where the notes say so.

## What you need

- A machine that stays on and that your other machines can reach, such as a home server or a small VPS.
- Either Rust and a C compiler, to build the binary, or Docker, to build an image.
- For machines other than the server itself: a domain name pointing at the server, and a reverse proxy that serves it over https.
  Bronze accepts `https://` addresses, and plain `http://` only for `localhost` and `127.0.0.1`.

Get the source first:

```sh
git clone https://github.com/bryceeppler/bronze.git
cd bronze
```

Every command below runs from that folder.

## Run it from a binary

Install Rust with [rustup](https://rustup.rs) and a C compiler.
On Debian and Ubuntu that is:

```sh
sudo apt install build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On macOS, `xcode-select --install` gives you the C compiler, and rustup installs the same way.
The repository pins its Rust version, so the first build downloads that version by itself.

Build and start the server:

```sh
cargo build --release --locked --manifest-path server/Cargo.toml
mkdir -p ~/bronze-sync-data
BRONZE_SYNC_DATA=~/bronze-sync-data ./server/target/release/bronze-sync
```

It prints `bronze-sync listening on 127.0.0.1:8787, data in /home/you/bronze-sync-data` and keeps running in the foreground.
Stop it with `Ctrl+C`.
To keep it running after you log out, see [Run it as a service](#run-it-as-a-service).

## Run it with Docker

```sh
docker build -t bronze-sync .
docker run -d --name bronze-sync --restart unless-stopped -p 127.0.0.1:8787:8787 -v bronze-sync:/data bronze-sync
```

The image runs as a user without privileges and keeps its database in the `bronze-sync` volume, mounted at `/data`.
`-p 127.0.0.1:8787:8787` lets only this machine reach it, which is what a reverse proxy on the same machine needs.
Docker restarts it after a reboot.

## Check that it is working

```sh
curl http://127.0.0.1:8787/health
```

It answers `ok`, and `curl -i` shows the `200 OK` status line above it.

The server can check itself, with the same env vars it runs with:

```sh
BRONZE_SYNC_DATA=~/bronze-sync-data ./server/target/release/bronze-sync health && echo healthy
```

The Docker image runs that check every 30 seconds, and `docker ps` shows `(healthy)` beside the container.
You can also run it by hand:

```sh
docker exec bronze-sync bronze-sync health && echo healthy
```

## Settings

The server reads four env vars and nothing else.

| Var | Default | Meaning |
| --- | --- | --- |
| `BRONZE_SYNC_DATA` | required | Folder for `bronze-sync.sqlite3`, created if missing |
| `BRONZE_SYNC_LISTEN` | `127.0.0.1:8787` | IP address and port to listen on |
| `BRONZE_SYNC_MAX_BYTES` | `8388608` | Largest encrypted board, in bytes |
| `BRONZE_SYNC_MAX_VAULTS` | `16` | Most vaults this server creates |

The Docker image sets `BRONZE_SYNC_DATA=/data` and `BRONZE_SYNC_LISTEN=0.0.0.0:8787`.
Pass others with `-e`, for example `docker run -e BRONZE_SYNC_MAX_VAULTS=1 ...`.

A board of a few thousand notes is well under the 8 MiB default.

## Limit who can create vaults

Anyone who can reach the server can create a vault until it holds `BRONZE_SYNC_MAX_VAULTS` of them.
A vault is one set of notes shared by your machines, so one person needs one vault.
Set the limit to the number of vaults you use:

```sh
BRONZE_SYNC_DATA=~/bronze-sync-data BRONZE_SYNC_MAX_VAULTS=1 ./server/target/release/bronze-sync
```

Once the server is full, it turns down new vaults, and Bronze says the server declined.
Machines that already sync keep working, because they never need to create their vault again.
A stranger who reaches the server cannot read your notes or change your vault without your sync code.

## Serve it over https

Bronze on another machine needs an `https://` address.
Put a reverse proxy in front of the server that holds the certificate.
[Caddy](https://caddyserver.com) gets one from Let's Encrypt by itself.

This part needs a domain name, here `sync.example.com`, whose DNS points at your server, with ports 80 and 443 open to the internet.
Install Caddy from its [install page](https://caddyserver.com/docs/install), then make `/etc/caddy/Caddyfile` this:

```
sync.example.com {
	reverse_proxy 127.0.0.1:8787
}
```

Reload it with `sudo systemctl reload caddy`, then check from any machine:

```sh
curl https://sync.example.com/health
```

Your server address in Bronze is then `https://sync.example.com`.

If you use another proxy, give it two settings that Caddy has by default.
Requests wait up to 55 seconds for a change, so allow responses to take at least 60 seconds.
Request bodies can be as large as `BRONZE_SYNC_MAX_BYTES`, so do not cap them lower.
In nginx those are `proxy_read_timeout 75s;` and `client_max_body_size 8m;`.

## Run it as a service

On a Linux server with systemd, install the binary and a unit so it starts at boot:

```sh
sudo install -m 755 server/target/release/bronze-sync /usr/local/bin/bronze-sync
sudo cp docs/bronze-sync.service /etc/systemd/system/bronze-sync.service
sudo systemctl daemon-reload
sudo systemctl enable --now bronze-sync
```

The unit, [bronze-sync.service](bronze-sync.service), runs the server as a throwaway system user and keeps its data in `/var/lib/bronze-sync`:

```ini
[Unit]
Description=bronze-sync, the sync server for Bronze
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/bronze-sync
Environment=BRONZE_SYNC_DATA=/var/lib/bronze-sync
Environment=BRONZE_SYNC_LISTEN=127.0.0.1:8787
DynamicUser=yes
StateDirectory=bronze-sync
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

`systemctl status bronze-sync` shows whether it runs, and `journalctl -u bronze-sync` shows what it printed.
On macOS, run the binary from a launchd agent, or use Docker.

## Connect your machines

On the first machine:

1. In Bronze, open the menu (the `...` button) and choose **Sync…**.
2. Enter your server address, such as `https://sync.example.com`, and choose **Start New Vault**.
3. Choose **Copy Code**, and keep the sync code somewhere safe, such as a password manager.

The sync code looks like `https://sync.example.com#bronze1-` followed by 73 more characters.
It is the only way into your notes.
There is no account or password, so nobody can reset it, the server's owner included.

On every other machine:

1. Open the menu and choose **Sync…**.
2. Paste the sync code and choose **Join Vault**.

Notes already on that machine are added to the vault, so nothing is lost.
A new note shows on the other machines within a second or two.

When a machine cannot reach the server, its new notes stay on it and sync once the server is back.
The **Sync…** item in the menu says whether a machine is synced, and the sheet it opens says what is wrong when it is not.

To stop syncing a machine, open **Sync…** and choose **Stop Syncing**.
Every note stays on that machine, and the other machines keep syncing.

Bronze keeps the server address and the key in `sync.json` beside its notes, readable only by you.
Its notes folder is `~/.local/share/bronze` on Linux, `~/Library/Application Support/ca.bryce.bronze` on macOS, and `%APPDATA%\bryce\bronze\data` on Windows.

## Where the data lives

The server keeps everything in one SQLite database, `bronze-sync.sqlite3`, in `BRONZE_SYNC_DATA`.
While it runs, SQLite also keeps `bronze-sync.sqlite3-wal` and `bronze-sync.sqlite3-shm` beside it.
In Docker that folder is the `bronze-sync` volume.

The database holds, for each vault, its id, a hash of its access token, the encrypted board and a version.
Without a sync code nobody can read the boards, the server's owner included.

## Back up and restore

Stop the server, copy its data, then start it again.
For the binary:

```sh
cp -a ~/bronze-sync-data ~/bronze-sync-backup
```

For Docker, this saves the volume as a tar file that keeps the owner the server runs as:

```sh
docker stop bronze-sync
docker cp bronze-sync:/data - > bronze-sync-backup.tar
docker start bronze-sync
```

To restore, replace the whole data folder rather than copying files into it.
A running server leaves `-wal` and `-shm` files beside the database, and SQLite would apply those over the restored copy.
For the binary, stop the server, then:

```sh
rm -rf ~/bronze-sync-data
cp -a ~/bronze-sync-backup ~/bronze-sync-data
```

For Docker, put the backup in a fresh volume before the container first starts:

```sh
docker rm -f bronze-sync
docker volume rm bronze-sync
docker create --name bronze-sync --restart unless-stopped -p 127.0.0.1:8787:8787 -v bronze-sync:/data bronze-sync
docker cp -a - bronze-sync:/ < bronze-sync-backup.tar
docker start bronze-sync
```

A backup matters less than it seems, because every synced machine holds all of your notes.
After a restore, each machine merges what it has into the server's older copy, so nothing written since the backup is lost.
After the server loses its data entirely, the machines create the vault again and put every note back the next time they sync.
A note deleted on one machine stays deleted everywhere, even when the server's copy is older than the delete.

What a backup cannot replace is your sync code.
If you lose it while your machines still sync, open **Sync…** on one of them and copy it again.

## Update

```sh
git pull
cargo build --release --locked --manifest-path server/Cargo.toml
```

Then restart the server, or for Docker rebuild the image and recreate the container:

```sh
docker build -t bronze-sync .
docker rm -f bronze-sync
docker run -d --name bronze-sync --restart unless-stopped -p 127.0.0.1:8787:8787 -v bronze-sync:/data bronze-sync
```

The data stays in the volume.
Machines retry on their own while the server restarts.
