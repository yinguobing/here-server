use serde::{Deserialize, Serialize};
use surrealdb::engine::local::SurrealKv;
use surrealdb::types::SurrealValue;
use surrealdb::Surreal;
use tracing::warn;

// ---------------------------------------------------------------------------
// Input type (shared between handlers and data layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LocationInput {
    pub lat: f64,
    pub lon: f64,
    pub timestamp: i64,
    pub source: String,
    #[serde(default)]
    pub accuracy: Option<f64>,
    #[serde(default)]
    pub altitude: Option<f64>,
    #[serde(default)]
    pub speed: Option<f64>,
}

// ---------------------------------------------------------------------------
// Database init
// ---------------------------------------------------------------------------

pub async fn init(
    path: &str,
) -> Result<Surreal<surrealdb::engine::local::Db>, Box<dyn std::error::Error>> {
    let db = Surreal::new::<SurrealKv>(path).await?;

    db.use_ns("iamhere").use_db("iamhere").await?;

    // Schema (idempotent — IF NOT EXISTS)
    db.query(
        r#"
        DEFINE TABLE IF NOT EXISTS users SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name     ON users TYPE string;
        DEFINE FIELD IF NOT EXISTS api_token   ON users TYPE string ASSERT $value != NONE;
        DEFINE FIELD IF NOT EXISTS max_records ON users TYPE int DEFAULT 10000;
        DEFINE INDEX IF NOT EXISTS idx_token ON users COLUMNS api_token UNIQUE;
        "#,
    )
    .await?;

    db.query(
        r#"
        DEFINE TABLE IF NOT EXISTS locations SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS user_id     ON locations TYPE string;
        DEFINE FIELD IF NOT EXISTS lat         ON locations TYPE float;
        DEFINE FIELD IF NOT EXISTS lon         ON locations TYPE float;
        DEFINE FIELD IF NOT EXISTS timestamp   ON locations TYPE int;
        DEFINE FIELD IF NOT EXISTS source      ON locations TYPE string DEFAULT 'harmonyos';
        DEFINE FIELD IF NOT EXISTS accuracy    ON locations TYPE option<float>;
        DEFINE FIELD IF NOT EXISTS altitude    ON locations TYPE option<float>;
        DEFINE FIELD IF NOT EXISTS speed       ON locations TYPE option<float>;
        DEFINE FIELD IF NOT EXISTS received_at ON locations TYPE string;
        DEFINE INDEX IF NOT EXISTS idx_user_ts ON locations COLUMNS user_id, timestamp;
        "#,
    )
    .await?;

    Ok(db)
}

// ---------------------------------------------------------------------------
// User types & queries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct User {
    pub id: serde_json::Value,
    pub name: String,
    pub api_token: String,
}

impl User {
    pub fn id_str(&self) -> String {
        id_value_to_string(&self.id)
    }
}

fn id_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => {
            let tb = obj.get("tb").and_then(|v| v.as_str()).unwrap_or("users");
            let rid = obj
                .get("id")
                .and_then(|v| {
                    v.as_str()
                        .or_else(|| v.as_object()?.get("String")?.as_str())
                })
                .unwrap_or("unknown");
            format!("{tb}:{rid}")
        }
        other => {
            warn!("Unexpected SurrealDB record ID format: {other:?}");
            "users:unknown".into()
        }
    }
}

pub async fn find_user_by_token(
    db: &Surreal<surrealdb::engine::local::Db>,
    token: &str,
) -> Result<Option<User>, surrealdb::Error> {
    let mut result = db
        .query("SELECT id, name, api_token FROM users WHERE api_token = $api_tok")
        .bind(("api_tok", token.to_string()))
        .await?;
    let users: Vec<User> = result.take(0)?;
    Ok(users.into_iter().next())
}

pub async fn create_user(
    db: &Surreal<surrealdb::engine::local::Db>,
    name: &str,
) -> Result<User, surrealdb::Error> {
    let token = uuid::Uuid::new_v4().to_string();
    let mut result = db
        .query(
            "CREATE users CONTENT { name: $name, api_token: $api_tok } RETURN id, name, api_token",
        )
        .bind(("name", name.to_string()))
        .bind(("api_tok", token.clone()))
        .await?;
    let mut users: Vec<User> = result.take(0)?;
    let mut user = users.pop().unwrap_or_else(|| User {
        id: serde_json::Value::String("users:unknown".into()),
        name: name.into(),
        api_token: token.clone(),
    });
    user.api_token = token;
    Ok(user)
}

pub async fn list_users(
    db: &Surreal<surrealdb::engine::local::Db>,
) -> Result<Vec<User>, surrealdb::Error> {
    let mut result = db
        .query("SELECT id, name, api_token FROM users ORDER BY id")
        .await?;
    let users: Vec<User> = result.take(0)?;
    Ok(users)
}

pub async fn delete_user(
    db: &Surreal<surrealdb::engine::local::Db>,
    id: &str,
) -> Result<(), surrealdb::Error> {
    let rid = surrealdb::types::RecordId::parse_simple(id)
        .expect("invalid record ID format from database");
    db.query("DELETE FROM locations WHERE user_id = $user_id")
        .bind(("user_id", id.to_string()))
        .await?;
    db.query("DELETE $rid").bind(("rid", rid)).await?;
    Ok(())
}

pub async fn rotate_user_token(
    db: &Surreal<surrealdb::engine::local::Db>,
    id: &str,
) -> Result<String, surrealdb::Error> {
    let token = uuid::Uuid::new_v4().to_string();
    let rid = surrealdb::types::RecordId::parse_simple(id)
        .expect("invalid record ID format from database");
    db.query("UPDATE $rid SET api_token = $api_tok")
        .bind(("rid", rid))
        .bind(("api_tok", token.clone()))
        .await?;
    Ok(token)
}

// ---------------------------------------------------------------------------
// Location types & queries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
pub struct LocationRecord {
    pub id: serde_json::Value,
    pub user_id: String,
    pub lat: f64,
    pub lon: f64,
    pub timestamp: i64,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    pub received_at: String,
}

pub async fn insert_location(
    db: &Surreal<surrealdb::engine::local::Db>,
    user_id: &str,
    input: &LocationInput,
    received_at: &str,
) -> Result<(), surrealdb::Error> {
    db.query(
        r#"
        CREATE locations CONTENT {
            user_id: $user_id,
            lat: $lat,
            lon: $lon,
            timestamp: $ts,
            source: $source,
            accuracy: $accuracy,
            altitude: $altitude,
            speed: $speed,
            received_at: $received_at
        }
        "#,
    )
    .bind(("user_id", user_id.to_string()))
    .bind(("lat", input.lat))
    .bind(("lon", input.lon))
    .bind(("ts", input.timestamp))
    .bind(("source", input.source.clone()))
    .bind(("accuracy", input.accuracy))
    .bind(("altitude", input.altitude))
    .bind(("speed", input.speed))
    .bind(("received_at", received_at.to_string()))
    .await?;
    Ok(())
}

pub async fn get_locations(
    db: &Surreal<surrealdb::engine::local::Db>,
    user_id: &str,
    limit: usize,
) -> Result<Vec<LocationRecord>, surrealdb::Error> {
    let mut result = db
        .query("SELECT * FROM locations WHERE user_id = $uid ORDER BY timestamp DESC LIMIT $limit")
        .bind(("uid", user_id.to_string()))
        .bind(("limit", limit as i64))
        .await?;
    let records: Vec<LocationRecord> = result.take(0)?;
    Ok(records)
}

pub async fn prune_old_locations(
    db: &Surreal<surrealdb::engine::local::Db>,
    user_id: &str,
    cutoff: i64,
) -> Result<(), surrealdb::Error> {
    db.query("DELETE FROM locations WHERE user_id = $uid AND timestamp <= $cutoff")
        .bind(("uid", user_id.to_string()))
        .bind(("cutoff", cutoff))
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize, SurrealValue)]
struct CountResult {
    count: i64,
}

pub async fn count_locations(
    db: &Surreal<surrealdb::engine::local::Db>,
    user_id: &str,
) -> Result<usize, surrealdb::Error> {
    let mut result = db
        .query("SELECT count() FROM locations WHERE user_id = $uid GROUP ALL")
        .bind(("uid", user_id.to_string()))
        .await?;
    let rows: Vec<CountResult> = result.take(0)?;
    Ok(rows.first().map(|r| r.count as usize).unwrap_or(0))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_value_to_string_plain_string() {
        let v = serde_json::json!("users:abc123");
        assert_eq!(id_value_to_string(&v), "users:abc123");
    }

    #[test]
    fn test_id_value_to_string_object_with_string_id() {
        let v = serde_json::json!({
            "tb": "users",
            "id": "abc123"
        });
        assert_eq!(id_value_to_string(&v), "users:abc123");
    }

    #[test]
    fn test_id_value_to_string_object_with_nested_string() {
        let v = serde_json::json!({
            "tb": "users",
            "id": {"String": "abc123"}
        });
        assert_eq!(id_value_to_string(&v), "users:abc123");
    }

    #[test]
    fn test_id_value_to_string_object_missing_tb() {
        let v = serde_json::json!({
            "id": "abc123"
        });
        assert_eq!(id_value_to_string(&v), "users:abc123");
    }

    #[test]
    fn test_id_value_to_string_unknown_format() {
        let v = serde_json::json!(42);
        assert_eq!(id_value_to_string(&v), "users:unknown");
    }

    #[test]
    fn test_id_value_to_string_location_record() {
        let v = serde_json::json!({
            "tb": "locations",
            "id": "xyz789"
        });
        assert_eq!(id_value_to_string(&v), "locations:xyz789");
    }
}
