# Open-source readiness review — 17 October 2026

This is a historical review. DBsaur became desktop-only in version 0.1.3; web
server and container deployment instructions below no longer apply. Use the
[README](../README.md) for the supported desktop workflow.

## Scope and conclusion

Reviewed the tracked repository, architecture, provider contracts, connection and
credential flow, SQL boundaries, deployment defaults, tests, documentation, and
release metadata. The current structure is suitable for community contributions:
Nuxt components/composables consume typed contracts; Rust transport depends on
application contracts, implemented by provider and vault adapters. Provider
registration is centralized in `router_with_repository`.

This is a source review and release-preparation pass, not an independent security
assessment or legal/compliance certification. Real database behavior was not
retested because no disposable database was explicitly configured.

## Changes

- Added the MIT license, manifest license/runtime metadata, contribution guidance,
  issue/PR templates, editor/line-ending conventions, and third-party asset notices.
- Added CI for formatting, Clippy, TypeScript, tests, production builds, and the
  sample API smoke test. Added independent scheduled npm/Rust audits and Dependabot.
  Actions use commit pins, read-only permissions, and no persisted checkout token.
- Kept dependency versions unchanged and enabled locked Cargo checks/builds.
- Extracted the API test module from `backend/src/lib.rs` into `backend/src/tests.rs`;
  production composition and HTTP behavior are unchanged. Documented adapter contracts.
- Removed the personal street address from current privacy copy. Clarified that
  deployment operators control processing and that usage notes do not restrict MIT
  rights. Added warranty, data-loss, access, support, and deployment disclosures.
- Disabled Nuxt telemetry explicitly, expanded local-secret ignore patterns, and
  bundled font/icon license texts in `frontend/public/licenses` for built artifacts.
  Docker build contexts now exclude local key material and the isolated Rust
  validation output directory.
- Corrected README pagination to the actual API limit of 1,000 rows per page and
  removed the implication that SQL Server uses read-only transactions.

## Findings and remaining limitations

The main security boundary is the database account plus the trusted local
installation. The API intentionally accepts private database hosts and has no
multi-user login. Read-only query validation cannot guarantee that privileged
functions or external systems have no side effects. See [SECURITY.md](../SECURITY.md).

The SQL parser, timeout handling, identifier quoting, parameter binding, stale-row
checks, sanitized errors, and encrypted credential vault already have focused
tests. This review did not establish an exploitable SQL injection or credential
disclosure; that is not proof none exists.

`DatabaseWorkspace.vue` and the global stylesheet remain relatively large, but
are organized around the existing workspace. Further splitting should accompany
focused behavior work with UI verification, rather than a broad release refactor.
The backend transport module is now separate from its test fixtures.

All 20 reachable commits were checked with high-signal token/private-key patterns;
matches were synthetic PEM validation fixtures. Tracked environment/key/vault file
path checks found no committed files of those types. This limited pattern review
is not a comprehensive secret scan. A personal street address remains in older
privacy-page commits; removing it from the working tree does not remove history.

## Maintainer publication checklist

- [ ] Track upstream fixes for the 11 disclosed high-severity development/build
  dependency findings. The dependency audit remains failing; no clean-source-tree
  security claim should accompany publication.
- [ ] Decide whether to remove the personal address from Git history before making
  it public. A coordinated history rewrite or a fresh publication repository is
  needed to omit old content. Do not force-push without considering existing clones.
- [ ] Run a dedicated history secret scanner before changing repository visibility;
  rotate any genuine exposed credentials rather than relying on deletion.
- [ ] Confirm rights to publish all original code and the Vijitek logo. The notices
  document asset licenses but do not establish ownership or audit every dependency.
- [ ] Push the prepared changes and review the first GitHub Actions runs. CI YAML
  was prepared locally; hosted CI cannot be verified until GitHub runs it.
- [ ] Enable private vulnerability reporting, secret scanning/push protection and
  Dependabot alerts where available. Require CI and review on the default branch.
  See [GitHub's reporting setup](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).
- [ ] Before promising provider compatibility in a release, run opt-in integration
  tests against explicitly disposable PostgreSQL/MySQL/MariaDB/SQL Server instances.
- [ ] Before shipping binaries/images, review the complete resolved dependency
  license inventory, retain all required notices, and scan container OS packages.

Repository visibility, settings, branches, and remote history were not changed.
No release, package, container, or public deployment was published by this review.

## Validation results

- `npm run typecheck`: passed.
- `npm run lint`: passed, including rustfmt, Clippy with warnings denied, and Prettier.
- `npm test`: passed, 23 Rust tests and 16 frontend tests; 9 real-database tests
  ignored by design. Final run used `backend/target-validation` because an existing
  development server held the ordinary debug executable open on Windows.
- `npm run build`: passed (release Rust binary and Nuxt production server).
- `npm run test:smoke`: passed against isolated production processes on ports
  34626/34627; existing development servers were left running.
- Built privacy/terms pages and font/icon license endpoints returned HTTP 200;
  the built privacy page no longer includes the removed street address.
- Online npm source-tree audit FAILED with 11 high-severity findings. Generated
  production-server and RustSec audits passed. No findings were suppressed.
  See the dated [scan report](../SECURITY.md).
- GitHub workflow and Dependabot YAML parsed successfully. Hosted execution and
  branch protection still require GitHub-side setup.

The Nuxt production build emits an upstream Node DEP0155 package-export
deprecation warning from `@vue/shared`. It does not prevent the build; it has not
been hidden or treated as a security advisory.
