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

Online audits were rerun on 12 November 2026 after the MongoDB and saved connection updates: npm reports zero
findings. All three Cargo lockfiles have no vulnerability findings; the desktop
glib and proc-macro-error warnings remain visible. Native build and signed update
verification are separate gates.

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
- SQLite opens an existing, user-selected local file. Browsing uses read-only
  handles and query-only mode; explicit writes use transactions with foreign keys
  enabled. URI parameters, attached databases, extension loading, and arbitrary
  PRAGMA commands are unavailable. OS file permissions are the access boundary;
  SQLite files have no server-side user permissions or TLS. Keep backups before
  editing files, including files used by other applications.
- Query parsing and read-only transactions where supported reduce accidental
  writes; they do not replace database privileges. SQL Server has no read-only
  transaction mode. Database functions, triggers, extensions, and remote systems
  may have side effects outside ordinary transaction rollback.
- Use SELECT-only accounts for browsing. Explicit writes commit after choosing Save and
  can destroy data; keep tested backups. MySQL/MariaDB DDL implicitly commits.
- Column definition changes compare a metadata snapshot before applying. PostgreSQL
  and SQL Server keep these checks and changes in a transaction. MySQL/MariaDB
  apply one ALTER statement; their implicit DDL commit does not provide the same
  transactional protection against concurrent schema changes. Column deletion
  requires typing its name and confirming Save; dependent keys/indexes can change.
- Credentials stay in bounded local process memory unless explicitly saved to the
  encrypted vault. Restrict its directory and keep the key separate from backups.
  Connection settings return redacted passwords and TLS private keys. Unchanged
  secrets are retained on the backend; replacements are encrypted with a fresh
  nonce under the same saved connection ID. Saving or testing settings does not
  replace the active session.
  Saved SQL, exports, clipboard contents, webview caches, OS memory/dumps,
  and database logs need their own protection.
- Index edits compare the displayed metadata before opening the write operation.
  Concurrent schema changes after that check remain possible. PostgreSQL and SQL
  Server replace indexes in a transaction; MySQL/MariaDB use one ALTER TABLE.
  Advanced indexes remain read only in the schema editor to preserve their options.
- Pagination limits rows, not bytes. Large values and expensive queries can still
  consume memory and database resources. This is not a hostile multi-tenant service.
- Editable single-table query results include hidden source-record snapshots in the
  response, even for partial projections. Record changes still require explicit
  Save confirmation and stale-row checks. Joins and aggregates do not gain editing
  rights from matching column names. Exact page counts share query timeouts.

## Release security gate — 11 November 2026

The desktop release workflow requires successful tests/builds on Windows, Linux,
and macOS plus separate dependency audit jobs before publication. Online npm
audits block high/critical findings. RustSec audits cover backend, desktop, and
xtask lockfiles and block every vulnerability, including unscored ones. Audit
errors also block publication. No advisory exceptions are configured.

Current live audit results after adding SQLite support (12 November 2026):

| Check | Result |
| --- | --- |
| npm | 0 findings at every severity |
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
The Nuxt removal also removes node-forge and braces from the resolved npm tree.
Their overrides, local braces patch, and dependency-specific regression tests
were removed because those dependencies are no longer installed.

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

At the time, regression tests covered vulnerable inputs and valid behavior.
The Nuxt migration later removed these dependencies, patches, and their tests;
frontend/vendor is no longer part of the current tree.

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

## MongoDB boundary

MongoDB browsing and aggregation queries are read only. Aggregation uses an explicit
stage allowlist, recursively validates facets, and rejects write stages and
server-side JavaScript. Read-only connection mode blocks mutations at the API and
adapter. Writes use separate insert/replace/delete document operations, require
confirmation, and target one document. Replacement/deletion use an atomic original
BSON comparison with simple collation; partial projections and aggregation results
never expose edit snapshots. System collections cannot be edited. Original BSON
snapshots preserve field order and exact BSON types alongside the JSON view.
These safeguards do not replace database privileges.

Results use canonical Extended JSON to preserve BSON types and exact numbers. Reads
have a 10-second deadline, a 1,001-document cursor limit (including the pagination
sentinel), and an 8 MiB serialized-page budget including snapshots. Large BSON
documents can be received before that budget is checked. Write payloads are limited
to 1 MiB of Extended JSON; snapshots to 1 MiB of BSON. Writes have a 15-second client
deadline; a timeout does not prove that a write was rolled back. TLS validates server
certificates and hostnames using public trust roots; custom CA/client certificates
and SRV URIs are not supported yet.

Online audits were rerun on 12 November 2026 after adding MongoDB editing and the
CodeMirror JSON language package:
npm reports zero vulnerabilities and all three Cargo lockfiles have no vulnerability
findings. The existing desktop glib unsoundness and proc-macro-error maintenance
warnings remain visible. These results supersede the SQLite-era scans above.
The unchanged native platform build gates remain required before release.
