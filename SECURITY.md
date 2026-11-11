# Security policy

## Reporting a vulnerability

Do not disclose vulnerabilities, credentials, private connection strings, or real
database records in public issues. Use GitHub's **Report a vulnerability** option
on the repository Security tab when enabled. Otherwise email
**steliosvitzi@gmail.com** with the subject **DBsaur security report**.

Include the affected commit/version, provider, deployment method, impact, and a
minimal reproduction with synthetic data. Maintainers review reports as time
permits; there is no guaranteed response time or paid bug bounty. Please coordinate
public disclosure while a report is investigated.

## Desktop update trust

Release builds check GitHub for updates on launch. Updates require explicit
installation confirmation and a valid Tauri signature before installation.
The release workflow uses an Actions private-key secret and embeds the public
key in the application. Keep the key stable and backed up; see
[desktop update setup](docs/desktop-updates.md). Unsigned local builds do not
check for updates. GitHub receives the client's IP address during update checks
and downloads; database contents and credentials are not included.

Tauri updater signatures are separate from OS code signing. Windows executables
and installers currently have no Authenticode signature and can trigger
SmartScreen warnings. macOS packages use ad-hoc signing without Developer ID
notarization and can trigger Gatekeeper warnings. See
[operating-system trust](docs/desktop-updates.md#operating-system-trust).

Online audits were rerun after adding the updater: npm still reports 6 low and
6 moderate findings, with no high/critical findings. All three Cargo lockfiles
have no vulnerability findings; the desktop glib and proc-macro-error warnings
remain visible. Native build and signed update verification are separate gates.

## Supported versions and threat model

Security fixes target the latest default-branch code. No older release branch has
a promised support window. DBsaur is early-development software for a trusted,
single-user workstation. The bundled interface and Rust API run within the
Tauri application. There is no standalone server or multi-user deployment mode.

- The embedded API listens only on `127.0.0.1:34618` and requires a private
  desktop session token. Keep it private; remote/shared hosting is unsupported.
- The application intentionally connects to user-selected database hosts.
  All locally saved connections belong to the same OS user installation.
- TLS is disabled by default. Enable verified TLS on untrusted networks.
- Query parsing and read-only transactions where supported reduce accidental
  writes; they do not replace database privileges. SQL Server has no read-only
  transaction mode. Database functions, triggers, extensions, and remote systems
  may have side effects outside ordinary transaction rollback.
- Use SELECT-only accounts for browsing. Explicit writes commit after choosing Save and
  can destroy data; keep tested backups. MySQL/MariaDB DDL implicitly commits.
- Credentials stay in bounded local process memory unless explicitly saved to the
  encrypted vault. Restrict its directory and keep the key separate from backups.
  Saved SQL, exports, clipboard contents, webview caches, OS memory/dumps,
  and database logs need their own protection.
- Pagination limits rows, not bytes. Large values and expensive queries can still
  consume memory and database resources. This is not a hostile multi-tenant service.

## Release security gate — 11 November 2026

The desktop release workflow requires successful tests/builds on Windows, Linux,
and macOS plus separate dependency audit jobs before publication. Online npm
audits block high/critical findings. RustSec audits cover backend, desktop, and
xtask lockfiles and block every vulnerability, including unscored ones. Audit
errors also block publication. No advisory exceptions are configured.

Current live audit results after the dependency and vault-driver updates:

| Check | Result |
| --- | --- |
| npm | 0 high, 0 critical; 6 low and 6 moderate findings remain visible |
| backend Cargo lockfile | No vulnerabilities |
| desktop Cargo lockfile | No vulnerabilities; glib unsoundness and proc-macro-error maintenance warnings remain visible |
| xtask Cargo lockfile | No vulnerabilities |

The earlier RUSTSEC-2026-0459 publication blocker is resolved by using rusqlite
0.42.0 with bundled libsqlite3-sys 0.40.0 for the encrypted connection vault.
The schema and AES-256-GCM record format remain compatible with existing vaults;
database operations run on Tokio's blocking pool. PostgreSQL and MySQL retain
the matching SQLx 0.9.0 core/driver crates, pinned exactly because those crates
do not promise independent semver compatibility. This avoids the SQLx umbrella
crate's incompatible optional SQLite constraint without suppressing advisories.
The existing crypto/TLS fixes remain in place. Upstream node-forge 1.4.3 replaces
the previous local Forge patch and passes its security regression tests.

Passing audits alone does not publish a release: all native platform build gates
must also pass. Version 0.1.3 removes standalone web hosting, container images,
and container CI jobs. The daily/PR/push npm and RustSec audits remain enabled
and still audit all three Cargo lockfiles. Historical container scan results
below describe the retired deployment and are not current desktop scan results.

## Dependency review — 17 October 2026

Live registry and image scans completed on this date reported zero known
vulnerabilities across the frontend and backend packages and the build and
deployment images:

| Check | Result |
| --- | --- |
| `npm audit --offline=false --audit-level=low` | Passed; 0 findings across 607 packages |
| `cargo audit --file backend/Cargo.lock` | Passed; 0 findings across 241 crate dependencies; 1,334 RustSec advisories loaded |
| Trivy 0.76.0 on frontend and backend build and deployment images | Passed; 0 findings at every severity, including unknown severity |

At the time of this historical scan, the frontend build tree included local fixes for two upstream
advisories that did not have fixed releases at scan time:

- [braces](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) is patched in
  `frontend/vendor/braces` with recursion depth limits and cyclic parent-chain
  detection.
- [node-forge](https://github.com/advisories/GHSA-86w9-cpqp-85rv) was patched
  locally to reject malformed nested DigestInfo structures; the local copy has
  since been replaced by upstream 1.4.3.

Regression tests cover the vulnerable inputs and valid behavior. The files retain
the upstream licenses and are documented in `frontend/vendor/README.md`.

The container-image results above are historical. Dockerfiles, Compose files,
container deployment helpers, and Trivy image jobs were removed when DBsaur
became desktop-only. They no longer describe a supported deployment.

CI audits the live npm registry and RustSec database on pushes, pull requests,
manual runs, and daily. To reproduce all current package audits:

```sh
cargo install cargo-audit --locked --version 0.23.0
npm run audit
```

These are point-in-time checks for published advisories; they cannot detect
vulnerabilities that have not been disclosed or added to advisory databases.
Scans do not replace application security review or license review.
