# Signing requirements

Code signing is the **hard blocker** for Plan 9. Without it, builds still
produce and publish installers, but users hit platform warnings:

| Platform | Unsigned consequence | User workaround |
|----------|---------------------|-----------------|
| macOS | Gatekeeper blocks launch ("unidentified developer") | Right-click → Open, or `xattr -dr com.apple.quarantine /Applications/OMP\ Desktop.app` |
| Windows | SmartScreen "Windows protected your PC" | "More info" → "Run anyway" |
| Linux | None (AppImage is unsigned by design) | — |

This document lists every secret the release pipeline can consume and how to
obtain each. The build degrades gracefully: omit any subset of these and the
matching signing step is skipped (see `release.yml`).

## Secret inventory

### macOS (Apple Developer ID + notarization)

| Secret | Purpose | Source |
|--------|---------|--------|
| `APPLE_CERTIFICATE` | base64 of the Developer ID Application `.p12` bundle | Exported from Keychain Access after creating the cert |
| `APPLE_CERTIFICATE_PASSWORD` | password protecting that `.p12` | Set when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | the cert's Common Name, e.g. `Developer ID Application: Your Name (TEAMID)` | Shown in Keychain Access / `security find-identity` |
| `APPLE_ID` | Apple ID of a notarization account | Your Apple Developer account email |
| `APPLE_PASSWORD` | app-specific password for notarytool | Generated at appleid.apple.com → Sign-In & Security → App-Specific Passwords |
| `APPLE_TEAM_ID` | 10-character Team ID (Organizational Unit) | Shown on the Developer ID cert details, or App Store Connect → Membership |

**Cost:** Apple Developer Program membership, **USD $99/year**.

**Obtain:**
1. Enroll at <https://developer.apple.com/programs/>.
2. In Certificates, Identifiers & Profiles → Certificates → create a
   **Developer ID Application** certificate (signs `.app` bundles for
   distribution outside the App Store).
3. Create an **App Store Connect API key** (`.p8`) for automated notarization —
   this is the modern `notarytool` path (preferred over `APPLE_ID` +
   `APPLE_PASSWORD`, which is the legacy app-specific-password path).
4. Export the `.p12` from Keychain Access and base64-encode it:
   ```sh
   base64 -i developer-id.p12 -o APPLE_CERTIFICATE.b64
   ```

> ⚠️ **Do not pass empty `APPLE_*` secrets to the build.** `tauri build`
> treats an empty `APPLE_CERTIFICATE` as "import this cert" and fails with
> "failed to import keychain certificate". Omit the secret entirely instead.
> This is handled in `release.yml` by only forwarding non-empty values.

### Windows (Authenticode)

| Secret | Purpose | Source |
|--------|---------|--------|
| `TAURI_SIGNING_PRIVATE_KEY` | (updater, not Authenticode) — see below | Tauri signer keypair |
| Windows cert | Authenticode signature on the installer/exe | A CA-issued code signing certificate |

> The release pipeline (`release.yml`) currently wires **Tauri updater**
> signing but does **not** yet invoke `signtool` for Windows Authenticode.
> Adding Windows signing requires: (a) an Authenticode cert, (b) a
> `sign-windows.sh` step using `signtool sign`, (c) wiring the cert thumbprint
> into `tauri.conf.json` `windows.certificateThumbprint`. This is tracked as
> Plan 9 remaining work once a cert is in hand.

**Authenticode certificate types:**

| Type | SmartScreen reputation | Cost | Notes |
|------|------------------------|------|-------|
| OV (Standard) | Builds reputation over time as users run it | ~$100–$300/yr | Cheapest; newly-issued OV certs still trigger SmartScreen until reputation accrues |
| EV (Extended Validation) | Immediate SmartScreen trust (no warning) | ~$300–$700/yr | Hardware token required; harder to automate in CI |

Recommendation for an open-source project: start with **OV** and accept the
reputation warm-up period, unless corporate distribution demands EV.

#### Free alternative: SignPath Foundation (open-source projects)

[SignPath.io](https://about.signpath.io/express/open-source) offers **free
Authenticode code signing** for non-commercial open-source projects, backed by
the CA [Certum](https://certum.eu). No hardware token, fully cloud-based, and
integrates with GitHub Actions via
[`signpath/github-action-submit-signing-request`](https://github.com/SignPath/github-action-submit-signing-request).

**How to enable:**
1. Apply at <https://about.signpath.io/express/open-source> with the GitHub repo URL.
2. After approval, add `SIGNPATH_API_TOKEN`, `SIGNPATH_ORG_ID`, and
   `SIGNPATH_PROJECT_SLUG` as repo secrets.
3. Add a post-build step in `release.yml`:
   ```yaml
   - name: Sign Windows installer with SignPath
     if: matrix.platform == 'windows-latest' && env.SIGNPATH_API_TOKEN != ''
     uses: signpath/github-action-submit-signing-request@v1
     with:
       api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
       organization-id: ${{ secrets.SIGNPATH_ORG_ID }}
       project-slug: ${{ secrets.SIGNPATH_PROJECT_SLUG }}
       signing-policy-slug: release-signing
       artifact-configuration-slug: nsis-installer
       github-artifact-id: ${{ steps.build.outputs.artifact-id }}
       wait-for-completion: true
       output-artifact-directory: signed/
   ```

**Cost:** Free for qualifying open-source projects.
**Effect:** Eliminates Windows SmartScreen warnings immediately (Certum OV cert
with SignPath trust).

### Updater (minisign keypair — cross-platform)

| Secret | Purpose | Source |
|--------|---------|--------|
| `OMP_DESKTOP_UPDATER_PUBLIC_KEY` | minisign public key embedded in the app | Output of `pnpm tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY` | minisign private key (signs updater archives) | Output of `pnpm tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password for the private key (empty string OK) | Chosen at generation |

**Cost:** Free. Generate once:

```sh
pnpm tauri signer generate -w ~/.tauri/omp-desktop.key
# CWt... (public key)  → OMP_DESKTOP_UPDATER_PUBLIC_KEY (repo secret)
# private key file    → TAURI_SIGNING_PRIVATE_KEY (repo secret)
```

These are **independent of** Apple/Windows code-signing: updater signing
verifies the *update archive integrity*, while code-signing satisfies the *OS
trust* layer. You can enable updater signing before code-signing is set up, but
macOS users would still hit Gatekeeper on the (correctly updated) binary.

## Release pipeline behavior per secret state

| Secrets present | macOS | Windows | Updater artifacts |
|-----------------|-------|---------|-------------------|
| none | unsigned (Gatekeeper warn) | unsigned (SmartScreen warn) | none (GitHub download path) |
| updater only | unsigned | unsigned | `.tar.gz` + `.sig` + `latest.json` |
| updater + Apple | signed + notarized | unsigned | signed updater archives |
| updater + Apple + Windows | signed + notarized | Authenticode signed | full chain |

## Community distribution channels (no certificate required)

Package managers bypass or mitigate OS trust warnings without paid certificates:

| Channel | Platform | Effect |
|---------|----------|--------|
| [Homebrew Cask](https://github.com/Po1nt9/homebrew-tap) | macOS | Clears quarantine flag automatically — no Gatekeeper dialog |
| [install.sh](../../scripts/install.sh) (`curl \| bash`) | macOS / Linux | Downloads + `xattr -cr` (macOS) or `chmod +x` (Linux) |
| Winget (`winget install`) | Windows | Microsoft-verified source reduces SmartScreen friction (submit after stable release) |
| Scoop bucket | Windows | Green/portable install, no admin required |

These are **complementary** to code signing, not a replacement. Code signing
removes the warning at the OS trust layer; package managers remove it at the
delivery layer. Both can coexist.
