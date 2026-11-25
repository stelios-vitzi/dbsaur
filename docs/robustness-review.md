# Robustness roadmap review — 17 November 2026

Reviewed against source version 0.2.2; this change set uses the approved patch
bump to 0.2.3. Installer versions still use the existing CI run-number scheme.

| Roadmap item              | Evidence before this change                                                                                                                                                               | Result                                                                                                                                                                                                                                                                                        |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Connection read-only lock | Already persisted in profiles/sessions; HTTP mutations and settings reject locked sessions; SQL and MongoDB query runners already reject write statements/stages. Defaults were writable. | Remote form/URL defaults are now locked; omitted backend lock flags default locked, including legacy profiles. A lock badge appears in the connection header. Unlocking requires editing the profile and reconnecting.                                                                        |
| Change preview            | Existing confirmation only described the target. Adapter transactions already protect individual SQL record changes.                                                                      | Save now opens a modal with the exact pending operation and values, including raw SQL when using Database changes, plus irreversible-change warnings. **Generated SQL previews and atomic multi-row editing remain incomplete**; the modal explicitly identifies its operation-level preview. |
| Smart TLS defaults        | Form and URL imports defaulted to plaintext. Drivers already support certificate and hostname verification.                                                                               | Only localhost, 127.0.0.1, and ::1 receive plaintext development defaults. Remote hosts default verified TLS. Explicit URL downgrades remain explicit. Existing saved TLS choices are preserved.                                                                                              |
| Vault recovery            | AES-GCM vault existed; losing vault.key prevented startup.                                                                                                                                | Passphrase-encrypted export/import, generated recovery passphrase, first-profile reminder, atomic additive imports, and separate recovery storage when the original key is missing. See [recovery instructions](user-guide.md#vault-backup-and-recovery).                                     |
| Private/offline maps      | Opening a map automatically requested VijiMaps tiles/fonts. A background-free geometry style already existed.                                                                             | No map network requests by default. Privacy settings configure opt-in XYZ tiles or vector style JSON; an offline coordinate grid and geometry remain usable without a tile server.                                                                                                            |
| Engine CI                 | Live tests existed but were ignored by normal CI. SQLite tests already run without external services.                                                                                     | Disposable PostGIS/PostgreSQL 16, MySQL 8.4, MariaDB 10.11, SQL Server 2022, MongoDB 8 service containers run opt-in tests. MySQL uses a generated CA and hostname-verified TLS. Releases depend on this job. This does not prove every historical supported server version.                  |
| SQL Server CTEs           | CTEs could not be nested in the derived table used for paging.                                                                                                                            | Parsed WITH clauses stay at statement scope for description, pagination, and exact counts. Live multi-CTE tests pass; write/unsafe query validation remains in force.                                                                                                                         |
| SQLite parity             | Record CRUD, constraint introspection, transactions, index edits, and limited DDL already existed. Query results were always read only.                                                   | Direct single-table query projections now reuse the existing hidden-snapshot editing contract. Views, joins, aggregates, generated fields, and locked sessions stay read only. **Automatic table rebuilding for type/key changes remains incomplete.**                                        |
| OS signing                | Tauri updater signatures existed; OS signing/notarization did not.                                                                                                                        | Authenticode and Developer ID signing and notarization are independent release opt-ins. When enabled, preflight requires credentials and the workflow verifies OS signatures. Hosted builds with real identities still need verification. |
| Community readiness       | Bug Report and Feature Request templates already existed; screenshots and engine template were missing. Public repository description was null and topics empty.                          | Added Engine Compatibility template and three high-resolution screenshots rendered from synthetic fixtures. Remote metadata is still pending: connector has no edit tool, browser host is unavailable, and no Git CLI credential is saved.                                                    |

## Remaining engineering work

An exact SQL preview must share an immutable prepared plan with execution.
Duplicating SQL generation in the UI would become misleading when metadata,
generated constraint names, or PostgreSQL record locations change. The current
review shows the frozen mutation that is actually submitted; it makes no claim
to execute a dry run. A rollback-based dry run also cannot undo external effects
or MySQL/MariaDB implicit DDL commits.

Multi-row edits need one connection and transaction for the whole batch, with
stale-row checks, rollback tests, and an explicit policy for MongoDB deployments
without transaction support. The current interface saves individual records;
separate Save actions are not an atomic batch.

SQLite table rebuilding needs a schema snapshot, explicit column mapping,
preservation of triggers/indexes/foreign keys and table options, foreign-key
validation before commit, and rollback fixtures. Existing unsupported operations
remain rejected rather than being approximated through lossy recreation.

## External setup

Follow [OS signing setup](desktop-updates.md#operating-system-trust); do not put
private signing material in this repository. No secrets were read from GitHub,
no release was published, and no branch or tag was pushed.

Repository metadata can be applied from an authenticated GitHub CLI session:

```sh
gh repo edit stelios-vitzi/dbsaur --description "A desktop database workspace for PostgreSQL, PostGIS, MySQL, MariaDB, SQL Server, SQLite, and MongoDB. Built with Tauri, Rust, and Vue." --add-topic database-client --add-topic tauri --add-topic rust --add-topic vue --add-topic postgresql --add-topic postgis --add-topic mongodb --add-topic sqlite
```

## Verification

Local Windows checks include TypeScript, lint/Clippy, backend and frontend tests,
desktop and packaging tests, a production frontend build, and all three Cargo
lockfile audits plus npm audit. No npm or Rust vulnerability findings; existing
desktop glib unsoundness and proc-macro-error maintenance warnings remain visible.
Live tests used only newly created, loopback-bound disposable containers. Browser
checks verified cancellation without a write, one confirmed write, and encrypted
backup export/import using synthetic fixtures. Linux/macOS native builds and real
OS signing/notarization were not verified locally; hosted CI must pass.
