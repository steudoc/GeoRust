use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, broadcast, RwLock};

use common::WsServerMessage;

type UserId = i64;

/*
Stato condiviso tra tutti gli handler axum.
Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,
    pub tokens: Mutex<HashMap<String, UserId>>, // token -> user_id
    pub clients: RwLock<HashMap<UserId, mpsc::Sender<WsServerMessage>>>, // canali diretti
    pub broadcast_tx: broadcast::Sender<WsServerMessage>, // canale broadcast
}
impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        let (broadcast_tx, _) = broadcast::channel(100); // buffer size 100
        Arc::new(Self {
            db,
            tokens: Mutex::new(HashMap::new()),
            clients: RwLock::new(HashMap::new()),
            broadcast_tx,
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

    pub async fn send_direct_message(&self, user_id: &UserId, message: String) -> Result<(), String> {
        let clients = self.clients.read().await;
        if let Some(tx) = clients.get(&user_id) {
            let msg = WsServerMessage::DirectText { id: 0, text: message, timestamp: chrono::Utc::now() };
            match tx.send(msg).await {
                Ok(_) => Ok(()),
                Err(e) => Err(format!("Errore nell'invio del messaggio: {e}")),
            }
        } else {
            Err("Utente {user_id} non connesso".to_string())
        }
    }

    pub fn send_broadcast_message(&self, message: String) -> Result<(), String> {
        let msg = WsServerMessage::BroadcastText { id: 0, text: message, timestamp: chrono::Utc::now() };
        match self.broadcast_tx.send(msg) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Errore nell'invio del messaggio broadcast: {e}")),
        }
    }
}