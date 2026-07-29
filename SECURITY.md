# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security issue in OMP Desktop (for example token leakage, unsafe
process spawning, or local secrets exposure), please report it privately:

- Open a GitHub Security Advisory on [Po1nt9/omp-desktop](https://github.com/Po1nt9/omp-desktop), or
- Contact the maintainer via GitHub.

Please include:

- A clear description of the issue
- Steps to reproduce
- Impact assessment if known

Do **not** open a public issue for sensitive vulnerabilities until a fix is available.

## Local security notes

- **API keys** prefer the **OS secret store**:
  - macOS: Keychain
  - Windows: Credential Manager
  - Linux: FreeDesktop Secret Service (when available)
  - Fallback: `secrets.json` under the app data root with mode `0600` when the OS store is unavailable
- Do not commit secrets, tokens, or local configuration files.
- Support zip / Doctor export never include `secrets.json`, OS keychain material, or raw API keys (redacted logs and chat only).
