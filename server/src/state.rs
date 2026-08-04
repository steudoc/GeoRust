use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use std::sync::Mutex;

/*
Stato condiviso tra tutti gli handler axum.
Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,
    pub tokens: Mutex<HashMap<String, i64>>, // token -> user_id
}
impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            db,
            tokens: Mutex::new(HashMap::new()),
        })
    }


}