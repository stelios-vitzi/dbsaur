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

## Supported versions and threat model

Security fixes target the latest default-branch code. No older release branch has
a promised support window. DBsaur is early-development software for a trusted,
single-user workstation, with no built-in multi-user authentication or per-user
authorization. Session cookies are not an independent login system.

- Keep both the frontend and API on loopback or a trusted isolated network.
  Remote use requires authentication at the entrypoint, HTTPS, network controls,
  and `WEBDB_SECURE_COOKIES=true`. Never expose the backend directly to the public.
- The API intentionally connects to user-selected hosts. It is not an SSRF-safe
  public proxy. All locally saved connections belong to the same installation.
- TLS is disabled by default. Enable verified TLS on untrusted networks.
- Query parsing and read-only transactions where supported reduce accidental
  writes; they do not replace database privileges. SQL Server has no read-only
  transaction mode. Database functions, triggers, extensions, and remote systems
  may have side effects outside ordinary transaction rollback.
- Use SELECT-only accounts for browsing. Explicit writes commit immediately and
  can destroy data; keep tested backups. MySQL/MariaDB DDL implicitly commits.
- Credentials stay in bounded server memory unless explicitly saved to the
  encrypted vault. Restrict its directory and keep the key separate from backups.
  Saved SQL, exports, clipboard contents, browser extensions, OS memory/dumps,
  database logs, and reverse-proxy logs need their own protection.
- Pagination limits rows, not bytes. Large values and expensive queries can still
  consume memory and database resources. This is not a hostile multi-tenant service.

## Release security gate — 11 November 2026

The desktop release workflow requires successful tests/builds on Windows, Linux,
and macOS plus separate dependency audit jobs before publication. Online npm
audits block high/critical findings. RustSec audits cover backend, desktop, and
xtask lockfiles and block every vulnerability, including unscored ones. Audit
errors also block publication. No advisory exceptions are configured.

Current live audit results after compatible crypto/TLS lockfile updates:

| Check | Result |
| --- | --- |
| npm | 0 high, 0 critical; 3 low and 11 moderate findings remain visible |
| backend Cargo lockfile | Blocked by RUSTSEC-2026-0459 in libsqlite3-sys 0.37.0 |
| desktop Cargo lockfile | Blocked by the same SQLite advisory; informational dependency warnings also reported |
| xtask Cargo lockfile | No vulnerabilities |

Release publication remains blocked. The SQLite advisory requires libsqlite3-sys
0.39.0 or newer, while SQLx 0.9.0 currently requires a version below 0.38.0.
Resolving that requires a supported upstream dependency upgrade or a separately
reviewed SQLite integration change; the advisory is not ignored. Compatible
updates resolve RUSTSEC-2026-0385 (ctutils), RUSTSEC-2026-0382 (rustls-webpki), and
RUSTSEC-2026-0381 (webpki-roots). Container scans remain a separate daily/PR/push
workflow and are not claimed as rerun in this desktop release check.

## Dependency review — 17 October 2026

Live registry and image scans completed on this date reported zero known
vulnerabilities across the frontend and backend packages and the build and
deployment images:

| Check | Result |
| --- | --- |
| `npm audit --offline=false --audit-level=low` | Passed; 0 findings across 607 packages |
| `cargo audit --file backend/Cargo.lock` | Passed; 0 findings across 241 crate dependencies; 1,334 RustSec advisories loaded |
| Trivy 0.76.0 on frontend and backend build and deployment images | Passed; 0 findings at every severity, including unknown severity |

The frontend build tree includes local maintained fixes for two upstream
advisories that did not have fixed releases at scan time:

- [braces](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) is patched in
  `frontend/vendor/braces` with recursion depth limits and cyclic parent-chain
  detection.
- [node-forge](https://github.com/advisories/GHSA-86w9-cpqp-85rv) is patched in
  `frontend/vendor/node-forge` to reject malformed nested DigestInfo structures.

Regression tests cover the vulnerable inputs and valid behavior. The files retain
the upstream licenses and are documented in `frontend/vendor/README.md`.

The Bookworm and Debian 13 base images previously used by the Dockerfiles had
reported OS and bundled npm advisories. Production and development stages now use
minimal Chainguard Node 26 and Rust images; the backend runtime uses the minimal
Chainguard glibc image. The Cargo registry is kept in a BuildKit cache rather than
in the builder image, so unrelated test lockfiles from downloaded crate sources
are not retained. All current build and deployment images were scanned without
ignoring unfixed findings.

CI audits the live npm registry and RustSec database and builds and scans both
build stages and deployment images. It runs on pushes, pull requests, manual
dispatch, and daily. Trivy is pinned to version 0.76.0 and its release archive is
checked against the official SHA-256 before installation. To reproduce the package
audits:

```sh
npm --prefix frontend ci
npm --prefix frontend audit --offline=false --audit-level=low
cargo install cargo-audit --locked --version 0.22.2
cargo audit --file backend/Cargo.lock
```

Image scans run in `.github/workflows/security.yml` against all severities without
`--ignore-unfixed`. These results are point-in-time checks for published
advisories; they cannot detect vulnerabilities that have not been disclosed or
added to the advisory databases. CI repeats the checks daily. Scans do not replace
application security review, penetration testing, or license review.
