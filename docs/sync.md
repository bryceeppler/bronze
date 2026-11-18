# How sync works

This note describes Bronze's sync for people who want to know what it does with their notes, and for anyone writing a compatible client or server.
Running a server is covered in [self-hosting.md](self-hosting.md).

## In plain words

Sync is off until you turn it on, and with it off Bronze opens no network connection.

Every machine keeps your whole board, and the server keeps one copy of it.
That copy is encrypted on your machine before it leaves, with a key the server never sees.
The key comes from your sync code, which Bronze makes when you start a vault.
Anyone with the sync code can read and change your notes, and nobody without it can, the server's owner included.
There are no accounts or passwords, so a lost sync code cannot be reset.

When you make a change, Bronze fetches the server's copy, merges it with its own board, and uploads the result if anything is new.
When nothing changes, it waits on the server, which answers as soon as another machine uploads.
A change on one machine usually shows on the others within a second or two.

Merging never asks you anything.
Each note and section remembers when each of its parts last changed, and the newest change wins.
A note deleted on any machine stays deleted everywhere.
Because every machine holds the whole board and every sync is a full merge, a server that was wiped or restored from an old backup is filled again by the next machine that syncs.

## The sync code and the Sync Key

A sync code is the server address and the Sync Key joined by `#`:

```
https://sync.example.com#bronze1-000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f-630dcd29
```

That key is a made-up example.

- The Sync Key is 32 random bytes, written as `bronze1-`, 64 lowercase hex characters, `-`, and an 8 character check.
- The check is the first 4 bytes of SHA-256 of the 32 bytes, in hex, so a mistyped or cut code is caught before Bronze uses it.
- The server address must be `https://`, or plain `http://` for `localhost` and `127.0.0.1` only.
  It may carry a port and a path, and Bronze adds its routes after the path.

Bronze expands the 32 bytes with HKDF-SHA256, salt `bronze/v1`, into three 32 byte values:

| Info string | Becomes | Sent to the server |
| --- | --- | --- |
| `vault` | the vault id | yes, as 64 hex characters in the URL |
| `authentication` | the access token | yes, as `Authorization: Bearer <64 hex>` |
| `encryption` | the XChaCha20-Poly1305 key | never |

The server stores the SHA-256 of the token, not the token, and compares it in constant time.
Neither the vault id nor the token reveals the key or the encryption key.

Each machine keeps the server address and the key in `sync.json` in its notes folder, written with mode `0600` on Unix.
Choosing **Stop Syncing** turns sync off at once and removes that file.
Removing the file by hand turns sync off the next time Bronze starts; a running Bronze keeps syncing until then.

## The sealed snapshot

What the server stores for a vault is one sealed snapshot:

```
0x01 | 24 byte random nonce | XChaCha20-Poly1305 ciphertext and 16 byte tag
```

- `0x01` is the format.
  A machine that finds a higher format byte stops uploading and asks you to update Bronze, so an older Bronze never overwrites a newer one's data.
- The nonce is fresh for every upload.
- The associated data is `bronze/snapshot/v1:<vault id in hex>`, so a snapshot cannot be moved to another vault unnoticed.
- A snapshot that fails to decrypt is reported as damaged, and Bronze never overwrites it.

The plaintext is JSON:

```json
{
  "notes": {
    "<32 hex note id>": {
      "text": "Ask for a rollout plan",
      "done": false,
      "done_at": { "time": 1795000000000, "device": "<16 hex device id>" },
      "section": "<32 hex section id>",
      "rank": "<fractional index>",
      "placed_at": { "time": 1795000000000, "device": "<16 hex device id>" }
    },
    "<32 hex id of a deleted note>": null
  },
  "sections": {
    "<32 hex section id>": {
      "name": "Research",
      "named_at": { "time": 1795000000000, "device": "<16 hex device id>" },
      "created": { "time": 1795000000000, "device": "<16 hex device id>" }
    }
  }
}
```

`section` is `null` for a note outside any section, and a deleted section is `null` like a deleted note.

## The wire

The server speaks plain HTTP and leaves TLS to a reverse proxy.

| Request | Answers |
| --- | --- |
| `GET /health` | `200` |
| `GET /v1/vaults/{vault}` | `200` with the snapshot and `ETag`, `404` for no such vault, `401` for a wrong token |
| `GET` with `If-None-Match: "<version>"` and `Prefer: wait=<seconds>` | holds the request until the version changes, then `200`; `304` with the current `ETag` when the wait ends |
| `PUT /v1/vaults/{vault}` with `If-Match: "<version>"` | `204` with the new `ETag`; `412` with the current `ETag` when the version is stale, or with none when the vault does not exist |
| `PUT /v1/vaults/{vault}` with `If-None-Match: *` | `201` with the new `ETag`; `412` when the vault exists; `403` when the server holds all the vaults it allows |
| `PUT` with neither header | `428` |
| `PUT` larger than `BRONZE_SYNC_MAX_BYTES` | `413` |
| a malformed vault id | `404` |
| a missing or malformed token | `401` |
| a malformed `If-Match`, `If-None-Match` or `ETag` value, or both precondition headers at once | `400` |

- `{vault}` and the token are 64 lowercase hex characters.
- A version is 32 lowercase hex characters in a strong `ETag`, such as `"e401989c3a03b182322423abeca307c0"`.
  It is random on every write, never a counter, so a restored server never reuses a version a machine has already seen.
- The server caps the wait at 55 seconds.
  Bronze asks for 25, to stay under the idle timeout of common proxies.
- A `2xx` answer to a `PUT` means the snapshot is on disk.
- The body of a `403` is a sentence for a person, and Bronze shows it as the server's reason.

## The client's loop

One background thread per Bronze does this:

1. Fetch the snapshot, or note that the vault does not exist.
2. Decrypt it and merge it into the local board.
3. If the merged board differs from the server's copy, seal it and upload it with `If-Match`, or with `If-None-Match: *` when there is no vault yet.
   A `412` means another machine uploaded first, so go back to step 1.
4. If they are the same, wait on the server with `If-None-Match` and `Prefer: wait=25`, and go back to step 3 at once when you edit something.

After a failure it retries after 1 second, doubling up to 60 seconds, and an edit retries at once.
Starting a new vault and joining one are the same operation: the only difference is that starting makes a new key.

## How merges resolve

Every change carries a stamp: the wall clock in milliseconds and the id of the machine that made it.
A stamp is never older than the newest stamp the machine has seen, so a machine's own changes always move forward.
Stamps order by time, then by machine id, so two machines never tie.

- **Each field, the last writer wins.**
  A note's check mark and its place (section and position) are separate fields, so checking a note on one machine and moving it on another keeps both changes.
  A section's name is a field of its own too.
- **A delete beats everything.**
  A deleted note or section stays deleted, even against a check or a move made at the same time on another machine.
- **A note's text never changes.**
  Merging notes with `Cmd+J` or `Ctrl+J` makes a new note with the joined text and deletes the picked ones, so a delete of one picked note on another machine never loses the others' text.
- **Order is by position.**
  Positions are fractional indexes, and two notes at the same position order by id.
- **Sections with the same name show as one**, under the oldest, ignoring case.
  A note in a deleted section shows outside any section.
- The section new notes go to stays on each machine and is not synced.

Merging is a pointwise maximum, so merging in any order, any number of times, gives the same board.

## Limits

These follow from the design, and they are worth knowing before you rely on it.

- **The server learns some things.**
  It sees the vault id, the size of each upload, when each machine syncs, and the IP addresses machines connect from.
  It never sees a note, a section name, how many notes there are beyond what the size implies, or the key.
- **A lost sync code is final for the server's copy.**
  Without it, the copy on the server cannot be read by anyone.
  Each machine still keeps every note, and Bronze can show its sync code again from **Sync…** as long as one machine still syncs.
- **A clock that runs ahead wins ties.**
  Last writer wins by wall clock time, so a machine whose clock is minutes fast wins edits to the same field made in those minutes.
- **Deleted notes leave their ids behind.**
  Each deletion keeps a 16 byte id in the board for good, so a delete can never be undone by an old copy.
- **Every upload sends the whole board.**
  A board of a few thousand notes is a few hundred kilobytes, well under the server's 8 MiB default.
- **There is no history.**
  The server keeps only the latest snapshot, and a machine keeps only its current board, so a change cannot be rolled back through sync.
  Back up the server, or a machine's `notes.json`, if you want older copies.
