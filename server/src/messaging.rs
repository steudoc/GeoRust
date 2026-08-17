use chrono::{DateTime, Utc};
use common::WsServerMessage;
//use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::state::AppState;

// ============================================================================
// 1. DATA MODEL (DbMessage)
// ============================================================================

/*#[derive(Debug, sqlx::FromRow, Serialize, Deserialize)]
pub struct DbMessage {
    pub id: i64,
    pub sender_id: Option<i64>,    // None / 0 = Server/Admin
    pub recipient_id: Option<i64>, // None = Broadcast
    pub kind: String,              // "direct", "broadcast", "client_to_server"
    pub content: String,
    pub created_at_ms: i64,
}*/

// ============================================================================
// 2. DOMAIN SERVICE & PERSISTENCE (MessageService)
// ============================================================================

#[derive(Clone)]
pub struct MessageService {
    db: SqlitePool,
}

impl MessageService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    // Persiste qualsiasi tipologia di messaggio nel DB e ritorna l'ID del messaggio appena creato
    pub async fn save_message(
        &self,
        sender_id: Option<i64>,
        recipient_id: Option<i64>,
        kind: &str,
        content: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<i64, String> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return Err("Il testo del messaggio non può essere vuoto".to_string());
        }

        let created_at_ms = timestamp.timestamp_millis();

        match kind {
            "direct" | "broadcast" | "client_to_server" => {},
            _ => return Err(format!("Tipo di messaggio non valido: {}", kind)),
        }

        let result = sqlx::query(
            r#"
            INSERT INTO messages (sender_id, recipient_id, kind, content, created_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(sender_id)
        .bind(recipient_id)
        .bind(kind)
        .bind(trimmed)
        .bind(created_at_ms)
        .execute(&self.db)
        .await
        .map_err(|e| format!("Errore di salvataggio su DB: {e}"))?;

        Ok(result.last_insert_rowid())
    }

    // Recupera tutti i messaggi diretti o broadcast creati dopo since, per il client specificato
    pub async fn get_pending_messages_since(
        &self,
        client_id: i64,
        since: DateTime<Utc>,
    ) -> Result<Vec<WsServerMessage>, String> {
        let since_ms = since.timestamp_millis();

        let rows = sqlx::query(
            r#"
            SELECT id, kind, content, created_at_ms
            FROM messages
            WHERE created_at_ms > ?1
              AND (
                (kind = 'direct' AND recipient_id = ?2)
                OR kind = 'broadcast'
              )
            ORDER BY created_at_ms ASC
            "#,
        )
        .bind(since_ms)
        .bind(client_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| format!("Errore nel recupero dei messaggi pendenti: {e}"))?;

        let pending_messages = rows
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let kind: String = r.get("kind");
                let content: String = r.get("content");
                let created_at_ms: i64 = r.get("created_at_ms");

                let timestamp = DateTime::from_timestamp_millis(created_at_ms)
                    .unwrap_or_else(Utc::now);

                if kind == "broadcast" {
                    WsServerMessage::BroadcastText {
                        id,
                        text: content,
                        timestamp,
                    }
                } else {
                    WsServerMessage::DirectText {
                        id,
                        text: content,
                        timestamp,
                    }
                }
            })
            .collect();

        Ok(pending_messages)
    }
}

// ============================================================================
// 3. ADMIN FUNCTIONALITIES & WS HANDLERS
// ============================================================================

// Invio messaggi Server -> Singolo Client (da console admin)
pub async fn send_admin_direct_message(
    state: &AppState,
    recipient_id: i64,
    content: &str,
) -> Result<i64, String> {
    let now = Utc::now();

    let msg_id = state
        .message_service
        .save_message(Some(0), Some(recipient_id), "direct", content, now)
        .await?;

    let ws_msg = WsServerMessage::DirectText {
        id: msg_id,
        text: content.trim().to_string(),
        timestamp: now,
    };

    let clients = state.clients.read().await;
    if let Some(tx) = clients.get(&recipient_id) {
        let _ = tx.send(ws_msg).await;
    } else {
        println!(
            "[Admin] Client {} disconnesso. Messaggio #{} salvato in DB per il delivery successivo.",
            recipient_id, msg_id
        );
    }

    Ok(msg_id)
}

// Invio messaggi Server -> Broadcast (da console admin)
pub async fn send_admin_broadcast_message(
    state: &AppState,
    content: &str,
) -> Result<i64, String> {
    let now = Utc::now();

    let msg_id = state
        .message_service
        .save_message(Some(0), None, "broadcast", content, now)
        .await?;

    let ws_msg = WsServerMessage::BroadcastText {
        id: msg_id,
        text: content.trim().to_string(),
        timestamp: now,
    };

    let _ = state.broadcast_tx.send(ws_msg);

    Ok(msg_id)
}

// Ricezione messaggi Client -> Server (e stampa in console admin)
pub async fn handle_client_to_server_message(
    state: &AppState,
    sender_id: i64,
    content: &str,
) -> Result<i64, String> {
    let now = Utc::now();

    let msg_id = state
        .message_service
        .save_message(Some(sender_id), Some(0), "client_to_server", content, now)
        .await?;

    println!(
        "\n========================================\n\
         [MESSAGGIO RICEVUTO DA CLIENT {}]\n\
         ID Messaggio: {}\n\
         Data/Ora: {}\n\
         Contenuto: {}\n\
         ========================================",
        sender_id,
        msg_id,
        now.format("%Y-%m-%d %H:%M:%S"),
        content.trim()
    );

    Ok(msg_id)
}

// Gestione riconnessione client (Handshake) e invio messaggi pendenti
pub async fn handle_client_handshake(
    state: &AppState,
    client_id: i64,
    last_seen: DateTime<Utc>,
) -> Result<Vec<WsServerMessage>, String> {
    let pending = state
        .message_service
        .get_pending_messages_since(client_id, last_seen)
        .await?;

    println!(
        "[Sync] Handshake client {}: inviati {} messaggi accumulati dal {}",
        client_id,
        pending.len(),
        last_seen.format("%Y-%m-%d %H:%M:%S")
    );

    Ok(pending)
}

// ============================================================================
// 4. UNIT TEST
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
                created_at_ms INTEGER NOT NULL
            );
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
            .save_message(Some(0), Some(42), "direct", "Ciao Client 42", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_save_broadcast_message() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(Some(0), None, "broadcast", "Avviso generale", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_save_client_to_server_message() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let id = service
            .save_message(Some(42), Some(0), "client_to_server", "Messaggio client", Utc::now())
            .await
            .unwrap();

        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_handshake_retrieval() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let t1 = Utc::now();

        let _ = service
            .save_message(Some(0), Some(42), "direct", "Messaggio diretto offline", t1)
            .await;
        let _ = service
            .save_message(Some(0), None, "broadcast", "Broadcast offline", t1)
            .await;

        let since = t1 - chrono::Duration::seconds(5);
        let pending = service.get_pending_messages_since(42, since).await.unwrap();

        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_reject_empty_content() {
        let db = setup_in_memory_db().await;
        let service = MessageService::new(db);

        let res = service
            .save_message(Some(0), Some(42), "direct", "    ", Utc::now())
            .await;

        assert!(res.is_err());
    }
}