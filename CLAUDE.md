# CLAUDE.md

鸿蒙 App「我在这里」的定位数据接收后端。Single Rust crate, two binaries.

## Commands

```bash
# Build (produces both binaries)
cargo build --release

# Run tests (currently none)
cargo test --all-features

# Lint
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings

# Run server locally
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
| `here-server` | `src/main.rs` | HTTP daemon: public API + admin API on separate ports |
| `here` | `src/bin/here.rs` | CLI admin tool, talks to the running server over HTTP (ureq) |

Module map (`src/lib.rs` re-exports both as public):

```
src/main.rs      — tokio::main, AppState, handlers (post_location, get_locations, health),
                   extract_token(), uuid_v4(), binds two ports concurrently
src/admin.rs     — AdminState, check_admin(), CRUD handlers, AppError (duplicated), router()
src/db.rs        — SurrealDB init (idempotent schema), User / LocationInput / LocationRecord,
                   all query functions, uuid_v4() (duplicated)
src/bin/here.rs  — CLI: arg dispatch, HTTP calls via ureq (blocking), output formatting
```

## Two servers, one process

`here-server` runs two Axum servers concurrently via `tokio::select!`:

- **Public API** on `0.0.0.0:<PORT>` (default 9001) — for the mobile app
- **Admin API** on `127.0.0.1:<PORT+1>` (default 9002) — for local `here` CLI

Both share the same SurrealDB handle (`Arc<Surreal<Db>>`).

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

All queries use SurrealDB's parameterized `db.query(...).bind(...)` pattern. **Exception**: `delete_user` and `rotate_user_token` use `format!()` string interpolation for the record ID — be careful with this pattern.

Record IDs from SurrealDB come in varied shapes: plain strings (`"users:xxx"`) or objects (`{"tb":"users","id":{"String":"xxx"}}`). The `id_value_to_string()` helper handles both. Use `User::id_str()` to get a stable string ID.

```rust
// Good — parameterized
db.query("SELECT id, name, api_token FROM users WHERE api_token = $api_tok")
   .bind(("api_tok", token.to_string()))

// Format interpolation — only for record IDs, not user input
db.query(format!("DELETE FROM {id}"))
```

## Authentication

Two independent auth systems:

### Public API (mobile app)
- `Authorization: Bearer <token>` (preferred) or `X-Location-Token: <token>` (legacy).
- `extract_token()` checks Bearer first, falls back to legacy header.
- Token looked up in `users` table via `find_user_by_token()`. Returns 401 if missing/invalid.

### Admin API (CLI)
- `X-Admin-Token: <token>` header checked against `ADMIN_TOKEN` env var.
- Plain string equality (`==`), not constant-time. Admin API is localhost-only, so risk is low.
- Admin token auto-generated at startup if not set (prints to log).

## Token generation

`uuid_v4()` in both `main.rs` and `db.rs` (duplicated). Generates hex from timestamp nanoseconds:
```rust
format!("{:x}-{:x}", ts >> 32, ts & 0xFFFF_FFFF)
```
This is deterministic (clock-based), not cryptographic. Fine for a personal project.

## Error handling

`AppError` enum with `IntoResponse` — defined **twice** (identical copies in `main.rs` and `admin.rs`). Variants:
- `Unauthorized` → 401
- `Internal(String)` → 500 (logs error message, returns generic body)

Handlers return `Result<Json<T>, AppError>`. DB errors are mapped via `.map_err(|e| AppError::Internal(e.to_string()))`.

## Configuration (env vars only)

| Variable | Default | Notes |
|----------|---------|-------|
| `PORT` | `9001` | Public API; admin = PORT+1 |
| `DATA_DIR` | `/var/lib/here-server` | SurrealDB persistence |
| `MAX_HOURS` | `24` | Auto-prune locations older than this |
| `ADMIN_TOKEN` | auto-generated | Logged at startup if unset |

Deployed via deb package, config lives at `/etc/here-server/env` (sourced by systemd).

## Code conventions

- Rust 2021 edition, no unsafe code.
- `surrealdb::SurrealValue` derive macro on query result structs (`User`, `LocationRecord`, `CountResult`).
- `serde_json::Value` for SurrealDB record `id` field (handles varied shapes).
- `Arc<Surreal<Db>>` passed through Axum state — no connection pool needed (embedded DB).
- Logging via `tracing` crate with env-filter support.
- No middleware — auth is manual in each handler.

## Known issues / debt

1. **No tests.** CI runs `cargo test` but there are zero test functions.
2. **`AppError` duplicated** in `main.rs` and `admin.rs`. Should be extracted to a shared module.
3. **`uuid_v4()` duplicated** in `main.rs` and `db.rs`. Only the one in `db.rs` is actually used (the one in `main.rs` is for admin token, the one in `db.rs` is for user tokens).
4. **String interpolation in queries** (`delete_user`, `rotate_user_token`) — safe for record IDs but fragile if callers change.
5. **Token generation is predictable** — timestamp-based, not `rand`. Adequate for a personal project on localhost.
6. **No rate limiting or DoS protection.**
7. **`id_value_to_string`** returns `"users:unknown"` as a fallback — this could silently hide bugs.
