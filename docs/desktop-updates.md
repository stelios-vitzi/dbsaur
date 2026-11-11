# Desktop updates

Signed release builds check GitHub's latest published release on every launch,
without blocking the workspace. The update bar also provides a manual check,
download progress, retry after failure, and **Install and restart**. Installation
requires confirmation; save your work first. **Later** postpones the update until
you check again or reopen the app. Offline checks leave the workspace usable.

Windows NSIS/MSI installations, macOS app bundles, and Linux AppImages support this
flow. Windows may request administrator permission because DBsaur installs per
machine. DEB/RPM users should update through their package manager. Local builds
and PR artifacts have updates disabled unless configured with a signing key.

## Maintainer setup

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
artifacts. Main release builds fail when signing configuration is missing.

The release job combines signed assets into `latest.json`, with Windows x64,
Linux x64, and both architectures of the universal macOS app. The app reads
`https://github.com/stelios-vitzi/dbsaur/releases/latest/download/latest.json`.
Only newer versions are offered; draft releases are not offered. Manually
published releases need the same complete manifest and correctly signed assets.
The existing build matrix and all dependency audit gates still block publication.
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
