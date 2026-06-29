//! MCP (Model Context Protocol) endpoint — streamable HTTP on the admin port.
//!
//! Exposes 5 tools via the admin port's `/mcp` path:
//!   create_user    — Admin: create a user
//!   list_users     — Admin: list all users
//!   delete_user    — Admin: delete a user and their data
//!   rotate_token   — Admin: rotate a user's API token
//!   get_locations  — Public: query location records for a given user token

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ErrorCode},
    schemars, tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData,
};
use serde::Deserialize;

use crate::db;

// ---------------------------------------------------------------------------
// MCP handler struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct HereMcp {
    db: Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    admin_token: String,
}

impl HereMcp {
    pub fn new(
        db: Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
        admin_token: String,
    ) -> Self {
        Self { db, admin_token }
    }
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CreateUserRequest {
    /// Admin token for authentication
    pub admin_token: String,
    /// Display name for the new user
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ListUsersRequest {
    /// Admin token for authentication
    pub admin_token: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct DeleteUserRequest {
    /// Admin token for authentication
    pub admin_token: String,
    /// User ID (e.g. "users:xxxxx")
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RotateTokenRequest {
    /// Admin token for authentication
    pub admin_token: String,
    /// User ID (e.g. "users:xxxxx")
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetLocationsRequest {
    /// User API token
    pub token: String,
    /// Max records to return (default 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn internal_error(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INTERNAL_ERROR, msg.into(), None)
}

fn invalid_params(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

fn json_result(value: impl serde::Serialize) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(&value).map_err(|e| internal_error(e.to_string()))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router(server_handler)]
impl HereMcp {
    /// Create a new user. Returns the user's ID, name, and API token.
    #[tool(
        description = "Create a new user. Requires admin_token. Returns user ID, name, and API token."
    )]
    async fn create_user(
        &self,
        Parameters(req): Parameters<CreateUserRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if req.admin_token != self.admin_token {
            return Err(invalid_params("Invalid admin_token"));
        }

        let user = db::create_user(&self.db, &req.name)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        json_result(serde_json::json!({
            "id": user.id_str(),
            "name": user.name,
            "api_token": user.api_token,
        }))
    }

    /// List all registered users.
    #[tool(description = "List all registered users. Requires admin_token.")]
    async fn list_users(
        &self,
        Parameters(req): Parameters<ListUsersRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if req.admin_token != self.admin_token {
            return Err(invalid_params("Invalid admin_token"));
        }

        let users = db::list_users(&self.db)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let json: Vec<_> = users
            .iter()
            .map(|u| {
                serde_json::json!({
                    "id": u.id_str(),
                    "name": u.name,
                    "api_token": u.api_token,
                })
            })
            .collect();

        json_result(json)
    }

    /// Delete a user and all their location data.
    #[tool(
        description = "Delete a user and all their location data. Requires admin_token and user ID."
    )]
    async fn delete_user(
        &self,
        Parameters(req): Parameters<DeleteUserRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if req.admin_token != self.admin_token {
            return Err(invalid_params("Invalid admin_token"));
        }

        db::delete_user(&self.db, &req.id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        json_result(serde_json::json!({"ok": true, "deleted": req.id}))
    }

    /// Rotate (regenerate) a user's API token.
    #[tool(
        description = "Rotate (regenerate) a user's API token. Requires admin_token and user ID. Returns the new token."
    )]
    async fn rotate_token(
        &self,
        Parameters(req): Parameters<RotateTokenRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if req.admin_token != self.admin_token {
            return Err(invalid_params("Invalid admin_token"));
        }

        let token = db::rotate_user_token(&self.db, &req.id)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        json_result(serde_json::json!({
            "ok": true,
            "id": req.id,
            "api_token": token,
        }))
    }

    /// Query recent location records for a user, identified by their API token.
    #[tool(
        description = "Query recent location records for a user. Requires the user's API token. Returns up to `limit` records (default 50)."
    )]
    async fn get_locations(
        &self,
        Parameters(req): Parameters<GetLocationsRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = db::find_user_by_token(&self.db, &req.token)
            .await
            .map_err(|e| internal_error(e.to_string()))?
            .ok_or_else(|| invalid_params("Invalid user token"))?;

        let user_id = user.id_str();
        let records = db::get_locations(&self.db, &user_id, req.limit)
            .await
            .map_err(|e| internal_error(e.to_string()))?;

        let json: Vec<_> = records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "lat": r.lat,
                    "lon": r.lon,
                    "timestamp": r.timestamp,
                    "source": r.source,
                    "accuracy": r.accuracy,
                    "altitude": r.altitude,
                    "speed": r.speed,
                    "received_at": r.received_at,
                })
            })
            .collect();

        json_result(json)
    }
}

// ---------------------------------------------------------------------------
// Service factory
// ---------------------------------------------------------------------------

/// Build a `StreamableHttpService` that can be nested into an Axum router
/// at `/mcp`.
pub fn create_service(
    db: Arc<surrealdb::Surreal<surrealdb::engine::local::Db>>,
    admin_token: String,
) -> StreamableHttpService<HereMcp, LocalSessionManager> {
    StreamableHttpService::new(
        move || {
            let handler = HereMcp::new(db.clone(), admin_token.clone());
            Ok(handler)
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}
