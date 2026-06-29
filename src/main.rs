use std::env;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use chrono::{TimeDelta, Utc};
use serde::Deserialize;
use tracing::{error, info, warn};

use db::{find_user_by_token, insert_location, prune_old_locations, LocationInput};
use here_server::admin::{self, AppState};
use here_server::db;
use here_server::error::AppError;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LocationQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, serde::Serialize)]
struct PostResponse {
    ok: bool,
    count: usize,
}

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------

fn extract_token(headers: &HeaderMap) -> Option<&str> {
    if let Some(auth) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            return Some(token);
        }
    }
    headers
        .get("X-Location-Token")
        .and_then(|v| v.to_str().ok())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn post_location(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<LocationInput>,
) -> Result<Json<PostResponse>, AppError> {
    let token = extract_token(&headers).unwrap_or("");
    let user = find_user_by_token(&state.db, token)
        .await
        .map_err(|e| {
            error!("DB error: {e}");
            AppError::Internal(e.to_string())
        })?
        .ok_or(AppError::Unauthorized)?;

    let user_id = user.id_str();

    info!(
        "POST /location user={} lat={:.6} lon={:.6} ts={}",
        user_id, payload.lat, payload.lon, payload.timestamp
    );

    let received_at = Utc::now().to_rfc3339();
    insert_location(&state.db, &user_id, &payload, &received_at)
        .await
        .map_err(|e| {
            error!("Failed to insert location: {e}");
            AppError::Internal(e.to_string())
        })?;

    // Prune old records for this user
    let cutoff = Utc::now()
        .checked_sub_signed(TimeDelta::hours(state.max_hours))
        .unwrap_or_else(Utc::now)
        .timestamp();
    prune_old_locations(&state.db, &user_id, cutoff)
        .await
        .map_err(|e| {
            error!("Failed to prune: {e}");
        })
        .ok();

    // Count remaining records for this user
    let count = db::count_locations(&state.db, &user_id)
        .await
        .map_err(|e| {
            error!("Failed to count: {e}");
            AppError::Internal(e.to_string())
        })?;

    Ok(Json(PostResponse { ok: true, count }))
}

async fn get_locations(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LocationQuery>,
) -> Result<Json<Vec<db::LocationRecord>>, AppError> {
    let token = extract_token(&headers).unwrap_or("");
    let user = find_user_by_token(&state.db, token)
        .await
        .map_err(|e| {
            error!("DB error: {e}");
            AppError::Internal(e.to_string())
        })?
        .ok_or(AppError::Unauthorized)?;

    let user_id = user.id_str();

    let records = db::get_locations(&state.db, &user_id, q.limit)
        .await
        .map_err(|e| {
            error!("Failed to query locations: {e}");
            AppError::Internal(e.to_string())
        })?;

    Ok(Json(records))
}

async fn health() -> &'static str {
    "ok"
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path = env::var("DATA_DIR").unwrap_or_else(|_| "/var/lib/here-server".into());
    let max_hours: i64 = env::var("MAX_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let db = Arc::new(db::init(&db_path).await.unwrap_or_else(|e| {
        eprintln!("Failed to initialize database at {db_path}: {e}");
        std::process::exit(1);
    }));

    // --- Admin token ---
    let admin_token = env::var("ADMIN_TOKEN").unwrap_or_else(|_| {
        let token = uuid::Uuid::new_v4().to_string();
        warn!("ADMIN_TOKEN not set — auto-generated. Set ADMIN_TOKEN env var to fix.");
        info!("Auto-generated ADMIN_TOKEN: {token}");
        token
    });

    // --- Build the unified router ---
    let state = Arc::new(AppState {
        db: db.clone(),
        max_hours,
        admin_token: admin_token.clone(),
    });
    let mcp_service = here_server::mcp::create_service(db.clone(), admin_token.clone());

    let app = Router::new()
        .route("/location", get(get_locations).post(post_location))
        .route("/health", get(health))
        .route("/users", post(admin::add_user).get(admin::list_users))
        .route("/users/{id}", delete(admin::delete_user))
        .route("/users/{id}/rotate", post(admin::rotate_token))
        .with_state(state.clone())
        .nest_service("/mcp", mcp_service);

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9001);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Failed to bind port {port}: {e}");
            std::process::exit(1);
        });

    info!("Server listening on 0.0.0.0:{port}");

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        error!("Server error: {e}");
    });
}
