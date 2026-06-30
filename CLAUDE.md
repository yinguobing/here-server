# CLAUDE.md

鸿蒙 App「我在这里」的定位数据接收后端。Single Rust crate, two binaries.

## Commands

```bash
# Build (produces both binaries)
cargo build --release

# Run tests
cargo test --all-features

# Lint
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings

# Run server locally (single port, all endpoints)
DATA_DIR=/tmp/here-data PORT=9001 ADMIN_TOKEN=test cargo run --bin here-server

# CLI (requires server to be running)
ADMIN_TOKEN=test here add-user "name"
ADMIN_TOKEN=test here list-users
ADMIN_TOKEN=test here delete-user <id>
ADMIN_TOKEN=test here rotate-token <id>

# Build deb package
cargo install cargo-deb && cargo deb
```

## Architecture

Single crate → two binaries from `src/`:

| Binary | Entry | Role |
|--------|-------|------|
| `here-server` | `src/main.rs` | HTTP daemon: public API + admin API + MCP on a single port |
| `here` | `src/bin/here.rs` | CLI admin tool, talks to the running server over HTTP (ureq) |

Module map (`src/lib.rs` re-exports as public):

```
src/main.rs      — tokio::main, unified router: public handlers + admin routes + MCP
src/admin.rs     — AppState (shared), check_admin(), CRUD handlers
src/db.rs        — SurrealDB init (idempotent schema), User / LocationInput / LocationRecord,
                   all query functions, uuid_v4() via uuid crate
src/mcp.rs       — MCP streamable HTTP endpoint (rmcp), HereMcp handler, 5 tools
src/error.rs     — shared AppError enum with IntoResponse
src/bin/here.rs  — CLI: arg dispatch, HTTP calls via ureq (blocking), output formatting
```

## Single server, single port

`here-server` runs one Axum server on `0.0.0.0:<PORT>` (default 9001):

```
0.0.0.0:9001
├── GET  /health              (no auth)
├── POST /location            (Bearer token)
├── GET  /location            (Bearer token)
├── POST /users               (X-Admin-Token header)
├── GET  /users               (X-Admin-Token header)
├── DELETE /users/{id}        (X-Admin-Token header)
├── POST /users/{id}/rotate   (X-Admin-Token header)
└── POST /mcp                 (MCP streamable HTTP — auth per tool)
```

All routes share the same `Arc<AppState>` (defined in `admin.rs`):

```rust
pub struct AppState {
    pub db: Arc<Surreal<Db>>,
    pub max_hours: i64,
    pub admin_token: String,
}
```

## Database: SurrealDB embedded

- Engine: `SurrealKv` (RocksDB-backed, file-based, no external server).
- Namespace + database: both `iamhere`.
- Data directory: `DATA_DIR` env var (default `/var/lib/here-server`).
- Schema applied idempotently on startup (`DEFINE ... IF NOT EXISTS`). No migration system — just edit the `DEFINE` statements in `db::init()` and restart.

### Tables

**`users`** (SCHEMAFULL):
- `name` — string
- `api_token` — string, NOT NONE, UNIQUE index
- `max_records` — int, default 10000

**`locations`** (SCHEMAFULL):
- `user_id` — string
- `lat`, `lon` — float
- `timestamp` — int (Unix seconds)
- `source` — string, default `'harmonyos'`
- `accuracy`, `altitude`, `speed` — option<float>
- `received_at` — string (RFC 3339)
- Index: `(user_id, timestamp)`

### Query patterns

All queries use SurrealDB's parameterized `db.query(...).bind(...)` pattern.

Record IDs from SurrealDB come in varied shapes: plain strings (`"users:xxx"`) or objects (`{"tb":"users","id":{"String":"xxx"}}`). The `id_value_to_string()` helper handles both. Use `User::id_str()` to get a stable string ID.

```rust
// Parameterized — the standard pattern
db.query("SELECT id, name, api_token FROM users WHERE api_token = $api_tok")
   .bind(("api_tok", token.to_string()))
```

## Authentication

Two independent auth systems, both via HTTP headers:

| Header | Use | Verification |
|---|---|---|
| `Authorization: Bearer <token>` | User identity (location data) | Lookup in `users` table |
| `X-Location-Token` | Legacy, same as Bearer | Same |
| `X-Admin-Token` | Admin privilege (user mgmt + MCP) | String comparison with `ADMIN_TOKEN` env var |

### Public API (mobile app)
- `extract_token()` checks `Authorization: Bearer` first, falls back to `X-Location-Token`.
- Token looked up in `users` table via `find_user_by_token()`. Returns 401 if missing/invalid.

### Admin API + MCP
- `X-Admin-Token` header checked against `ADMIN_TOKEN` env var.
- Plain string equality (`==`), not constant-time. For a personal project this is fine.
- Admin token auto-generated at startup if not set (prints to log).
- MCP admin tools read `X-Admin-Token` from the HTTP header via `RequestContext<RoleServer>`.

## MCP endpoint

`/mcp` on the same port, built with `rmcp` 1.8.0 (streamable HTTP transport).

5 tools, defined in `src/mcp.rs` via `#[tool_router(server_handler)]`:

| Tool | Auth | Description |
|---|---|---|
| `create_user` | `X-Admin-Token` header | Create a new user |
| `list_users` | `X-Admin-Token` header | List all users |
| `delete_user` | `X-Admin-Token` header | Delete user + their data |
| `rotate_token` | `X-Admin-Token` header | Regenerate user API token |
| `get_locations` | user API token parameter | Query location records |

The `StreamableHttpService` is created via `create_service()` and nested into the Axum
router with `.nest_service("/mcp", ...)`.

## Token generation

Tokens use the `uuid` crate (`Uuid::new_v4()`). No more custom timestamp-based hex.

## Error handling

`AppError` enum in `src/error.rs` (shared by public and admin APIs). Variants:
- `Unauthorized` → 401
- `Internal(String)` → 500 (logs error message, returns generic body)

Handlers return `Result<Json<T>, AppError>`. DB errors are mapped via `.map_err(|e| AppError::Internal(e.to_string()))`.

## Configuration (env vars only)

| Variable | Default | Notes |
|----------|---------|-------|
| `PORT` | `9001` | Single HTTP port for all endpoints |
| `DATA_DIR` | `/var/lib/here-server` | SurrealDB persistence |
| `MAX_HOURS` | `24` | Auto-prune locations older than this |
| `ADMIN_TOKEN` | auto-generated | Admin API + MCP auth, logged at startup if unset |

Deployed via deb package, config lives at `/etc/here-server/env` (sourced by systemd).

## Code conventions

- Rust 2021 edition, no unsafe code.
- `surrealdb::SurrealValue` derive macro on query result structs (`User`, `LocationRecord`, `CountResult`).
- `serde_json::Value` for SurrealDB record `id` field (handles varied shapes).
- `Arc<Surreal<Db>>` passed through Axum state — no connection pool needed (embedded DB).
- `Arc<AppState>` shared by all handlers (public + admin + MCP).
- Logging via `tracing` crate with env-filter support.
- Admin handlers are `pub` (exposed to `main.rs` for inline routing).

## Known issues / debt

1. **No rate limiting or DoS protection.**
2. **`id_value_to_string`** returns `"users:unknown"` as a fallback — could silently hide bugs.
3. **Admin token comparison is not constant-time** (`==`). Low risk for a personal project.
