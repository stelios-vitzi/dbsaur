# Contributing to DBsaur

Bug reports, documentation improvements, and focused pull requests are welcome.
For substantial changes, open an issue describing the problem and proposed behavior first.
Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Development

Use Node.js 26 LTS (26.13+) and Rust 1.99.1 with rustfmt and Clippy. The versions
are recorded in `.nvmrc` and `rust-toolchain.toml`. TypeScript stays on 6.0.3
because the current Vue type checker does not support TypeScript 7's package exports.
From the repository root:

```sh
npm --prefix frontend ci
cargo install tauri-cli --locked --version 2.12.4
cargo xtask dev
```

The sample workspace needs no database. DBsaur ships only as a desktop app; the
Nuxt interface is bundled into Tauri and the Rust API runs inside that process.
See [README.md](README.md) for native builds and [the user guide](docs/user-guide.md)
for connection storage and disposable database test configuration.

Before submitting a pull request, run:

```sh
npm run format
npm run typecheck
npm run lint
npm test
npm run test:packaging
npm run test:desktop
cargo xtask build
cargo install cargo-audit --locked
npm run audit
```

Commit the frontend and all three Cargo dependency lockfiles when changing dependencies. Do not suppress
advisories or describe an incomplete scan as clean. Never run ignored database
tests unless you explicitly configured a disposable database; some tests create
and drop databases. CI runs the sample and unit tests without database credentials.

## Architecture and review expectations

- Use Nuxt 4 `app/`, Composition API, and strict TypeScript for the frontend.
- Keep Rust HTTP handlers dependent on domain/application contracts; adapters
  implement those contracts. Register providers in the composition root.
- Extend `DatabaseAdapter` and a dedicated provider directory for new databases.
  Describe provider-specific limitations and add relevant boundary tests.
- Keep browsing separate from explicit writes. Bind SQL values, quote dynamic
  identifiers, preserve exact numeric values, sanitize driver errors, and use
  bounded operations. Database permissions remain the security boundary.
- Never log credentials, persist them in browser storage, or include real records,
  connection URLs, vault files, certificates, or private keys in issues or fixtures.
- Keep changes focused. Explain the user-visible behavior, validation performed,
  and limitations. Add tests for behavior changes and security boundaries.

## Community and contributions

Treat others respectfully. Critique ideas and code constructively; harassment,
discrimination, threats, and publishing personal information are unacceptable.
Maintainers may remove abusive content or restrict participation. Report concerns
privately using the contact in SECURITY.md.

Submit only work you have the right to contribute. Contributions are provided
under the project's [MIT license](LICENSE). Preserve third-party notices and
document the source and license of new assets. No separate contributor agreement
is required. Maintainers review contributions as time permits; there is no
guaranteed support or release schedule.
