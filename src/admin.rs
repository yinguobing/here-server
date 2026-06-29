//! Admin API — user management endpoints.
//! All endpoints require X-Admin-Token header matching ADMIN_TOKEN.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::db::{self, User};
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Shared app state (used by public + admin + MCP)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    pub max_hours: i64,
    pub admin_token: String,
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

fn check_admin(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get("X-Admin-Token")
        .and_then(|v| v.to_str().ok())
        .map(|t| t == expected)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateUserBody {
    name: String,
}

pub async fn add_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateUserBody>,
) -> Result<Json<User>, AppError> {
    if !check_admin(&headers, &state.admin_token) {
        return Err(AppError::Unauthorized);
    }
    let user = db::create_user(&state.db, &body.name).await.map_err(|e| {
        error!("Admin add-user failed: {e}");
        AppError::Internal(e.to_string())
    })?;
    info!("Admin: created user {} (id={})", user.name, user.id_str());
    Ok(Json(user))
}

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<User>>, AppError> {
    if !check_admin(&headers, &state.admin_token) {
        return Err(AppError::Unauthorized);
    }
    let users = db::list_users(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(users))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !check_admin(&headers, &state.admin_token) {
        return Err(AppError::Unauthorized);
    }
    db::delete_user(&state.db, &id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    info!("Admin: deleted user {id}");
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn rotate_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !check_admin(&headers, &state.admin_token) {
        return Err(AppError::Unauthorized);
    }
    let token = db::rotate_user_token(&state.db, &id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    info!("Admin: rotated token for user {id}");
    Ok(Json(serde_json::json!({"token": token})))
}
