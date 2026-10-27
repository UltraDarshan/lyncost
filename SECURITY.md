# Security Policy

## Reporting a Vulnerability

Please **do not** open public GitHub issues for sensitive security problems.

Email **contact@wintershogun.com** with the subject **"Lyncost Security Disclosure"**.

Please include:
- A description of the issue and potential impact
- Your operating system and Lyncost version
- Proof of concept or reproduction steps (*without real financial data*)

We acknowledge reports within 48 hours and will keep you informed as we investigate and deploy a fix. Once resolved, we are happy to credit you in our release notes if desired.

## Supported Versions

Security patches and dependency updates are provided for the latest stable release:

| Version | Supported |
| :--- | :--- |
| 1.1.x | Yes |
| < 1.1.0 | No |

Update your installation to the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh
```

## Security & Architecture Principles

- **Zero Cloud Footprint:** All ledgers and personal transactions reside in an ACID-compliant local SQLite database.
- **PIN Protection:** The 6-digit master PIN is hashed with salted bcrypt and enforced with exponential lockout timers after repeated failed attempts.
- **Air-Gapped Network Policy:** Lyncost makes no outbound telemetry, ad network, or analytics calls. Network activity is limited exclusively to checking optional GitHub release updates.
- **Signed Binary Distribution:** Every release build is verified against SHA256 checksums and scanned against the RustSec Advisory Database.
