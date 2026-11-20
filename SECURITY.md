# Security

## Report a vulnerability

Report it privately through [GitHub's private vulnerability reporting](https://github.com/bryceeppler/bronze/security/advisories/new).
Only the maintainer sees the report.
Please do not open a public issue for it.

Say what an attacker can do, what they need first, and how to reproduce it.
You will get a reply within a week.
Once a fix ships, the advisory is published with credit to you, unless you would rather stay unnamed.

## What is in scope

- Sync: the encryption, the sync code, and anything that lets `bronze-sync` or someone on the network read or change notes.
- The app: anything that lets another program or a web page make Bronze run a command, read notes or capture text.
- The release downloads and the scripts that install them.

Bronze reads the keyboard for its Shift gestures and the selected text when you capture.
On Linux under Wayland that needs the `input` group, which lets every program you run read keystrokes; the README says so, and it is not a vulnerability in Bronze.

## Supported versions

Fixes go into the latest release only.
