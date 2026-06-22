use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct AppState {
    token: String,
    data_file: PathBuf,
    max_hours: i64,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LocationInput {
    lat: f64,
    lon: f64,
    timestamp: i64,
    source: String,
    #[serde(default)]
    accuracy: Option<f64>,
    #[serde(default)]
    altitude: Option<f64>,
    #[serde(default)]
    speed: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocationRecord {
    lat: f64,
    lon: f64,
    timestamp: i64,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accuracy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    altitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speed: Option<f64>,
    received_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocationData {
    locations: Vec<LocationRecord>,
}

#[derive(Debug, Deserialize)]
struct LocationQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Serialize)]
struct PostResponse {
    ok: bool,
    count: usize,
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

enum AppError {
    Unauthorized,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
        }
        .into_response()
    }
}

// ---------------------------------------------------------------------------
// Data layer
// ---------------------------------------------------------------------------

fn load_data(path: &std::path::Path) -> LocationData {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(LocationData {
            locations: Vec::new(),
        }),
        Err(_) => LocationData {
            locations: Vec::new(),
        },
    }
}

fn save_data(path: &std::path::Path, data: &LocationData) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(data).map_err(|e| format!("Failed to serialize: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(())
}

fn prune_old(data: &mut LocationData, max_hours: i64) {
    let cutoff = Utc::now()
        .checked_sub_signed(TimeDelta::hours(max_hours))
        .unwrap_or_else(Utc::now);
    let cutoff_ts = cutoff.timestamp();
    data.locations
        .retain(|loc| loc.timestamp > cutoff_ts);
}

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------

fn extract_token(headers: &HeaderMap) -> Option<&str> {
    // Prefer standard Authorization: Bearer header
    if let Some(auth) = headers.get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            return Some(token);
        }
    }
    // Fall back to legacy X-Location-Token
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
    // Token check
    let token = extract_token(&headers).unwrap_or("");
    if token != state.token {
        return Err(AppError::Unauthorized);
    }

    info!(
        "POST /location lat={:.6} lon={:.6} ts={}",
        payload.lat, payload.lon, payload.timestamp
    );

    // Load existing data
    let mut data = load_data(&state.data_file);

    // Append new record
    data.locations.push(LocationRecord {
        lat: payload.lat,
        lon: payload.lon,
        timestamp: payload.timestamp,
        source: payload.source,
        accuracy: payload.accuracy,
        altitude: payload.altitude,
        speed: payload.speed,
        received_at: Utc::now().to_rfc3339(),
    });

    // Prune and save
    prune_old(&mut data, state.max_hours);
    if let Err(e) = save_data(&state.data_file, &data) {
        error!("Failed to save data: {e}");
        // Still return success — the record is in memory even if I/O failed
    }

    Ok(Json(PostResponse {
        ok: true,
        count: data.locations.len(),
    }))
}

async fn get_locations(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LocationQuery>,
) -> Result<Json<Vec<LocationRecord>>, AppError> {
    let token = extract_token(&headers).unwrap_or("");
    if token != state.token {
        return Err(AppError::Unauthorized);
    }

    let data = load_data(&state.data_file);
    let start = data.locations.len().saturating_sub(q.limit);
    let recent: Vec<LocationRecord> = data.locations[start..].to_vec();

    Ok(Json(recent))
}

async fn health() -> &'static str {
    "ok"
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Initialise tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let token = env::var("LOCATION_TOKEN").unwrap_or_else(|_| "change-me-to-a-secret-token".into());
    let data_file = PathBuf::from("/tmp/location.json");
    let max_hours: i64 = env::var("MAX_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);

    let state = Arc::new(AppState {
        token,
        data_file,
        max_hours,
    });

    let app = Router::new()
        .route(
            "/location",
            get(get_locations).post(post_location),
        )
        .route("/health", get(health))
        .with_state(state);

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

    info!("Location receiver running on port {port}");
    axum::serve(listener, app).await.unwrap();
}
