# DBsaur

A desktop database workspace for PostgreSQL, MySQL, MariaDB, and SQL Server.
Browse data, run queries, edit records, manage schemas, and preview PostGIS
geometry from a single local application.

[Releases](https://github.com/stelios-vitzi/dbsaur/releases) ·
[User guide](docs/user-guide.md) · [Contributing](CONTRIBUTING.md) ·
[Security](SECURITY.md) · [License](LICENSE)

DBsaur is in early development. Database changes commit after choosing Save in
the pending-change notification and can be irreversible; use appropriate database
permissions and keep backups. Publication requires all desktop builds and dependency
audits to pass; see the [security policy](SECURITY.md#release-security-gate--11-november-2026).

## Features

- Explore instances, databases, schemas, tables, columns, keys, and relationships.
- Browse sortable, paginated data and inspect, copy, or export the current page.
- Work with independent query tabs, SQL completion, and locally saved queries.
- Insert, update, and delete records, with explicit schema and database actions.
- Save connection profiles and credentials in an encrypted local vault.
- Preview PostGIS geometry and geography on an interactive map.
- Try the built-in read-only sample workspace without connecting a database.

| Database         | Browsing and SELECT | Record and schema changes | Database settings |
| ---------------- | ------------------- | ------------------------- | ----------------- |
| PostgreSQL       | Supported           | Supported                 | Supported         |
| MySQL 8+         | Supported           | Supported                 | Not supported     |
| MariaDB 10.6+    | Supported           | Supported                 | Not supported     |
| SQL Server 2016+ | Supported           | Supported                 | Not supported     |

Provider behavior differs: SQL Server queries currently exclude CTEs, MySQL and
MariaDB DDL can commit implicitly, and some generated fields remain read only.
See the [user guide](docs/user-guide.md) for dialect, editing, and connection limits.

## Install

Published packages are available from [GitHub Releases](https://github.com/stelios-vitzi/dbsaur/releases).
The desktop application includes the interface and Rust API; end users do not
need Node.js, Rust, Docker, or a separate web server. Database servers remain external.

| Platform                       | Release packages                       | Installation                                                                  |
| ------------------------------ | -------------------------------------- | ----------------------------------------------------------------------------- |
| Windows x64                    | NSIS setup, MSI, standalone executable | Run setup; installation into Program Files requests UAC elevation.            |
| Linux x64                      | Debian, RPM, AppImage                  | Install the package with APT/DNF, or make the AppImage executable and run it. |
| macOS, Intel and Apple Silicon | Universal DMG and app archive          | Drag `DBsaur.app` into Applications.                                          |

Linux release builds use Ubuntu 22.04 as their compatibility baseline and require
WebKitGTK 4.1. macOS builds target macOS 11 or newer and use an ad-hoc signature;
Developer ID signing and notarization are required for distribution without
Gatekeeper warnings. Windows and Linux packages are unsigned.

## Build and run from source

Install Node.js 26 LTS (26.13+) and Rust 1.99.1 with rustfmt and Clippy, together
with the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/):
MSVC C++ build tools and WebView2 on Windows, GTK/WebKit development libraries on
Linux, or Xcode command-line tools on macOS.

From the repository root:

```sh
npm --prefix frontend ci
cargo install tauri-cli --locked --version 2.12.4
cargo xtask dev
```

`dev` generates the desktop interface and opens a native window. Rerun it after
frontend changes; this mode does not use a live frontend development server.

| Command                          | Purpose                                                                        |
| -------------------------------- | ------------------------------------------------------------------------------ |
| `cargo xtask build`              | Build native packages for the current OS.                                      |
| `cargo xtask install`            | Build and install using UAC on Windows or sudo on Linux/macOS.                 |
| `cargo xtask install --no-build` | Install the newest existing native package.                                    |
| `cargo xtask install --dry-run`  | Show the existing package and installation destination without making changes. |
| `cargo xtask frontend`           | Generate static desktop interface assets.                                      |
| `cargo xtask check`              | Check desktop Rust code after generating frontend assets.                      |

`install` uses Program Files on Windows, APT or DNF on supported Linux
distributions, and `/Applications/DBsaur.app` on macOS. Close a running DBsaur
before upgrading. The dry run requires an existing release package.

Build output, including Cargo intermediates, stays under the root `dist/`:

```text
dist/release/                       Native executable
dist/release/bundle/                Installers and app bundles
dist/debug/                         Development builds
dist/<target>/release/bundle/       Builds for an explicit target
```

To select a package format, use `cargo xtask build --bundles nsis` on Windows,
`--bundles appimage` on Linux, or `--bundles dmg` on macOS. Other Tauri build
arguments pass through unchanged.

For a universal macOS build:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo xtask build --target universal-apple-darwin --bundles app,dmg
```

Use `cargo xtask install --universal` to build and install that Mac bundle.

### Optional web development

```sh
npm run dev
```

Open `http://127.0.0.1:34616`; the Rust API listens on `127.0.0.1:34617`.
The desktop application uses its own loopback endpoint on port `34618` and stops
that API when it exits. Saved desktop connections are configured automatically;
web vault configuration and Docker deployment are covered in the
[operations guide](docs/user-guide.md#saved-connections).

## Tests and security checks

```sh
npm run typecheck
npm run lint
npm test
npm run test:packaging
npm run test:desktop
cargo install cargo-audit --locked --version 0.23.0
npm run audit
```

These checks cover TypeScript, formatting and Clippy, backend and frontend
behavior, desktop access controls, package selection, release collection, and
dependency vulnerabilities. Live database tests are opt-in and require an
explicitly configured disposable database; see [integration tests](docs/user-guide.md#integration-tests).

The [desktop release workflow](.github/workflows/desktop-release.yml) requires
successful Windows, Linux, and macOS builds and tests before publication. It also
requires npm audits with no **high or critical** findings and RustSec audits with
**no known vulnerabilities**, including unscored vulnerability advisories.
Audit errors block publication. No advisory exceptions are configured.

The [security workflow](.github/workflows/security.yml) repeats dependency audits
and container scans on pushes, pull requests, manual runs, and a daily schedule.
Dependency updates are manual; these checks do not create dependency-update commits.
See [SECURITY.md](SECURITY.md) for current findings and vulnerability reporting.

## Releases and versioning

The **Desktop build and release** workflow publishes a combined GitHub Release
after all native builds and dependency audits pass, on pushes to `main` and manual
runs on `main`. Pull requests and manual runs on other branches only produce
downloadable Actions artifacts. Its **Release outcome** summary reports blocking
gates. A successful ordinary **CI** run alone does not publish a release.
Packages include license notices and platform-specific SHA-256 manifests.

CI caches Rust dependencies and installed build tools per platform. The first
build after a cache miss remains slow; later builds reuse compiled dependencies.
macOS universal releases still compile for both Apple Silicon and Intel.

Signed desktop releases check for updates at launch and offer **Install and
restart** in the app. See [desktop updates](docs/desktop-updates.md) for supported
installation formats and the required GitHub signing-key setup.

The source version is maintained in the backend and desktop Cargo manifests and
`src-tauri/tauri.conf.json`. Ask the user to approve a version bump for every change
set, including documentation updates; never change it without their answer.

CI currently derives installer versions as `<source-major>.<source-minor>.<run-number>`
and tags releases `main-<run-number>-<attempt>`. This generated build version is
separate from the approved source version. See [AGENTS.md](AGENTS.md) for project
instructions and [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidance.

## Project structure

```text
frontend/       Nuxt 4, Vue 3, TypeScript, interface and web API proxy
backend/        Rust/Axum API, database adapters, sessions, encrypted vault
src-tauri/      Native desktop host, local API protection, installer configuration
xtask/          Desktop build and installation commands
scripts/        Development helpers, smoke tests, release asset collection
docs/           Detailed usage, operations, and historical reviews
```

## Security and data handling

DBsaur is a trusted, single-user tool. Database permissions remain the security
boundary; use a SELECT-only account for browsing. TLS is disabled by default, so
enable verified TLS before sending credentials over an untrusted network.
Keep the API private. Remote or shared deployments require authentication,
HTTPS, and appropriate network controls.

Credentials remain in session memory unless saved to the encrypted vault. Protect
the vault and its key; losing the key makes stored credentials unrecoverable.
Saved SQL uses local interface storage and should not contain secrets. Optional
PostGIS basemaps fetch tiles and fonts from `vijimaps.com`, exposing the client IP
address and viewed map area to that service; database records and geometry values
are not sent to the tile service.

## License

DBsaur is available under the [MIT license](LICENSE). Dependencies and bundled
assets retain their own licenses; see [third-party notices](THIRD_PARTY_NOTICES.md).
The software is provided as is, without a guaranteed support or release schedule.
