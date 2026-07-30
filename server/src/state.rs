use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{mpsc, RwLock};

/*
Stato condiviso tra tutti gli handler axum.
Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,

}
impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            db,

        })
    }


}