mod state;
mod auth;
mod console;
mod messaging;

use std::sync::Arc;
use common::{WsClientMessage::{self, DirectTextAck, Text}};
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
    let mut direct_rx = state.message_service.add_client(user_id).await;

    // Iscrizione al canale BROADCAST globale
    let mut broadcast_rx = state.message_service.subscribe_broadcast();

    // Invio automatico messaggi pendenti non letti
    if let Ok(unread_msgs) = state.message_service.get_unread_messages(user_id).await {
        for msg in unread_msgs {
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = ws_sender.send(Message::Text(json)).await;
            }
        }
    }

    loop {
        tokio::select! {
            // Handle messaggi in arrivo dal client sulla connessione WebSocket
            incoming = ws_receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break, // client disconnesso
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsClientMessage>(&text) {
                            Ok(Text { text: msg_text }) => {
                                if let Err(err) = state.message_service.handle_client_message(user_id, &msg_text).await {
                                    let err_msg = err.to_client_message();
                                    let _ = ws_sender.send(Message::Text(serde_json::to_string(&err_msg).unwrap())).await;
                                }
                            }
                            Ok(DirectTextAck { id }) => {
                                if let Err(e) = state.message_service.acknowledge_message(user_id, id).await {
                                    tracing::warn!("Errore ACK per user_id {user_id}: {:?}", e);                                }
                            },
                            Err(e) => tracing::error!("Errore nel parsing del messagio: {e}"),
                        }
                    }
                    Some(Ok(_)) => tracing::warn!("Non text message received from user_id {user_id}"),
                    Some(Err(e)) => {
                        tracing::error!("Errore sul socket per user_id {user_id}: {e}");
                        break;
                    }
                }
            }

            // Handle messaggi diretti
            Some(msg) = direct_rx.recv() => {
                if let Ok(msg_json) = serde_json::to_string(&msg) {
                    if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                        tracing::error!("Errore invio messaggio diretto a user_id {user_id}");
                        break;
                    }
                }
            }

            // Handle messaggi broadcast
            Ok(msg) = broadcast_rx.recv() => {
                if let Ok(msg_json) = serde_json::to_string(&msg) {
                    if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                        tracing::error!("Errore invio messaggio broadcast a user_id {user_id}");
                        break;
                    }
                }
            }
        }
    }

    state.message_service.remove_client(user_id).await;
    tracing::info!("Client disconnesso: user_id = {user_id}");
}