# Security Policy

Dotling handles sensitive data — encrypted dotfiles, vault passwords, and SSH/GPG keys. We take security seriously.

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.6.x   | Yes                |
| < 0.6   | No                 |

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, report privately via email:

- **Email:** [INSERT SECURITY EMAIL]
- **PGP key:** [INSERT PGP KEY FINGERPRINT OR LINK, optional]

Please include:

1. A description of the vulnerability.
2. Steps to reproduce or a proof of concept.
3. The potential impact.
4. Any suggested fix (if you have one).

### What to Expect

- **Acknowledgement** within 72 hours.
- A fix or mitigation plan within 14 days for confirmed issues.
- Credit in the release notes (unless you prefer to remain anonymous).

If we do not respond within 72 hours, feel free to follow up — the email may have been filtered.

## Scope

Vulnerabilities in scope include:

- Encryption/decryption logic (ChaCha20-Poly1305, Argon2id)
- Key derivation and vault handling
- Path traversal or symlink attacks in deployment
- Template injection in the template engine
- Hook execution (command injection, trust bypass)
- Secrets leaking into logs, error messages, or state files

## Out of Scope

- Social engineering
- Attacks requiring physical access to the user's machine
- Issues in third-party dependencies (report upstream, but do let us know)
