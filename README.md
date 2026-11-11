# DBsaur

A local, single-user database administration tool. **Early development:** APIs and
stored formats may change, and no stable release or support schedule is promised.
Keep the API private and back up databases before making changes. Writes commit
immediately and may be irreversible. Database permissions are the security boundary;
use a SELECT-only account for browsing. TLS is disabled by default: enable verified
TLS before sending credentials over an untrusted network.

[Contributing](CONTRIBUTING.md) ? [Security](SECURITY.md) ?
[License](LICENSE) ? [Third-party notices](THIRD_PARTY_NOTICES.md) ?
[Release audit](docs/open-source-readiness.md)

A database workspace with a Nuxt 4 / Vue 3 / TypeScript interface and a Rust / Axum backend. PostgreSQL supports browsing, record editing, and schema changes. MySQL, MariaDB, and SQL Server support browsing, record editing, and explicit schema changes. A clearly labeled read-only sample workspace works without a database.

## Database providers

| Provider | Default port | Browsing and SELECT queries | Instance discovery | Editing and settings |
| --- | --- | --- | --- | --- |
| PostgreSQL | 5432 | Yes | Yes, through postgres | Yes |
| MySQL 8+ | 3306 | Yes | Yes, without a default database | Records and schema; no settings |
| MariaDB 10.6+ | 3306 | Yes | Yes, without a default database | Records and schema; no settings |
| SQL Server 2016+ | 1433 | Yes | Yes, through master | Records and schema; no settings |

Select a provider in the connection form; the port, query dialect, completion quoting, and generated SELECT adapt automatically. MySQL schemas correspond to databases. SQL Server uses SQL authentication over TCP; enter a reachable TCP port rather than an instance-name suffix. New connections default to TLS disabled. Enable **Verify TLS connection** using the checkbox to the left of TLS settings to encrypt traffic and verify the server certificate and hostname. Saved connections and database switching work with each provider.

New providers return non-NULL cell values as exact text, including numbers, dates, JSON and booleans; binary values use hexadecimal text. NULL remains JSON null. Table sorting uses native database values. The new providers expose column types, nullability, defaults, and primary keys, with generated and identity fields marked read only. PostgreSQL settings remain exclusive to PostgreSQL.

MySQL and MariaDB browsing runs with session-level read-only transactions and query/lock timeouts. Their query editor accepts one parsed SELECT, including CTEs and joins, and rejects writes, SELECT INTO, locking SELECTs, executable comments, and pass-through table functions. The adapter executes the parsed SQL with backslash escapes disabled. SQL Server has no equivalent read-only transaction mode: its editor validates a single SELECT, rejects those same unsafe constructs, and validates a derived SELECT with the server before execution. For a browse-only permission boundary, use a SELECT-only account. Explicit editing needs corresponding write privileges. SQL Server queries must return named columns and be valid inside a derived table; CTEs and a bare inner ORDER BY are currently unsupported. Use TOP or OFFSET with an inner ORDER BY. Paging without a unique ordering can shift between requests.

Provider tests are opt-in and require explicit connection JSON naming a disposable database. The variables are WEBDB_TEST_MYSQL_CONNECTION_JSON, WEBDB_TEST_MARIADB_CONNECTION_JSON and WEBDB_TEST_SQLSERVER_CONNECTION_JSON, with the same ConnectionInput shape as PostgreSQL and the corresponding provider ID (mysql, mariadb, sqlserver). Run a single provider, for example:

```sh
cargo test --manifest-path backend/Cargo.toml --test providers providers::mysql::browse -- --ignored
```

These tests exercise connectivity, discovery, exact numeric and NULL results, and read-only query rejection. The additional writes tests create and remove randomly named tables in the explicitly configured disposable database, and cover defaults, generated columns, constraints, binary values, quoted identifiers, stale rows, and no-primary-key rejection. They do not provision databases. No live provider tests run without the explicit configuration.

## Editing in MySQL, MariaDB, and SQL Server

Double-click a cell to update it, select a row to delete it, or choose **Insert record**. Inserts and deletes retain the existing APPLY confirmation. Updates and deletes require a primary key and compare every browsed field against the current record before changing it; stale or deleted records are refused. Inserts also work in tables without a primary key. Views and query results remain read only. Generated, identity, rowversion, and unsupported fields cannot be supplied by the record editor. Omitted insert fields use database defaults. NULL and empty text remain distinct. Binary fields accept hexadecimal byte pairs without a 0x prefix; MySQL/MariaDB BIT fields accept hexadecimal digits, while SQL Server bit fields use 0 or 1.

MySQL and MariaDB structured record editing requires InnoDB and enables strict conversion checks to reject truncated/invalid values. It locks the target row and keeps the table metadata stable in a transaction. SQL Server takes a short exclusive table lock while comparing and applying a structured record change; failures roll back. Triggers, foreign keys, cascading rules, defaults and constraints are enforced by the database. Trigger side effects in external systems or nontransactional engines cannot be undone by this tool.

**Database changes** provides a dialect-specific CREATE TABLE example, one-statement table/index DDL and INSERT/UPDATE/DELETE, plus dedicated drop-table actions. Instance mode also enables create/drop database actions, with system-database and active-database protections and no forced client disconnections. Raw transaction control, batches, procedures, and raw CREATE/DROP DATABASE are rejected; use the dedicated database actions. MySQL/MariaDB DDL implicitly commits and cannot be rolled back; raw SQL follows the target storage engine transaction behavior. This is stated in the confirmation dialog. The SQL-change parser accepts a subset of each dialect; SQL Server computed-column DDL is currently unsupported, while existing computed columns can be browsed and remain protected during record editing. SQL Server table/index changes run inside a transaction; database create/drop run separately. Writes commit immediately with no undo. Database settings editing remains PostgreSQL-only.

To run an individual write test, use the same explicit provider connection variable and select, for example:

```sh
cargo test --manifest-path backend/Cargo.toml --test providers providers::mysql::writes -- --ignored
```

## PostGIS map previews

PostgreSQL databases with PostGIS already installed expose a folded-map icon on non-NULL `geometry` and `geography` cells, both in table browsing and SELECT results. Geometry previews are truncated within the normal resizable column width. The cell inspector also offers the map action. Previews are read only and load on demand; the original hexadecimal EWKB value remains available for copying and editing.

The map supports points, lines, polygons, multipart geometries, and collections. Curves are linearized for display. Known SRIDs are transformed to WGS 84 for a 2D preview; extra coordinate dimensions are not visualized. Empty geometries and SRID 0 show an explanation rather than an assumed location. Invalid geometries, unavailable coordinate transformations, and unsupported surface types may fail to preview without affecting table browsing. Individual preview inputs are limited to 1 MiB.

Maps use MapLibre and the bundled VijiTiles Soft Earth style from `maps/vijitiles/web/style.basemap.json`. Vector tiles and fonts load directly from `https://vijimaps.com`; the viewer does not depend on the live style API. Opening a map loads its VijiTiles background; turn off **VijiTiles background** for a plain geometry view. Background requests share your IP address and viewed area with vijimaps.com. Geometry values and record attributes are never sent to the tile service. No extension is installed or database data changed by opening a map. The renderer requires WebGL.

PostGIS integration tests require `WEBDB_TEST_CONNECTION_JSON` pointing to an explicitly disposable database with PostGIS installed: `cargo test --manifest-path backend/Cargo.toml --test postgis -- --ignored`.

## PostgreSQL editing and schema changes

Browse a PostgreSQL base table and double-click a cell (or press Enter on a focused cell) to edit its value in a compact horizontal dialog. Click **Save value** or press **Ctrl/⌘ + Enter** to save. A single row click selects and highlights it, revealing **Delete row** in the table toolbar. Use **Insert record** above the table to add one. Values use PostgreSQL text input (for example `true`, a UUID, JSON text, or `{1,2}` for an array). NULL, empty text, and omitted fields are distinct. Omitted insert fields use database defaults; generated columns are maintained by PostgreSQL. Inserts, deletes, and schema changes require typing `APPLY` and confirming; individual cell edits use **Save value**. Failed changes preserve the draft.

PostgreSQL enforces foreign keys (including configured cascades), UNIQUE, NOT NULL, CHECK, exclusion constraints, column types, permissions, and triggers. Safe error messages explain failures without returning driver details. Updates and deletes lock and compare the complete browsed record before changing it; stale, deleted, or identical duplicate records are refused. This also supports tables without primary keys when the complete record is unique. Views and query results remain read only. Writes commit immediately; there is no undo. Foreign key values are entered manually and checked by PostgreSQL.

Open **Database changes** to create or alter tables, columns, indexes, and constraints using one PostgreSQL SQL statement. The dialog starts with a CREATE TABLE example. This is an explicit write workspace, separate from the read-only query editor. CREATE, ALTER, DROP, TRUNCATE, INSERT, UPDATE, and DELETE are supported inside a transaction. Destructive SQL can affect dependent objects when explicitly specified; review the entire statement before confirming. **Drop table** uses RESTRICT by default. Connect in instance mode to **Create database** or **Drop database**; switch away from a database before dropping it. The dedicated database actions protect system databases and never force-disconnect clients. Commands requiring execution outside a transaction, such as CREATE INDEX CONCURRENTLY, are unsupported in the SQL change dialog.

Record field values are bound parameters, identifiers are quoted, and operations use statement and lock timeouts. The write API requires an active session and explicit confirmation. The application remains a trusted single-user tool, not a shared authenticated database administration service. Use an account with only the privileges needed for the operations you intend to perform.

Write integration tests are opt-in: set `WEBDB_TEST_CONNECTION_JSON` to a ConnectionInput JSON object for an explicitly disposable PostgreSQL database, then run `cargo test --manifest-path backend/Cargo.toml --test postgres_write -- --ignored`. The test creates and drops randomly named schemas and a database and covers typed values, defaults, generated columns, constraints, rollback, stale records, and SQL injection boundaries. With the application running, `npm run test:write-smoke` uses the same explicit connection configuration and `WEBDB_URL` (default `http://127.0.0.1:34616`) to exercise the write API through the frontend proxy. Never point these tests at production.

The interface has a table explorer, sortable and paginated rows, a filter for the current page, column metadata, a record inspector, clipboard copying, and current-page JSON export. Surfaces use contrast and shadows; keyboard focus uses a visible light halo.

The full-width query workspace sits above the table section. Use **+** to open independent query tabs; each tab retains its draft and result page during the current connection. The editor has a scrolling line-number gutter. Click **Run** or press **Ctrl/⌘ + Enter** to run a single read-only SELECT (including CTEs, joins, and aggregates on PostgreSQL). Below it, a compact title and one toolbar hold the table views, filter, refresh, JSON export, and pagination. Results retain cell selection and a compact read-only value dialog on double-click; put ordering in your SQL. **Back to table** restores normal browsing. An ending semicolon is optional; output columns must have unique names, so alias duplicate names in joins. Queries have a 12,000-byte limit, a 10-second statement timeout, and up to 1,000 result rows per page. Page navigation reruns the executed query; use an explicit ORDER BY for predictable paging.

Use **Save** to name a query, **Saved** to open it in its own tab, and **Update** to change its name or SQL. Opening an already open saved query switches to its tab and preserves any draft edits. The saved list lets you remove queries. Saved queries persist only in this browser's local storage (up to 100); loading never executes them. Connection credentials and result rows are not stored with them. Avoid putting secrets in saved SQL. The sample workspace supports `SELECT * FROM public.customers` (also `orders` and `products`) with an optional `LIMIT`; connect PostgreSQL for full SQL support.

## Desktop app and installers

DBsaur can run as a native Tauri desktop application. The installer includes the
static Nuxt interface and Rust API in one application; end users do not need Node,
Rust, Docker, or a separately running web server. Database servers remain external.

Build prerequisites: Node.js 24.11+, Rust 1.95+, the platform's
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), and the Tauri CLI
(`cargo install tauri-cli --locked --version '^2'`). On Windows, install the MSVC
C++ build tools and WebView2. The Windows installer bootstraps WebView2 if needed.

```sh
npm --prefix frontend ci
cargo xtask dev
cargo xtask build
```

`dev` generates a static interface and opens the native window; rerun after UI
changes. `build` generates that interface and creates native installers under
`dist/release/bundle/` (NSIS/MSI on Windows, deb/rpm/AppImage on Linux,
app/DMG on macOS). The Windows standalone executable is
`dist/release/dbsaur-desktop.exe`; Cargo intermediate output also stays in `dist/`.
Debug builds use `dist/debug/`; explicit targets use `dist/<target>/release/`.
Build on the target OS.
Use `cargo xtask build --bundles nsis` for just the Windows setup executable, or
`cargo xtask build --no-bundle` for an executable without an installer.
`cargo xtask frontend` generates assets alone; `cargo xtask check` checks Rust
after generation. Extra dev/build arguments pass through to the Tauri CLI.

### Install locally

```sh
cargo xtask install             # build and install the package for your OS
cargo xtask install --no-build  # install the newest existing package in dist/
cargo xtask install --dry-run   # inspect the existing installer and destination only
```

On Windows, installation requests UAC elevation and waits for setup to finish. Only
setup runs as administrator; builds and the application run normally. Cancelling
UAC or a failed installer returns an error. The NSIS installer is configured for
all users and creates normal uninstall entries. Application data remains per user.
`npm run install:desktop` is an alias. `--dry-run` requires an existing release
installer and makes no changes. Close a running DBsaur before upgrading.

On Linux, `install` builds a `.deb` and uses `sudo apt-get install` on APT-based
distributions, or builds an `.rpm` and uses `sudo dnf install` on DNF-based
distributions. The package manager installs runtime dependencies and desktop
launcher entries. Other distributions can use `cargo xtask build --bundles appimage`;
make the AppImage executable and run it directly. Linux builds need the
[Tauri system libraries](https://v2.tauri.app/start/prerequisites/#linux), including
WebKitGTK 4.1. Release packages use Ubuntu 22.04 as the Linux build baseline.

On macOS, install Xcode command-line tools. `install` builds `DBsaur.app` and copies
it to `/Applications/DBsaur.app` using `sudo ditto`. `--no-build` and `--dry-run`
work on all platforms. To build/install a universal Mac app, install both Rust
targets (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`) and run
`cargo xtask install --universal`. Its output is under
`dist/universal-apple-darwin/release/bundle/`. DMGs can also be installed by dragging
DBsaur into Applications. Mac builds use an ad-hoc signature; distribution without
Gatekeeper warnings requires an Apple Developer certificate and notarization.
See [Tauri signing](https://v2.tauri.app/distribute/sign/macos/).

### Automated desktop releases

`.github/workflows/desktop-release.yml` validates and builds Windows x64 NSIS/MSI
installers and a standalone executable, Linux x64 deb/rpm/AppImage packages, and
universal macOS DMG and app archives for Apple Silicon and Intel. Each platform
adds license notices and a separately named SHA-256 manifest. All platform jobs
must succeed before the combined GitHub Release is published. Pull
requests and manual workflow runs build downloadable Actions artifacts; each
successful push to `main` also publishes a GitHub Release. Releases use unique
`main-<run-number>-<attempt>` tags at the pushed commit, with installer version
`<major>.<minor>.<run-number>` derived from `tauri.conf.json`. No manual version
bump or personal token is needed. Only the publishing job gets `contents: write`;
the release is published after its assets upload successfully. The workflow must
be committed and pushed before it can run. Windows/Linux packages are unsigned;
macOS uses an ad-hoc signature unless Developer ID signing is configured.
Run `npm run test:packaging` for package-selection and release-collection tests.

The desktop API runs inside the app on loopback port 34618, with a per-launch
private cookie and same-origin mutation checks. Closing the app stops the API.
If that port is occupied, startup fails instead of connecting to another process.
The stable origin preserves saved SQL and preferences. Saved connections use the
existing encrypted SQLite vault in the OS per-user application data directory
(`com.dbsaur.desktop`), with a generated local `vault.key`. Keep that key private
and back it up together with `connections.sqlite`; deleting it makes saved
credentials unrecoverable. This protects stored ciphertext, not a compromised OS
account. Desktop startup does not read the web deployment's vault variables.

## Run locally (web development)

The gear beside the selected database opens tabbed PostgreSQL settings: General, Performance, Queries, Locale, Advanced, and Server. Database owners and administrators can edit the connection limit and permitted database defaults, including memory, timeouts, transaction behavior, and locale. Enable **Override for this database** to set a value; disable it to remove the database override. **Save changes** applies the batch in a transaction. Unsaved drafts survive tab changes, and closing asks whether to discard them. Database defaults affect new connections; role defaults can take precedence. Server-wide, immutable, sensitive, and permission-restricted parameters explain their restrictions instead of offering an unsupported save. No server configuration files are changed.

The settings integration test is opt-in and requires `WEBDB_TEST_CONNECTION_JSON` pointing to an explicitly disposable PostgreSQL instance whose test user has CREATEDB. Run `cargo test --manifest-path backend/Cargo.toml --test postgres_settings -- --ignored`. It creates and drops its own randomly named database and checks saved defaults, resets, rollback, stale values, and SQL quoting.

Requires Node.js 24 LTS (24.11+) and Rust 1.95+. Docker is optional.

```sh
npm --prefix frontend ci
npm run dev
```

Open **http://127.0.0.1:34616**. The Rust API listens on `127.0.0.1:34617`. The first Rust build downloads and compiles dependencies. Alternatively run `npm run dev:backend` and `npm run dev:frontend` in separate terminals.

Use **Open sample workspace** to try browsing immediately. To connect PostgreSQL, enter its host, port, username, and password. Enter a database name for a single database, or enable **Browse all databases in this instance** to hide that field. Instance mode connects through the `postgres` maintenance database (the account needs CONNECT access to it), discovers non-template databases with CONNECT permission, and lists databases as expandable branches in the left drawer. Expand several databases to inspect their schemas and tables without changing the active database. Choosing a table in another database switches the active database after checking access; table results and query tabs reset for the new database. PostgreSQL permissions still govern which tables can be read. The drawer starts at 320px on desktop; drag its right edge to resize, or focus the separator and use Left/Right arrows (Shift for larger steps), Home/End for the bounds, and double-click to reset. Width is constrained on smaller screens. TLS defaults to disabled for new connections. Enable **Verify TLS connection** when your server supports TLS; this requires certificate and hostname verification. Docker connections use the host as seen from the backend container: use `host.docker.internal` for a database on the Windows host, or the reachable hostname of your existing PostgreSQL server.

## Saved connections

The connection page also provides **Test connection**, **Import from URL**, password visibility, and optional **Read-only mode**. Testing does not create or replace a session or save a profile. Read-only mode blocks record/schema mutations and database-settings writes on the backend and survives saved reconnects and database switches. Existing profiles keep their previous editing behavior.

URL import accepts `postgres://`, `postgresql://`, `mysql://`, `mariadb://`, and `sqlserver://` URI syntax, with percent-encoded credentials. Omit the database to select instance discovery. TLS defaults to disabled when no TLS option is supplied; accepted URL options are `sslmode`, `ssl-mode`, `ssl`, or `encrypt` (only one), with verified values `verify-full`, `VERIFY_IDENTITY`, or `true`, or disabled values `disable`, `DISABLED`, or `false`. Other options and TLS modes are rejected. ADO/JDBC connection strings are unsupported.

Open the **Verify TLS connection** modal to upload an optional PEM CA certificate and a paired client certificate/private key for PostgreSQL, MySQL, or MariaDB (32 KiB each, unencrypted private keys). Files are sent as contents, never server filesystem paths, and stored only in session memory unless saving to the encrypted backend vault is selected. SQL Server custom certificates and built-in SSH tunnels are not yet supported. See the [screenshot audit](docs/connection-page-audit.md) for the feature comparison and remaining priorities.

Enable **Save connection and credentials** to name a connection after it successfully connects. Saved connections appear on the setup screen and in **New connection**, with actions to reconnect or remove them. Reconnect runs on the backend and never returns a saved password to the browser. Disconnect ends the active session; removing a saved entry deletes its stored copy and leaves an active session running.

Saving is disabled until the backend has both `WEBDB_VAULT_KEY` (32 random bytes encoded as 64 hex characters) and `WEBDB_STORE_PATH` (a writable SQLite file in an existing private directory). Configuration errors fail startup. The database stores connection names as plaintext labels and encrypts all connection details using AES-256-GCM, fresh random nonces, and the connection ID as authenticated data. There is no browser credential persistence. Keep the encryption key separate from the SQLite file and its backups; losing the key makes saved credentials unrecoverable. Use names without secrets and restrict the directory and key to your OS account.

**Docker Compose:** no host storage path is needed. Both Compose files already create a named volume automatically (`dev-connections` or `prod-connections`) and mount it at `/var/lib/webdb`. The backend sets `WEBDB_STORE_PATH=/var/lib/webdb/connections.sqlite` inside that volume and creates the SQLite file automatically. Set `WEBDB_VAULT_KEY` in `.env`, then start Compose. The path example below applies only to running directly on Windows with `npm run dev`.

For local PowerShell development, start in the project root. This creates a new key only if one does not already exist; use the same private key file on later starts:

```powershell
$vaultDirectory = Join-Path $env:LOCALAPPDATA 'WebDB'
New-Item -ItemType Directory -Force -Path $vaultDirectory | Out-Null
$vaultKeyFile = Join-Path $vaultDirectory 'vault.key'
if (!(Test-Path -LiteralPath $vaultKeyFile)) {
  $vaultKeyBytes = New-Object byte[] 32
  $vaultRng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
  $vaultRng.GetBytes($vaultKeyBytes)
  $vaultRng.Dispose()
  [System.IO.File]::WriteAllText($vaultKeyFile, [BitConverter]::ToString($vaultKeyBytes).Replace('-', '').ToLowerInvariant())
}
$env:WEBDB_VAULT_KEY = [System.IO.File]::ReadAllText($vaultKeyFile).Trim()
$env:WEBDB_STORE_PATH = Join-Path $vaultDirectory 'connections.sqlite'
npm run dev
```

The dev launcher inherits these variables; it does not load them from `.env`. Both Compose files enable saved connections using `/var/lib/webdb/connections.sqlite` inside the backend, backed by separate `dev-connections` and `prod-connections` named volumes. SQLite runs in the backend process and needs no separate service. The production image prepares the volume directory for UID 10001; the rest of its filesystem stays read only. Compose requires a persistent `WEBDB_VAULT_KEY` supplied through `.env` or the process environment. Keep the same key across restarts and backups; changing it makes existing saved credentials unreadable. Keep the API private: saved connections belong to the local tool, with no separate per-user accounts or authorization.

## Architecture

```text
frontend/
  app/components/       Connection form, data grid, record inspector, icons
  app/composables/      Typed workspace state and request lifecycle
  app/providers/        Dedicated provider directories for ports and SQL dialects
  shared/              Browser API contracts
  server/api/          Same-origin proxy; Rust address stays server-side
backend/src/
  domain.rs            Provider-neutral inputs, table/column/page models
  application.rs       Adapter contract, provider registry, sanitized errors
  adapters/postgres/    PostgreSQL connection, metadata, reads, writes and settings
  adapters/mysql/       MySQL connection, metadata, read queries and write adapter
  adapters/mariadb/     MariaDB registration and shared MySQL protocol adapter
  adapters/sqlserver/   SQL Server TDS connection, metadata, read and write adapters
  adapters/read_query.rs Shared SELECT validation and operation deadline
  adapters/demo.rs      Deterministic sample workspace
  adapters/vault.rs     SQLite storage with authenticated credential encryption
  sessions.rs          Bounded, expiring in-memory connection sessions
  lib.rs               HTTP transport and dependency composition
  main.rs              Process configuration and shutdown
```

To add a database, implement `DatabaseAdapter` in a dedicated adapter directory, register it in `router_with_repository()`, and add any provider-specific form fields. Supported providers are fetched from `/api/providers`. Saved connections use a separate local SQLite application store behind the `ConnectionRepository` contract. Its schema initializes automatically; connected database schemas are never changed.

## Commands and checks

```sh
npm run typecheck
npm run lint
npm test
npm run build
npm run test:smoke   # requires the frontend and backend to be running
npm run format
cargo install cargo-audit --locked
npm run audit
```

The real PostgreSQL integration tests are explicitly opt-in. The browsing test creates and drops a randomly named test schema, checks exact numeric serialization and quoted names, and verifies that a view cannot write through a read-only transaction. Supply your own explicitly disposable database using the same `WEBDB_TEST_CONNECTION_JSON` configuration as the write tests:

```sh
export WEBDB_TEST_CONNECTION_JSON='{"provider":"postgres","host":"127.0.0.1","port":55432,"database":"webdb_test","username":"postgres","password":"YOUR_TEST_PASSWORD","ssl":false,"allDatabases":false}'
cargo test --manifest-path backend/Cargo.toml -- --include-ignored
```

PowerShell uses `$env:WEBDB_TEST_CONNECTION_JSON='...'` instead of `export`. DBsaur does not provision PostgreSQL or seed a database.

## Docker

Copy `.env.example` to `.env`. Generate a key once with the following command and set `WEBDB_VAULT_KEY` in `.env` to its output before starting either environment:

```sh
node -e "console.log(require('node:crypto').randomBytes(32).toString('hex'))"
```

```sh
docker compose -f docker-compose.dev.yml up -d
docker compose -f docker-compose.prod.yml up -d --build
```

Both bind the browser entrypoint to localhost and keep the API private. Development uses bind-mounted source; restart the backend service after Rust edits. Production uses multistage builds, unprivileged users, read-only filesystems, and a frontend health check. Container images need their own vulnerability scans before deployment; dependency scans do not cover operating system packages.

Saved connections persist in the SQLite volumes across container rebuilds and ordinary `down` commands. Back up the SQLite store and retain its encryption key separately. The built-in sample workspace uses in-memory data; connect to your own PostgreSQL server for real database operations.

Optional Bash helpers (Git Bash, WSL, Linux or macOS):

```sh
bash scripts/rollup.sh dev          # or prod; supports --no-cache
bash scripts/logs.sh dev backend
bash scripts/down.sh dev            # preserves volumes
bash scripts/down.sh dev volumes    # explicitly deletes project volumes, including saved connections
```

All scripts support `--help`. `migrate` reports that the SQLite connection-store schema initializes automatically at backend startup.

## Security and scope

DBsaur is a local, single-user tool. The browser sends credentials only to the local API through the Nuxt proxy; no third-party services, font CDNs, analytics, or persistent browser credential storage are used. Password fields clear on submission. Unless you opt into saving a connection, credentials stay in bounded server memory, expire after eight hours, and are removed on disconnect or process exit; a periodic cleanup removes expired entries within a minute. Sessions use random opaque IDs in `HttpOnly`, `SameSite=Strict` cookies. Refreshing the page restores a valid session without returning its password.

Browsing uses query validation (plus read-only transactions where supported), statement and lock timeouts, quoted identifiers, bound parameters, and at most 1,000 rows per page. Use a PostgreSQL account with only the necessary privileges; browsing needs `SELECT`, while explicit editing requires corresponding write privileges. Large integers and decimals are returned as strings. JSON and array columns retain their database text representation to preserve nested numeric precision. Sorting uses native database values. Pagination is offset-based: concurrent changes can shift rows, and views without a sortable unique key have no guaranteed stable order. Large cell contents are returned in full; page size is not a response-byte limit.

Keep the API private. Remote/multi-user deployment requires an authenticated entrypoint, HTTPS, network access controls for target databases, and `WEBDB_SECURE_COOKIES=true`. Because connecting to private database hosts is an intentional feature, the API must not be exposed as an unauthenticated public connection proxy. Behind an HTTPS reverse proxy, pass the original Host and `X-Forwarded-Proto` consistently so same-origin validation matches the public URL. Configure `NUXT_API_BASE` for the private backend address. Run `WEBDB_BIND=0.0.0.0:34617` only on an appropriately isolated network.

See [SECURITY.md](SECURITY.md) for vulnerability reporting, scope, and dated dependency scan results. A clean dependency scan cannot guarantee the absence of all vulnerabilities.

## License and responsibility

Original DBsaur code and documentation are available under the [MIT license](LICENSE),
including its warranty disclaimer and limitation of liability. Third-party packages
and assets retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
The license does not grant trademark rights or imply endorsement by database vendors.

Use only databases you are authorized to access. You control the queries, changes,
exports, and credentials you submit. Keep backups and verify important results.
DBsaur is provided as is, without guaranteed support, fitness for a particular purpose,
or security or regulatory certification. Installation operators must provide any
privacy disclosures, legal grounds, retention policies, and access controls required
for their own deployment. The in-app privacy notice describes software behavior;
it is not a complete privacy policy for every hosting arrangement.
