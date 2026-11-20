# Contributing to Bronze

Bug reports, ideas and pull requests are welcome.

## Issues

Use the bug report or feature request form when you [open an issue](https://github.com/bryceeppler/bronze/issues/new/choose).
For a bug, say which system you are on (macOS, Windows, or Linux with X11 or Wayland and which desktop) and how you installed Bronze.
Security problems go through [SECURITY.md](SECURITY.md) instead.

## Building

The README shows how to [build Bronze from source](README.md#build-from-source) on macOS, Windows and Linux.
The repository pins its Rust version in `rust-toolchain.toml`, and rustup installs it on the first build.
`cargo run -- --data /tmp/bronze-dev` keeps your test notes apart from your real ones.

## Before a pull request

```sh
./scripts/check.sh
```

That runs format, lint, tests, every scene rendered headless and the scripted runs, on Linux or macOS.
On Windows, run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
For a change to capture, shortcuts or windows, also run the real-window check for your system, listed under [Check](README.md#check) in the README.
GitHub runs the Linux build and tests on every pull request that is not a draft.

Keep a pull request to one change, and say in it what you ran.
Code explains itself through names, types and structure, so a comment is kept only for a constraint the code cannot show.

## Releasing

Releases are built on real machines, not in CI, all from one commit on `main`:

```sh
./scripts/mac-app.sh --package          # on a Mac with an Apple Development identity
./scripts/linux-package.sh              # on Linux with Docker
.\scripts\windows-package.ps1           # on Windows with the MSVC toolchain
```

Each writes its archive to `target/package/`.
Gather the three, write `SHA256SUMS.txt` with `sha256sum`, add the release to `CHANGELOG.md`, and publish the tag and the GitHub release with the archives and the checksums attached.
`./scripts/linux-app-check.sh bronze-<version>-linux-x86_64.tar.gz` installs the Linux archive into a throwaway home and drives it under Xvfb.

## License

By contributing, you agree that your contribution is released under the [MIT License](LICENSE).
