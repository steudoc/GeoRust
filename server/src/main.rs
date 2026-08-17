mod state;
mod auth;
mod console;
mod messaging;

use std::sync::Arc;
use common::{WsClientMessage::{self, Handshake, Text}, WsServerMessage};
use futures_util::{SinkExt, StreamExt};
use sqlx::sqlite::SqlitePoolOptions;

use crate::state::AppState;
use crate::console::SimpleConsole;

const DB_PATH: &str = "db.sqlite";

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
    http::StatusCode,
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use tokio::sync::mpsc;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db_url = format!("sqlite://{DB_PATH}?mode=rw"); // mode read/write
    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .connect(&db_url)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Impossibile open {DB_PATH}: {e}\n\
                "
            )
        })?;

    let state = AppState::new(pool);

    let console = SimpleConsole::new(Arc::clone(&state));

    let app = Router::new()
        .route("/register", post(auth::register))
        .route("/login", post( auth::login))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server running at http://0.0.0.0:3000");

    let console_handle = tokio::task::spawn_blocking(move || {
        let handle = tokio::runtime::Handle::current();
        if let Err(e) = handle.block_on(console.run()) {
            tracing::error!("Console error: {e}");
        }
    });

    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        _ = console_handle => {
            tracing::info!("Console terminated");
        }
    }

    tracing::info!("Server shutting down");
    Ok(())
}

// ============================================================================
// WEBSOCKET
// ============================================================================

/// Handler per la connessione WebSocket. 
/// Verifica il token e, se valido, promuove la connessione a WebSocket.
async fn ws_handler(
    State(state): State<Arc<AppState>>,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let token = auth.token();

    let user_id = {
        let tokens = state.tokens.lock().unwrap();
        tokens.get(token).copied()
    };

    let user_id = match user_id {
        Some(id) => id,
        None => return (StatusCode::UNAUTHORIZED, "token non valido").into_response(),
    };

    ws.on_upgrade(move |socket| do_server_side_socket_operations(socket, user_id, state))
        .into_response()
}

/// Gestisce la connessione una volta "promossa" a WebSocket.
/// Effettua il loop di ricezione dei messaggi dal client e l'invio di messaggi diretti e broadcast.
async fn do_server_side_socket_operations(
    socket: WebSocket, 
    user_id: i64,
    state: Arc<AppState>
) {
    tracing::info!("Client connesso: user_id = {user_id}");

    // Split del socket
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Crezione del canale MPSC per i messaggi diretti
    let (direct_tx, mut direct_rx) = mpsc::channel::<WsServerMessage>(100);
    
    // Registrazione del sender nello stato globale
    state.register_client(user_id, direct_tx.clone()).await;

    // Iscrizione al canale BROADCAST globale
    let mut broadcast_rx = state.register_broadcast();
    let mut handshake_completed = false;

    loop {
        tokio::select! {
            // Handle messaggi in arrivo dal client sulla connessione WebSocket
            incoming = ws_receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Client disconnesso: user_id = {user_id}");
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsClientMessage>(&text) {
                            Ok(Handshake { last_received }) => {
                                if handshake_completed {
                                    tracing::warn!("Handshake duplicato da user_id {user_id}");
                                    continue;
                                }

                                let pending = match messaging::handle_client_handshake(&state, user_id, last_received).await {
                                    Ok(pending) => pending,
                                    Err(e) => {
                                        tracing::error!("Errore nel recupero messaggi pendenti per user_id {user_id}: {e}");
                                        break;
                                    }
                                };

                                let mut replay_failed = false;
                                for pending_msg in pending {
                                    let msg_json = match serde_json::to_string(&pending_msg) {
                                        Ok(value) => value,
                                        Err(e) => {
                                            tracing::error!("Errore serializzazione messaggio pendente per user_id {user_id}: {e}");
                                            replay_failed = true;
                                            break;
                                        }
                                    };

                                    if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                                        tracing::warn!("Client {user_id} disconnesso durante l'invio messaggi pendenti");
                                        replay_failed = true;
                                        break;
                                    }
                                }

                                if replay_failed {
                                    break;
                                }

                                handshake_completed = true;
                            },
                            Ok(Text { text }) => {
                                if !handshake_completed {
                                    let err = WsServerMessage::Error {
                                        code: "handshake_required".to_string(),
                                        message: "Handshake non completato. Inviare prima un messaggio di Handshake.".to_string(),
                                    };
                                    match serde_json::to_string(&err) {
                                        Ok(payload) => {
                                            if ws_sender.send(Message::Text(payload)).await.is_err() {
                                                tracing::warn!("Client {user_id} disconnesso durante l'invio errore handshake");
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("Errore serializzazione messaggio errore handshake per user_id {user_id}: {e}");
                                            break;
                                        }
                                    }
                                    continue;
                                }

                                if let Err(e) = messaging::handle_client_to_server_message(&state, user_id, &text).await {
                                    tracing::error!("Errore nel gestire messaggio da user_id {user_id}: {e}");
                                }
                            },
                            Err(e) => {
                                tracing::error!("Errore nel parsing del messagio: {e}");
                            }
                        }
                    }
                    Some(Ok(_)) => {
                        tracing::warn!("Non text message received from user_id {user_id}");
                    }
                    Some(Err(e)) => {
                        tracing::error!("Errore sul socket per user_id {user_id}: {e}");
                        break;
                    }
                }
            }

            // Handle messaggi diretti
            Some(msg) = direct_rx.recv() => {
                let msg_json = match serde_json::to_string(&msg) {
                    Ok(value) => value,
                    Err(e) => {
                        tracing::error!("Errore serializzazione messaggio diretto per user_id {user_id}: {e}");
                        break;
                    }
                };
                if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                    tracing::warn!("Client {user_id} disconnesso durante l'invio messaggio");
                    break;
                }
            }

            // Handle messaggi broadcast
            msg = broadcast_rx.recv() => {
                match msg {
                    Ok(msg) => {
                        let msg_json = match serde_json::to_string(&msg) {
                            Ok(value) => value,
                            Err(e) => {
                                tracing::error!("Errore serializzazione messaggio broadcast per user_id {user_id}: {e}");
                                break;
                            }
                        };
                        if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                            tracing::info!("Client {user_id} disconnesso durante l'invio messaggio");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Errore nel ricevere messaggio broadcast per user_id {user_id}: {e}");
                        break;
                    }
                }
            }
        }
    }

    state.unregister_client(user_id).await;
    println!("Client disconnesso: user_id = {user_id}");
}