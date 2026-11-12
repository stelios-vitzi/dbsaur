# DBsaur desktop user guide

[Back to the README](../README.md)

This guide covers provider behavior, queries, editing, PostGIS, and saved
connections in the desktop application. For native installation, build commands, release
artifacts, and the security gate, start with the [README](../README.md).

## Desktop connection storage

The desktop application creates its encrypted connection store automatically in
the per-user application data directory for `com.dbsaur.desktop`. It keeps
`connections.sqlite` and a generated `vault.key` there. Restrict access to this
directory and back up both files securely; losing the key makes stored credentials
unrecoverable. No vault environment variables or separate service are required.
Saved SQL and interface preferences use the embedded webview's local storage.

## Database providers

| Provider         | Default port | Browsing and SELECT queries | Instance discovery              | Editing and settings            |
| ---------------- | ------------ | --------------------------- | ------------------------------- | ------------------------------- |
| PostgreSQL       | 5432         | Yes                         | Yes, through postgres           | Yes                             |
| MySQL 8+         | 3306         | Yes                         | Yes, without a default database | Records and schema; no settings |
| MariaDB 10.6+    | 3306         | Yes                         | Yes, without a default database | Records and schema; no settings |
| SQL Server 2016+ | 1433         | Yes                         | Yes, through master             | Records and schema; no settings |

Select a provider in the connection form; the port, query dialect, completion quoting, and generated SELECT adapt automatically. MySQL schemas correspond to databases. SQL Server uses SQL authentication over TCP; enter a reachable TCP port rather than an instance-name suffix. New connections default to TLS disabled. Enable **Verify TLS connection** using the checkbox to the left of TLS settings to encrypt traffic and verify the server certificate and hostname. Saved connections and database switching work with each provider.

New providers return non-NULL cell values as exact text, including numbers, dates, JSON and booleans; binary values use hexadecimal text. NULL remains JSON null. Table sorting uses native database values. The new providers expose column types, nullability, defaults, and primary keys, with generated and identity fields marked read only. PostgreSQL settings remain exclusive to PostgreSQL.

MySQL and MariaDB browsing runs with session-level read-only transactions and query/lock timeouts. Their query editor accepts one parsed SELECT, including CTEs and joins, and rejects writes, SELECT INTO, locking SELECTs, executable comments, and pass-through table functions. The adapter executes the parsed SQL with backslash escapes disabled. SQL Server has no equivalent read-only transaction mode: its editor validates a single SELECT, rejects those same unsafe constructs, and validates a derived SELECT with the server before execution. For a browse-only permission boundary, use a SELECT-only account. Explicit editing needs corresponding write privileges. SQL Server queries must return named columns and be valid inside a derived table; CTEs and a bare inner ORDER BY are currently unsupported. Use TOP or OFFSET with an inner ORDER BY. Paging without a unique ordering can shift between requests.

Provider tests are opt-in and require explicit connection JSON naming a disposable database. The variables are WEBDB_TEST_MYSQL_CONNECTION_JSON, WEBDB_TEST_MARIADB_CONNECTION_JSON and WEBDB_TEST_SQLSERVER_CONNECTION_JSON, with the same ConnectionInput shape as PostgreSQL and the corresponding provider ID (mysql, mariadb, sqlserver). Run a single provider, for example:

```sh
cargo test --manifest-path backend/Cargo.toml --test providers providers::mysql::browse -- --ignored
```

These tests exercise connectivity, discovery, exact numeric and NULL results, and read-only query rejection. The additional writes tests create and remove randomly named tables in the explicitly configured disposable database, and cover defaults, generated columns, constraints, binary values, quoted identifiers, stale rows, and no-primary-key rejection. They do not provision databases. No live provider tests run without the explicit configuration.

## Editing in MySQL, MariaDB, and SQL Server

Double-click a cell to update it, or right-click a row and choose **Delete row**. Writes require Save confirmation. Updates and deletes require a primary key and compare every browsed field against the current record before changing it; stale or deleted records are refused. Views, joins, aggregates, computed projections, and ambiguous query results remain read only. Direct single-table queries can edit records through their source table, including aliased or partial projections. Hidden full-row snapshots retain primary keys and support stale-row checks; running a query itself never writes data. Generated, identity, rowversion, and unsupported fields cannot be supplied by the record editor. NULL and empty text remain distinct. Binary fields accept hexadecimal byte pairs without a 0x prefix; MySQL/MariaDB BIT fields accept hexadecimal digits, while SQL Server bit fields use 0 or 1.

MySQL and MariaDB structured record editing requires InnoDB and enables strict conversion checks to reject truncated/invalid values. It locks the target row and keeps the table metadata stable in a transaction. SQL Server takes a short exclusive table lock while comparing and applying a structured record change; failures roll back. Triggers, foreign keys, cascading rules, defaults and constraints are enforced by the database. Trigger side effects in external systems or nontransactional engines cannot be undone by this tool.

Right-click a table in the explorer for **View structure** or **Edit table schema**. In the schema editor, select a column to edit its type, nullability, default expression, name, or deletion. Defaults are SQL expressions: quote text defaults, for example 'pending'. Apply or discard new-field/key drafts before editing an existing column. Column edits require a current metadata snapshot and Save confirmation. Deletion also requires typing the column name. Generated, identity, auto-updated, and other special definitions are protected from replacement; rename and deletion remain available subject to database permissions and dependencies.

MySQL/MariaDB text columns expose character set and collation. PostgreSQL encoding belongs to the database, while column collation can be changed. SQL Server uses data types and collations for Unicode/UTF-8 behavior. Type conversions and constraints are enforced by the server; no automatic CASCADE is added. PostgreSQL and SQL Server column edits run in a transaction. MySQL/MariaDB DDL implicitly commits and cannot offer the same snapshot protection against concurrent schema changes. Database settings editing remains PostgreSQL-only. The Insert record and generic Database changes dialogs are no longer part of the interface.

Provider syntax: [PostgreSQL ALTER TABLE](https://www.postgresql.org/docs/current/sql-altertable.html), [MySQL column character sets](https://dev.mysql.com/doc/refman/8.0/en/charset-column.html), and [SQL Server column collations](https://learn.microsoft.com/en-us/sql/relational-databases/collations/set-or-change-the-column-collation).

Table and query footers show the current page and total pages using exact row counts; counts include a query's LIMIT or TOP. Counts run under the existing request/statement timeouts and can take longer on large results. Concurrent changes may shift rows and totals between requests.

Date, time, and datetime cells use the styled calendar and clock controls. Fractional seconds and explicit UTC offsets are preserved without browser timezone conversion. Use **Edit as text** for special values such as PostgreSQL infinity. SQL Server timestamp/rowversion stays a binary value, not a date.

To run an individual write test, use the same explicit provider connection variable and select, for example:

```sh
cargo test --manifest-path backend/Cargo.toml --test providers providers::mysql::writes -- --ignored
```

## PostGIS map previews

PostgreSQL databases with PostGIS already installed expose a folded-map icon on non-NULL `geometry` and `geography` cells, both in table browsing and SELECT results. Geometry previews are truncated within the normal resizable column width. The cell inspector also offers the map action. Previews are read only and load on demand; the original hexadecimal EWKB value remains available for copying and editing.

The map supports points, lines, polygons, multipart geometries, and collections. Curves are linearized for display. Known SRIDs are transformed to WGS 84 for a 2D preview; extra coordinate dimensions are not visualized. Empty geometries and SRID 0 show an explanation rather than an assumed location. Invalid geometries, unavailable coordinate transformations, and unsupported surface types may fail to preview without affecting table browsing. Individual preview inputs are limited to 1 MiB.

Maps use MapLibre and the bundled VijiTiles Soft Earth style in `frontend/public/maps/vijitiles.json`. Vector tiles and fonts load directly from `https://vijimaps.com`; the viewer does not depend on the live style API. Opening a map loads its VijiTiles background; turn off **VijiTiles background** for a plain geometry view. Background requests share your IP address and viewed area with vijimaps.com. Geometry values and record attributes are never sent to the tile service. No extension is installed or database data changed by opening a map. The renderer requires WebGL.

PostGIS integration tests require `WEBDB_TEST_CONNECTION_JSON` pointing to an explicitly disposable database with PostGIS installed: `cargo test --manifest-path backend/Cargo.toml --test postgis -- --ignored`.

## SQLite connections

Choose **SQLite** and enter the absolute path to an existing local database file (for example, `C:\data\example.sqlite` on Windows or `/home/user/example.sqlite` on Linux). No host, port, username, or TLS settings are needed. Connecting opens the file without creating it; instance browsing, database creation/deletion, encrypted SQLite files, and SQLite URI options are unavailable. Manage files outside DBsaur. Saved profiles use the same encrypted connection vault as other providers.

Tables and views appear under `main`, with columns, primary keys, indexes, and foreign keys. Browsing and SELECT queries use read-only file handles, a 10-second execution deadline, and up to 1,000 rows per page. Query results and views remain read only. Integers are returned as exact decimal strings; SQLite REAL values retain SQLite's floating-point precision. Binary values use hexadecimal text. Individual values are limited to 16 MiB. SQLite applies its normal type affinity when writing field text; use explicit SQL casts for storage types requiring precise control.

Record inserts, updates, deletes, table creation/deletion, single-column additions, index changes, and supported SQLite DDL in **Database changes** use transactions with foreign-key enforcement. Updates and deletes require a non-NULL primary key and compare the complete browsed record to reject stale changes. Generated columns are read only. Partial, expression, descending, and custom-collation indexes must be managed with SQL. Use **Database changes** to rename or drop a column. Changing column definitions or primary/unique constraints generally requires rebuilding the table and is unavailable in the structured editor. SQLite pragmas, ATTACH, and transaction-control commands are excluded from the SQL workspaces.

The SQLite integration tests create their own temporary disposable files and run with `cargo test --manifest-path backend/Cargo.toml --locked --test sqlite`; no database server is required.

## PostgreSQL editing and schema changes

Browse a PostgreSQL base table and double-click a cell (or press Enter on a focused cell) to edit its value in a compact horizontal dialog. Click **Save value** or press **Ctrl/⌘ + Enter** to save. A single row click selects and highlights it, revealing **Delete row** in the table toolbar. Use **Insert record** above the table to add one. Values use PostgreSQL text input (for example `true`, a UUID, JSON text, or `{1,2}` for an array). NULL, empty text, and omitted fields are distinct. Omitted insert fields use database defaults; generated columns are maintained by PostgreSQL. Inserts, deletes, and schema changes require typing `APPLY` and confirming; individual cell edits use **Save value**. Failed changes preserve the draft.

PostgreSQL enforces foreign keys (including configured cascades), UNIQUE, NOT NULL, CHECK, exclusion constraints, column types, permissions, and triggers. Safe error messages explain failures without returning driver details. Updates and deletes lock and compare the complete browsed record before changing it; stale, deleted, or identical duplicate records are refused. This also supports tables without primary keys when the complete record is unique. Views and query results remain read only. Writes commit immediately; there is no undo. Foreign key values are entered manually and checked by PostgreSQL.

Open **Database changes** to create or alter tables, columns, indexes, and constraints using one PostgreSQL SQL statement. The dialog starts with a CREATE TABLE example. This is an explicit write workspace, separate from the read-only query editor. CREATE, ALTER, DROP, TRUNCATE, INSERT, UPDATE, and DELETE are supported inside a transaction. Destructive SQL can affect dependent objects when explicitly specified; review the entire statement before confirming. **Drop table** uses RESTRICT by default. Connect in instance mode to **Create database** or **Drop database**; switch away from a database before dropping it. The dedicated database actions protect system databases and never force-disconnect clients. Commands requiring execution outside a transaction, such as CREATE INDEX CONCURRENTLY, are unsupported in the SQL change dialog.

Record field values are bound parameters, identifiers are quoted, and operations use statement and lock timeouts. The write API requires an active session and explicit confirmation. The application remains a trusted single-user tool, not a shared authenticated database administration service. Use an account with only the privileges needed for the operations you intend to perform.

Write integration tests are opt-in: set `WEBDB_TEST_CONNECTION_JSON` to a ConnectionInput JSON object for an explicitly disposable PostgreSQL database, then run `cargo test --manifest-path backend/Cargo.toml --test postgres_write -- --ignored`. The test creates and drops randomly named schemas and a database and covers typed values, defaults, generated columns, constraints, rollback, stale records, and SQL injection boundaries. Never point these tests at production.

The interface has a table explorer, sortable and paginated rows, a filter for the current page, column metadata, a record inspector, clipboard copying, and current-page JSON export. Surfaces use contrast and shadows; keyboard focus uses a visible light halo.

The full-width query workspace sits above the table section. Use **+** to open independent query tabs; each tab retains its draft and result page during the current connection. The editor has a scrolling line-number gutter. Click **Run** or press **Ctrl/⌘ + Enter** to run a single read-only SELECT (including CTEs, joins, and aggregates on PostgreSQL). Below it, a compact title and one toolbar hold the table views, filter, refresh, JSON export, and pagination. Results retain cell selection and a compact read-only value dialog on double-click; put ordering in your SQL. **Back to table** restores normal browsing. An ending semicolon is optional; output columns must have unique names, so alias duplicate names in joins. Queries have a 12,000-byte limit, a 10-second statement timeout, and up to 1,000 result rows per page. Page navigation reruns the executed query; use an explicit ORDER BY for predictable paging.

Use **Save** to name a query, **Saved** to open it in its own tab, and **Update** to change its name or SQL. Opening an already open saved query switches to its tab and preserves any draft edits. The saved list lets you remove queries. Saved queries persist only in the desktop webview's local storage (up to 100); loading never executes them. Connection credentials and result rows are not stored with them. Avoid putting secrets in saved SQL. The sample workspace runs read-only SELECT queries (filters such as `ILIKE` and `IS NOT NULL`, joins, grouping, ordering, and `LIMIT`) against `public.customers`, `public.orders`, and `public.products` in an in-memory SQLite copy; PostgreSQL-specific functions are unavailable, so connect PostgreSQL for full SQL support.

## Saved connections

The connection page also provides **Test connection**, **Import from URL**, password visibility, and optional **Read-only mode**. Testing does not create or replace a session or save a profile. Read-only mode blocks record/schema mutations and database-settings writes on the backend and survives saved reconnects and database switches. Existing profiles keep their previous editing behavior.

URL import accepts `postgres://`, `postgresql://`, `mysql://`, `mariadb://`, and `sqlserver://` URI syntax, with percent-encoded credentials. Omit the database to select instance discovery. TLS defaults to disabled when no TLS option is supplied; accepted URL options are `sslmode`, `ssl-mode`, `ssl`, or `encrypt` (only one), with verified values `verify-full`, `VERIFY_IDENTITY`, or `true`, or disabled values `disable`, `DISABLED`, or `false`. Other options and TLS modes are rejected. ADO/JDBC connection strings are unsupported.

Open the **Verify TLS connection** modal to upload an optional PEM CA certificate and a paired client certificate/private key for PostgreSQL, MySQL, or MariaDB (32 KiB each, unencrypted private keys). Files are sent as contents, never server filesystem paths, and stored only in session memory unless saving to the encrypted backend vault is selected. SQL Server custom certificates and built-in SSH tunnels are not yet supported. See the [screenshot audit](connection-page-audit.md) for the feature comparison and remaining priorities.

Enable **Save connection and credentials** to name a connection after it successfully connects. Saved connections appear on the setup screen and in **New connection**, with actions to reconnect or remove them. Reconnect runs on the backend and never returns a saved password to the desktop interface. Disconnect ends the active session; removing a saved entry deletes its stored copy and leaves an active session running.

Saved profiles are encrypted in the desktop connection store described above.
Connection names remain plaintext labels; all connection details use AES-256-GCM
with fresh nonces and the connection ID as authenticated data. Keep both the
SQLite file and its generated key in secure backups. Credentials are never
persisted in the embedded interface storage.

## Architecture

```text
frontend/
  app/components/       Connection form, data grid, record inspector, icons
  app/composables/      Typed workspace state and request lifecycle
  app/providers/        Dedicated provider directories for ports and SQL dialects
  shared/              Desktop API contracts
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
src-tauri/src/
  main.rs              Native window, encrypted vault and embedded API lifecycle
  server.rs            Desktop loopback session and origin protection
```

To add a database, implement `DatabaseAdapter` in a dedicated adapter directory, register it in `router_with_repository()`, and add any provider-specific form fields. Supported providers are fetched from `/api/providers`. Saved connections use a separate local SQLite application store behind the `ConnectionRepository` contract. Its schema initializes automatically; connected database schemas are never changed.

## Integration tests

The real PostgreSQL integration tests are explicitly opt-in. The browsing test creates and drops a randomly named test schema, checks exact numeric serialization and quoted names, and verifies that a view cannot write through a read-only transaction. Supply your own explicitly disposable database using the same `WEBDB_TEST_CONNECTION_JSON` configuration as the write tests:

```sh
export WEBDB_TEST_CONNECTION_JSON='{"provider":"postgres","host":"127.0.0.1","port":55432,"database":"webdb_test","username":"postgres","password":"YOUR_TEST_PASSWORD","ssl":false,"allDatabases":false}'
cargo test --manifest-path backend/Cargo.toml -- --include-ignored
```

PowerShell uses `$env:WEBDB_TEST_CONNECTION_JSON='...'` instead of `export`. DBsaur does not provision PostgreSQL or seed a database.

## Security and scope

DBsaur is a local, single-user tool. The bundled interface sends credentials to the protected loopback API inside the desktop process. There are no analytics, font CDNs, or persistent browser credentials. Optional PostGIS basemaps load tiles and fonts from vijimaps.com, sharing the client IP address and viewed map area with that service. Password fields clear on submission. Unless you opt into saving a connection, credentials stay in bounded server memory, expire after eight hours, and are removed on disconnect or process exit; a periodic cleanup removes expired entries within a minute. Sessions use random opaque IDs in `HttpOnly`, `SameSite=Strict` cookies. Refreshing the page restores a valid session without returning its password.

Browsing uses query validation (plus read-only transactions where supported), statement and lock timeouts, quoted identifiers, bound parameters, and at most 1,000 rows per page. Use a PostgreSQL account with only the necessary privileges; browsing needs `SELECT`, while explicit editing requires corresponding write privileges. Large integers and decimals are returned as strings. JSON and array columns retain their database text representation to preserve nested numeric precision. Sorting uses native database values. Pagination is offset-based: concurrent changes can shift rows, and views without a sortable unique key have no guaranteed stable order. Large cell contents are returned in full; page size is not a response-byte limit.

The embedded API binds only to `127.0.0.1:34618` and requires the desktop
session token. It exits with the native application. There is no standalone
server, browser deployment, or multi-user hosting mode.

See [SECURITY.md](../SECURITY.md) for vulnerability reporting, scope, and dated dependency scan results. A clean dependency scan cannot guarantee the absence of all vulnerabilities.
