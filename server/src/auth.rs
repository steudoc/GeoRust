use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use common::{LoginRequest, LoginResponse, RegisterRequest, RegisterResponse};
use rand_core::OsRng;
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

// POST /register
/*
Crea un nuovo utente. 
argon2 genera un salt casuale e produce un hash (il formato è salt + parametri + hash, tutto in una stringa)
*/
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, String)> {
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "username and password cannot be empty".to_string(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .to_string();

    let result = sqlx::query(   // r# -> raw format, automatically escapes char like " or '
        r#"         
        INSERT INTO users (username, password_hash, current_state)
        VALUES (?1, ?2, 'disconnected')
        "#,
    ).bind(&req.username)
    .bind(&password_hash)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            (StatusCode::CONFLICT, "username already registered".to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    Ok(Json(RegisterResponse { 
        user_id: result.last_insert_rowid(),
     }))
}

// POST /login
/*
Effettua il login. Verifica username e password e restituisce un token (UUID)
che il client dovrà riusare per autenticare la connessione al WebSocket
*/
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ?1")
    .bind(&req.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::UNAUTHORIZED, "invalid credentials".to_string()))?;

    let user_id: i64 = row
        .try_get("id")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let password_hash: String = row
        .try_get("password_hash")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let parsed_hash = PasswordHash::new(&password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid credentials".to_string()))?;

    /*
    Nota: qui il token viene generato al volo e NON viene salvato. Per essere più rigorosi, si potrebbe tenere una HashMap<token, user_id> in AppState. Altrimenti va segnalato nel report come semplificazione */
    let token = Uuid::new_v4().to_string();

    Ok(Json(LoginResponse { user_id, token }))
}