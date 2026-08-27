use chrono::{DateTime, Local, Utc};
use common::WsServerMessage;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, RwLock};
use thiserror::Error;

// ============================================================================
// MESSAGE ERRORS
// ============================================================================

#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Errore di validazione: {0}")]
    ValidationError(String),

    #[error("Risorsa non trovata: {0}")]
    NotFound(String),

    #[error("Errore del database")]
    DatabaseError(#[from] sqlx::Error),
}

impl MessageError {
    pub fn to_client_message(&self) -> WsServerMessage {
        match self {
            MessageError::ValidationError(msg) => WsServerMessage::Error {
                code: "validation_error".to_string(),
                message: msg.clone(),
            },
            MessageError::NotFound(msg) => WsServerMessage::Error {
                code: "not_found".to_string(),
                message: msg.clone(),
            },
            MessageError::DatabaseError(e) => {
                tracing::error!("Database error: {e}");
                WsServerMessage::Error {
                    code: "internal_error".to_string(),
                    message: "Si è verificato un errore interno del server".to_string(),
                }
            }
        }
    }
}

// ============================================================================
// MESSAGE SERVICE
// ============================================================================

type UserId = i64;

pub struct MessageService {
    db: SqlitePool,
    clients: RwLock<HashMap<UserId, mpsc::Sender<WsServerMessage>>>,
    broadcast_tx: broadcast::Sender<WsServerMessage>,
}

impl MessageService {
    pub fn new(db: SqlitePool) -> Self {
        let (broadcast_tx, _) = broadcast::channel(100);
        Self {
            db,
            clients: RwLock::new(HashMap::new()),
            broadcast_tx,
        }
    }

    pub async fn add_client(&self, user_id: UserId) -> mpsc::Receiver<WsServerMessage> {
        let (tx, rx) = mpsc::channel::<WsServerMessage>(100);
        let mut clients = self.clients.write().await;
        clients.insert(user_id, tx);
        rx
    }

    pub async fn remove_client(&self, user_id: UserId) {
        let mut clients = self.clients.write().await;
        clients.remove(&user_id);
    }

    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<WsServerMessage> {
        self.broadcast_tx.subscribe()
    }

    pub async fn handle_client_message(&self, user_id: UserId, text: &str) -> Result<(), MessageError> {
        let trimmed = text.trim();

        if trimmed.is_empty() {
            return Err(MessageError::ValidationError(
                "Il testo del messaggio non può essere vuoto".to_string(),
            ));
        }
        if trimmed.len() > 200 {
            return Err(MessageError::ValidationError(
                "Il testo del messaggio non può superare i 200 caratteri".to_string(),
            ));
        }

        let timestamp = Utc::now();

        let id = self
            .save_message(Some(user_id), None, "client_to_server", trimmed, timestamp)
            .await?;

        println!(
            "{} [{}] user_{}: {}",
            timestamp.with_timezone(&Local).format("%H:%M:%S"),
            format!("#{}", id),
            user_id,
            trimmed
        );

        Ok(())
    }

    pub async fn acknowledge_message(&self, user_id: UserId, message_id: i64) -> Result<(), MessageError> {
        self.mark_as_read(message_id, user_id).await
    }

    pub async fn send_admin_direct_message(&self, recipient_username: &str, content: &str) -> Result<i64, MessageError> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(MessageError::ValidationError(
                "Il messaggio non può essere vuoto".to_string(),
            ));
        }

        let recipient_id = self.get_recipient_id_by_username(recipient_username).await?;

        let now = Utc::now();
        let msg_id = self
            .save_message(None, Some(recipient_id), "direct", trimmed, now)
            .await?;

        let ws_msg = WsServerMessage::DirectText {
            id: msg_id,
            text: trimmed.to_string(),
            timestamp: now,
        };

        let clients = self.clients.read().await;
        if let Some(tx) = clients.get(&recipient_id) {
            let _ = tx.send(ws_msg).await;
        }

        Ok(msg_id)
    }

    pub async fn send_admin_broadcast_message(&self, content: &str) -> Result<i64, MessageError> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(MessageError::ValidationError(
                "Il messaggio non può essere vuoto".to_string(),
            ));
        }

        let now = Utc::now();
        let msg_id = self
            .save_message(None, None, "broadcast", trimmed, now)
            .await?;

        let ws_msg = WsServerMessage::BroadcastText {
            id: msg_id,
            text: trimmed.to_string(),
            timestamp: now,
        };

        let _ = self.broadcast_tx.send(ws_msg);
        Ok(msg_id)
    }

    async fn save_message(
        &self,
        sender_id: Option<i64>,
        recipient_id: Option<i64>,
        kind: &str,
        content: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<i64, MessageError> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err(MessageError::ValidationError(
                "Impossibile salvare un messaggio vuoto".to_string(),
            ));
        }

        let created_at_ms = timestamp.timestamp_millis();

        let result = sqlx::query(
            r#"
            INSERT INTO messages (sender_id, recipient_id, kind, content, is_read, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, 0, ?5)
            "#,
        )
        .bind(sender_id)
        .bind(recipient_id)
        .bind(kind)
        .bind(trimmed)
        .bind(created_at_ms)
        .execute(&self.db)
        .await?;

        Ok(result.last_insert_rowid())
    }

    async fn mark_as_read(&self, message_id: i64, recipient_id: i64) -> Result<(), MessageError> {
        let result = sqlx::query(
            r#"
            UPDATE messages
            SET is_read = 1
            WHERE id = ?1 AND recipient_id = ?2
            "#,
        )
        .bind(message_id)
        .bind(recipient_id)
        .execute(&self.db)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MessageError::NotFound(format!(
                "Messaggio {} per utente {} non trovato",
                message_id, recipient_id
            )));
        }

        Ok(())
    }

    pub async fn get_unread_messages(&self, client_id: i64) -> Result<Vec<WsServerMessage>, MessageError> {
        let rows = sqlx::query(
            r#"
            SELECT id, content, created_at_ms
            FROM messages
            WHERE recipient_id = ?1 AND kind = 'direct' AND is_read = 0
            ORDER BY created_at_ms ASC
            "#,
        )
        .bind(client_id)
        .fetch_all(&self.db)
        .await?;

        let unread = rows
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let content: String = r.get("content");
                let created_at_ms: i64 = r.get("created_at_ms");
                let timestamp = DateTime::from_timestamp_millis(created_at_ms).unwrap_or_else(Utc::now);

                WsServerMessage::DirectText { id, text: content, timestamp }
            })
            .collect();

        Ok(unread)
    }

    async fn get_recipient_id_by_username(&self, username: &str) -> Result<i64, MessageError> {
        let row = sqlx::query(
            r#"
            SELECT id
            FROM users
            WHERE username = ?1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = row {
            Ok(row.get("id"))
        } else {
            Err(MessageError::NotFound(format!(
                "Utente con username '{}' non trovato",
                username
            )))
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_in_memory_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Errore creazione DB in-memory");

        sqlx::query(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sender_id INTEGER,
                recipient_id INTEGER,
                kind TEXT NOT NULL,
                content TEXT NOT NULL,
                is_read INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL
            );

            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE
            );

            INSERT INTO users (username) VALUES ('user1'), ('user2'), ('user3');
            "#,
        )
        .execute(&pool)
        .await
        .expect("Errore creazione schema");

        pool
    }

    #[tokio::test]
    async fn test_save_direct_message() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(None, Some(1), "direct", "Ciao", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_save_broadcast_message() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(None, None, "broadcast", "Avviso generale", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_save_client_to_server_message() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(Some(1), None, "client_to_server", "Messaggio client", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_get_unread_messages_returns_only_direct_unread_sorted() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let t1 = Utc::now() - chrono::Duration::seconds(30);
        let t2 = Utc::now() - chrono::Duration::seconds(20);
        let t3 = Utc::now() - chrono::Duration::seconds(10);

        let first_id = service
            .save_message(None, Some(1), "direct", "Primo messaggio", t1)
            .await
            .unwrap();
        let second_id = service
            .save_message(None, Some(1), "direct", "Secondo messaggio", t2)
            .await
            .unwrap();
        service.mark_as_read(second_id, 1).await.unwrap();
        service
            .save_message(None, None, "broadcast", "Broadcast", t3)
            .await
            .unwrap();

        let unread = service.get_unread_messages(1).await.unwrap();

        assert_eq!(unread.len(), 1);
        match &unread[0] {
            WsServerMessage::DirectText { id, text, .. } => {
                assert_eq!(*id, first_id);
                assert_eq!(text, "Primo messaggio");
            }
            _ => panic!("Messaggio non diretto ricevuto come non letto"),
        }
    }

    #[tokio::test]
    async fn test_acknowledge_message_marks_direct_message_as_read() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(None, Some(1), "direct", "Messaggio da confermare", Utc::now())
            .await
            .unwrap();

        service.acknowledge_message(1, id).await.unwrap();

        let unread = service.get_unread_messages(1).await.unwrap();
        assert!(unread.is_empty());
    }

    #[tokio::test]
    async fn test_admin_direct_message_is_saved_for_offline_client() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .send_admin_direct_message("user1", "Messaggio admin offline")
            .await
            .unwrap();

        let unread = service.get_unread_messages(1).await.unwrap();
        assert_eq!(unread.len(), 1);
        match &unread[0] {
            WsServerMessage::DirectText { id: msg_id, text, .. } => {
                assert_eq!(*msg_id, id);
                assert_eq!(text, "Messaggio admin offline");
            }
            _ => panic!("Messaggio ricevuto non è diretto"),
        }
    }

    #[tokio::test]
    async fn test_reject_empty_and_too_long_content() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let empty = service.handle_client_message(1, "   ").await;
        assert!(matches!(empty, Err(MessageError::ValidationError(_))));

        let long = "x".repeat(201);
        let too_long = service.handle_client_message(1, &long).await;
        assert!(matches!(too_long, Err(MessageError::ValidationError(_))));
    }
}