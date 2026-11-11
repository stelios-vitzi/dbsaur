# Refactoring review - 11 November 2026

Scope: frontend structure and framework coupling, Rust transport/application/adapter
boundaries, desktop integration, dependency ownership, repository cleanup, and tests.
This is a source review with local checks, not an independent security certification.

## Changes implemented

- Replaced Nuxt with Vue 3 and Vite. Components, composables, Vue APIs, routing,
  and HTTP calls now declare their imports. No SSR, Nitro, server proxy, or
  standalone frontend server is required.
- Added an explicit entry point and router. Legal documents receive typed props;
  routing owns titles and navigation. The workspace remains cached while visiting
  legal pages, and route changes retain a screen-reader announcement.
- Centralized HTTP defaults in [http.ts](../frontend/app/services/http.ts).
  Requests retain same-origin cookies, cancellation, JSON bodies, query encoding,
  and sanitized API messages. Automatic retries are disabled so a failed request
  does not implicitly repeat database operations.
- Replaced Nuxt toast globals with application-provided Vue state. Confirmation
  ownership and disposal cancellation remain in place.
- Extracted connection identity and persistence from database orchestration into
  [connectionPreferences.ts](../frontend/app/services/connectionPreferences.ts).
  Existing storage keys and opaque identities remain compatible. Storage failures
  remain nonfatal, and passwords are excluded from connection identities.
- Separated the Rust composition root in [lib.rs](../backend/src/lib.rs) from
  [HTTP transport](../backend/src/http/mod.rs). The public router functions and
  provider registrations remain compatible; HTTP tests moved with their module.
- Updated Tauri to bundle Vite's frontend/dist output. Source versions are 0.1.5
  in both manifests, both Cargo lockfiles, and Tauri configuration. CI installer
  version derivation and xtask's own version are unchanged.
- Removed Nuxt configuration, its redundant workspace page wrapper, unused local
  braces patches, Forge/braces overrides and their now-inapplicable regression
  tests, the obsolete npm link setting, old generated Nuxt output, audit artifacts,
  and loose build/test logs. Removed a broken frontend/dist junction pointing to
  a previous checkout; build output now stays inside this workspace.
- Retained safety, behavior, packaging, and opt-in disposable-database tests.
  Added connection preference coverage and updated test imports for plain Vue.
  Compiler and map-style test dependencies are declared directly.

## Remaining findings, in recommended order

| Priority | Location                                                                                                              | Finding and next refactor                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Medium   | [useDatabase.ts](../frontend/app/composables/useDatabase.ts)                                                          | Session lifecycle, discovery, row requests, errors, and mutation confirmation still share one orchestrator. Extract browsing and session lifecycle behind explicit interfaces. Row requests have a generation check, while database discovery and table error/loading cleanup do not consistently have equivalent ownership guards. Add deferred-response tests for disconnect/reconnect and overlapping refreshes before splitting these paths. |
| Medium   | [DatabaseWorkspace.vue](../frontend/app/components/DatabaseWorkspace.vue)                                             | The view constructs provider-specific relationship SQL, owns mutation dialogs, builds context menus, and coordinates navigation. Extract relationship commands into a provider-aware service and mutation workflow composable; keep confirmation and refresh orchestration together. Verify each provider's generated SQL rather than spreading dialect branches through the view.                                                               |
| Medium   | [SchemaDiagram.vue](../frontend/app/components/SchemaDiagram.vue)                                                     | Relationship eligibility, draft state, pointer gestures, viewport transforms, edge geometry, rendering, and styling are combined. Extract viewport/gesture state, relationship drafts/validation, and pure edge geometry independently. Preserve the existing interaction tests at each step; layout is already separated in schemaFlowLayout.ts.                                                                                                |
| Medium   | [QueryEditor.vue](../frontend/app/components/QueryEditor.vue)                                                         | Saved-query persistence and validation, draft tabs, and pointer resizing are mixed with editor presentation. Extract a saved-query repository and draft composable, then a resize controller. Preserve the existing saved-query format and limits.                                                                                                                                                                                               |
| Low      | [HTTP module](../backend/src/http/mod.rs)                                                                             | Transport is now isolated from composition but still combines session, connection, browsing, geometry, and mutation handlers. Split by route responsibility when changing those areas, with common state and error mapping in the parent module. Avoid duplicating authorization, limits, or write confirmation checks.                                                                                                                          |
| Low      | [MySQL adapter](../backend/src/adapters/mysql/mod.rs), [SQL Server adapter](../backend/src/adapters/sqlserver/mod.rs) | Connection setup, metadata discovery, projection, and pagination share large provider modules. Extract provider-local connection and metadata modules. Keep dialect differences local and retain DatabaseAdapter as the common boundary; MariaDB intentionally reuses MySQL behavior.                                                                                                                                                            |
| Low      | [Component tests](../frontend/tests/schema-diagram-interaction.test.mjs)                                              | Some interaction tests compile and evaluate component setup with mocked lifecycle APIs. They cover logic but cannot prove DOM focus, real pointer capture, navigation caching, or WebView behavior. Add a small real-renderer or desktop smoke layer before larger interaction refactors.                                                                                                                                                        |

Do not replace the existing domain and adapter boundaries with a generic repository
for arbitrary database operations. Keep identifier quoting provider-specific, values
parameterized, errors sanitized, numeric values exact, and explicit write confirmation
at the operation boundary.

## Verification

- TypeScript, formatting, backend Clippy, backend tests, frontend behavior tests,
  packaging tests, desktop tests, and xtask formatting/Clippy pass on Windows.
- Frontend production build passes. Large JavaScript chunks remain a performance
  follow-up; MapLibre is already dynamically imported.
- Live npm audit: zero findings. All three Cargo lockfile audits: no vulnerability
  findings; existing desktop glib unsoundness and proc-macro-error maintenance
  warnings remain visible. See [SECURITY.md](../SECURITY.md).
- Disposable-database tests remain ignored because no disposable database was
  explicitly configured. No real database was used for verification.
- Local Node is 24.21.0, below the project's required Node 26.13+. CI must verify
  the declared toolchain. Local Tauri CLI is 2.5.0, below the documented 2.12.4;
  cargo xtask build --no-bundle fails configuration validation with that old CLI.
  Windows Cargo desktop tests, cargo xtask check, and a native Windows release
  executable build pass. Installer generation,
  Linux/macOS builds, and signed update installation are not verified here.

## Framework references

The build follows Vite's [static production build](https://vite.dev/guide/build).
Workspace caching follows Vue Router's [RouterView slot and KeepAlive pattern](https://router.vuejs.org/guide/advanced/router-view-slot).
The native server already falls back to index.html for client routes, so existing
/privacy and /terms URLs continue to work with history routing.
