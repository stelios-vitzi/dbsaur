# DBsaur project instructions

These instructions apply to the entire repository. Read more specific AGENTS.md
files when working in a subdirectory that has one.

## Version changes

- For every new change set, explicitly ask the user whether to bump the project's
  version and which version to use. This includes features, fixes, refactors,
  dependency updates, configuration, and documentation changes.
- Ask once per change set, not once per file. Suggest a patch bump for compatible
  fixes or documentation changes, but let the user decide.
- Continue authorized implementation and verification while awaiting the answer.
  Do not change version numbers without explicit approval. If the user declines,
  keep the current version and mention that decision in the handoff.
- When approved, keep `backend/Cargo.toml`, `src-tauri/Cargo.toml`, and
  `src-tauri/tauri.conf.json` aligned, and update the corresponding local package
  entries in `backend/Cargo.lock` and `src-tauri/Cargo.lock`. The xtask helper has
  its own version; do not bump it unless the user includes it in the request.
- Source versions and CI-generated installer versions are distinct. Inspect
  `scripts/desktop-release.mjs` before changing release versioning; do not silently
  alter the CI version scheme.

## Architecture and conventions

- Frontend: Nuxt 4, Vue Composition API, strict TypeScript, and `app/` conventions.
- Backend: Rust, Axum, and SQLx. Do not add .NET projects.
- Desktop: Tauri. Keep desktop builds and installation commands in xtask, with
  generated native output under the repository-root `dist/` directory.
- HTTP routes depend on application/domain contracts; database adapters implement
  those contracts. Add providers through `DatabaseAdapter` and register them in
  the composition root.
- Reuse existing abstractions and make the smallest correct change. Avoid
  unrelated refactoring, formatting, dependencies, or generated files in commits.
- Keep dependency lockfiles committed. Dependency updates are manual; do not
  enable Dependabot or other automated dependency-update commits without an
  explicit user request.

## Database and credential safety

- Keep browsing read only and separate from explicitly confirmed write actions.
- Parameterize SQL values and quote every dynamic SQL identifier. Preserve exact
  numeric representations, bounded operations, and sanitized errors.
- Never log credentials, expose raw driver errors, or persist credentials in
  browser storage. Never commit real connection strings, vaults, keys,
  certificates, exports, or database records.
- Real database tests require an explicitly configured disposable database.
  Never run ignored integration tests against an assumed or production database.

## Verification and release gates

- Start with targeted checks. For relevant code changes, run `npm run typecheck`,
  `npm run lint`, `npm test`, and the appropriate build checks.
- For desktop or packaging changes, also run `npm run test:packaging`,
  `npm run test:desktop`, xtask formatting/Clippy, and native build checks where
  the host supports them. State which native platforms were actually verified.
- For documentation-only changes, check formatting, local links, command names,
  and consistency with current configuration; do not run unrelated full builds.
- Releases must remain blocked on failed tests/builds, npm high/critical
  vulnerabilities, any RustSec vulnerability, or audit infrastructure failures.
  Preserve audits of all three Cargo lockfiles and the platform build matrix.
- Never ignore advisories, weaken audit thresholds, add allow-failure steps, or
  describe incomplete or failed scans as clean. Report remaining findings and
  release blockers clearly.
- Do not bypass the release workflow to publish manually. Keep commits and pushes
  within the user's authorized scope.

## Documentation and communication

- Keep README.md concise and oriented around installation, development, testing,
  and releases. Put detailed provider and deployment instructions in `docs/`.
- Update SECURITY.md when relevant audit results or security limitations change;
  distinguish current findings from historical scans.
- Keep progress updates and handoffs concise. Report changed files, verification
  results, the version decision, and remaining concerns.
