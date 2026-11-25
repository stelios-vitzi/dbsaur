# Desktop updates

Signed release builds check GitHub's latest published release on every launch,
without blocking the workspace. The update bar also provides a manual check,
download progress, retry after failure, and **Install and restart**. Installation
requires confirmation; save your work first. **Later** postpones the update until
you check again or reopen the app. Offline checks leave the workspace usable.

Windows NSIS installations, macOS app bundles, and Linux AppImages support this
flow. Windows setup asks whether to install for the current user (without
elevation) or all users (with administrator permission). Updates preserve that
scope; all-users updates and uninstalling require elevation. Local builds
have updates disabled unless configured with a signing key.

Releases provide Windows setup `.exe`, Linux `.deb`, `.rpm`, and portable
`.AppImage` packages, and a universal macOS `.dmg`. The macOS `.app.tar.gz`, signatures,
and `latest.json` remain release assets for the updater; users should use the
five direct download links at the top of the release notes for manual installs.
SHA-256 manifests and license notices are also retained.

DEB and RPM installations should be updated manually by installing the newer
package with APT or DNF. The Linux updater manifest continues to target AppImage;
automatic updates of DEB/RPM installations are not configured or verified.

MSI and standalone Windows executables are no longer published.
Before switching from an existing MSI installation, uninstall it and install the
NSIS setup executable manually. Remove existing DEB/RPM installations with their
package manager before switching to AppImage. This format migration has not been
verified as an automatic update path. Local builds can still explicitly request
alternative formats with `cargo xtask build --bundles <format>`.

## Maintainer setup

### Operating-system trust

Updater signatures verify updates inside DBsaur; they do not establish a verified
Windows publisher or satisfy macOS Gatekeeper. Release preflight now requires
OS signing setup; local builds keep their existing unsigned/ad-hoc behavior.

Configure these Actions secrets before releasing:

- `APPLE_CERTIFICATE`: base64-encoded Developer ID Application `.p12`.
- `APPLE_CERTIFICATE_PASSWORD`: its export password.
- `APPLE_SIGNING_IDENTITY`: the full Developer ID Application identity.
- `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), and `APPLE_TEAM_ID`
  for notarization.
- `WINDOWS_CERTIFICATE`: base64-encoded signing `.pfx`, with its private key.
- `WINDOWS_CERTIFICATE_PASSWORD`: its export password.

Set repository variable `WINDOWS_TIMESTAMP_URL` to the timestamp endpoint
provided by your certificate authority. Tauri signs the application and NSIS
installer during bundling, before updater signatures and checksums are generated.
The workflow verifies both Windows signatures, and runs codesign, Gatekeeper,
and stapler validation for the macOS app. Credentials must support unattended
signing; a hardware-backed or hosted signing service may require a different
Windows signing command. These paths need hosted validation with real identities.
See Tauri's [macOS signing](https://v2.tauri.app/distribute/sign/macos/) and
[Windows signing](https://v2.tauri.app/distribute/sign/windows/) instructions.

For direct Windows downloads, sign the application and installer with a trusted
code-signing identity and timestamp the signatures. SmartScreen can still warn
on new signed files while reputation builds; EV certificates no longer grant an
automatic bypass. Microsoft documents Store distribution as a way to avoid these
warnings. There is no installer flag that guarantees suppression. See
[Microsoft's SmartScreen guidance](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
macOS distribution without Gatekeeper warnings requires Developer ID signing and
Apple notarization. These OS signing services are not configured by the updater
key settings below.

See the [Windows installer notes](../src-tauri/windows/README.md) for the scope
implementation, icon generation, and manual installer verification.

### Updater signing

Generate a persistent signing key with the Tauri CLI, outside this repository:

```sh
cargo tauri signer generate -w /secure/location/dbsaur-updater.key
```

Keep a secure backup. In the GitHub repository's Actions settings, configure:

- Variable `TAURI_UPDATER_PUBLIC_KEY`: the contents of the generated `.pub` file.
- Secret `TAURI_SIGNING_PRIVATE_KEY`: the contents of the private key file.
- Secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password (empty if none).

Never commit the private key or paste it into issues or logs. Keep the public key
stable: changing it prevents existing installations from trusting new updates.
The workflow embeds the public key in release builds and signs native updater
artifacts. Version-tag release builds fail when signing configuration is missing.
The **Validate release signing setup** job checks both key settings before any
native build starts. A setup failure blocks publication and names the missing
settings without printing key contents. It does not verify that the keys match;
Tauri signing and update verification still check their validity.

Repository settings are under **Settings → Secrets and variables → Actions**:
use the **Variables** tab for the public key and **Secrets** for the private key
and optional password. Do not put the public key only in Secrets: the workflow
reads `vars.TAURI_UPDATER_PUBLIC_KEY`.

The release job combines signed assets into `latest.json`, with Windows x64,
Linux x64, and both architectures of the universal macOS app. The app reads
`https://github.com/stelios-vitzi/dbsaur/releases/latest/download/latest.json`.
Only newer versions are offered; draft releases are not offered. Manually
published releases need the same complete manifest and correctly signed assets.
The existing build matrix and all dependency audit gates still block publication.
Native builds and publication run only when a version tag matching `<major>.<minor>.<patch>`
or `v<major>.<minor>.<patch>` is pushed, for example `git tag 0.2.0` followed by
`git push origin 0.2.0`. The tagged commit must contain this workflow; an already
pushed tag does not start a new run when later branch commits change the workflow.
The GitHub Release uses that existing tag. The tag push also runs dependency
audits and calls the CI workflow for frontend checks and disposable database
integration. Each of these checks must pass before publication.
Installer versions remain `<source-major>.<source-minor>.<run-number>`.

Install the first release containing this updater manually. Older DBsaur
installations cannot discover updates until they have the updater code. Verify
an actual update from one signed release to a newer signed release on each OS;
unit tests and compilation do not exercise installer permissions or replacement.

Local native builds keep updates disabled by default. To test signed bundles,
provide the signing environment variables and a Tauri configuration override
containing `bundle.createUpdaterArtifacts: true` and `plugins.updater.pubkey`;
pass the override through `cargo xtask build --config <file>`. Keep generated
key material outside the repository.
