//! Admin API — only reachable from 127.0.0.1, separate port.
//! All endpoints require X-Admin-Token header matching ADMIN_TOKEN.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
    routing::{delete, post},
    Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::db::{self, User};
use crate::error::AppError;
use surrealdb::Surreal;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct AdminState {
    pub db: Arc<Surreal<surrealdb::engine::local::Db>>,
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
struct CreateUserBody {
    name: String,
}

async fn add_user(
    State(state): State<Arc<AdminState>>,
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

async fn list_users(
    State(state): State<Arc<AdminState>>,
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

async fn delete_user(
    State(state): State<Arc<AdminState>>,
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

async fn rotate_token(
    State(state): State<Arc<AdminState>>,
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

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(state: Arc<AdminState>) -> Router {
    Router::new()
        .route("/users", post(add_user).get(list_users))
        .route("/users/{id}", delete(delete_user))
        .route("/users/{id}/rotate", post(rotate_token))
        .with_state(state)
}
