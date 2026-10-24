# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Email **contact@wintershogun.com** with the subject "Lyncost security". Include the Lyncost version,
your Linux distribution, and steps to reproduce. Please do not include your real financial data.

We aim to acknowledge reports within 2 business days and will keep you updated until the issue is
fixed. Once a fix is released, we are happy to credit you in the changelog if you wish.

## Supported versions

Security fixes are made for the latest release only. Update by running the install command again:

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh
```

## How Lyncost protects your data

- All records stay in a local SQLite database in `~/.local/share/com.lyncost.desktop/`.
- The 6-digit PIN is stored only as a bcrypt hash, with a lockout after repeated wrong attempts.
- The only network request the app makes is the update check (a small version file from GitHub).
- Every release is checked against the RustSec advisory database before it is published, and every
  download is verified against `SHA256SUMS` by the install script.
