use common::WsServerMessage;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, broadcast, RwLock};

use crate::messaging::MessageService;

type UserId = i64;

/*
* Stato condiviso tra tutti gli handler axum.
* Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,
    pub tokens: Mutex<HashMap<String, UserId>>, // token -> user_id
    pub clients: RwLock<HashMap<UserId, mpsc::Sender<WsServerMessage>>>, // canali diretti
    pub broadcast_tx: broadcast::Sender<WsServerMessage>, // canale broadcast
    pub message_service: MessageService, // servizio di persistenza e gestione dei messaggi
}

impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        let (broadcast_tx, _) = broadcast::channel(100); // buffer size 100
        Arc::new(Self {
            db: db.clone(),
            tokens: Mutex::new(HashMap::new()),
            clients: RwLock::new(HashMap::new()),
            broadcast_tx,
            message_service: MessageService::new(db.clone()),
        })
    }

    pub async fn register_client(&self, user_id: UserId, tx: mpsc::Sender<WsServerMessage>) {
        let mut clients = self.clients.write().await;
        clients.insert(user_id, tx);
    }

    pub fn register_broadcast(&self) -> broadcast::Receiver<WsServerMessage> {
        self.broadcast_tx.subscribe()
    }

    pub async fn unregister_client(&self, user_id: UserId) {
        let mut clients = self.clients.write().await;
        clients.remove(&user_id);
    }
}
