use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;

use crate::messaging::MessageService;

type UserId = i64;

/*
* Stato condiviso tra tutti gli handler axum.
* Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,
    pub tokens: Mutex<HashMap<String, UserId>>, // token -> user_id
    pub message_service: MessageService, // servizio di persistenza e gestione dei messaggi
}

impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            db: db.clone(),
            tokens: Mutex::new(HashMap::new()),
            message_service: MessageService::new(db.clone()),
        })
    }
}
