mod admin;
mod auth;
mod info;
mod messaging;
mod state;
mod stats;
#[cfg_attr(not(test), allow(dead_code))]
mod trip;

use chrono::Utc;
use common::{
    WsClientMessage::{self, DirectTextAck, Text},
    WsServerMessage,
};

use futures_util::{SinkExt, StreamExt};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;
use std::sync::Arc;

use crate::state::{AppState, initialize_database};

const DB_PATH: &str = "db.sqlite";

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use axum_extra::TypedHeader;
use headers::{Authorization, authorization::Bearer};
use tokio::sync::broadcast::error::RecvError;
use tracing_appender::rolling;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // crea un file di log giornaliero per gli eventi del server
    let file_appender = rolling::daily("./logs", "websocket.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    let db_url = format!("sqlite://{DB_PATH}?mode=rw"); // mode read/write

    // Configura le opzioni di connessione al database SQLite
    let connection_options = SqliteConnectOptions::from_str(&db_url)?.foreign_keys(true);

    // Crea un pool di connessioni al database SQLite
    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .connect_with(connection_options)
        .await
        .map_err(|e| anyhow::anyhow!("Impossibile aprire {db_url}: {e}"))?;

    // Crea le tabelle del database se non esistono già
    initialize_database(&pool).await?;

    let state = AppState::new(pool.clone());

    let admin_state = Arc::clone(&state);

    let app = Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server running at http://0.0.0.0:3000");

    // Lancia il logger della CPU in background
    tokio::spawn(info::start_cpu_logger());

    // setup della CLI amministratore
    tokio::spawn(admin::start_admin_console(admin_state));

    axum::serve(listener, app).await?;

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
async fn do_server_side_socket_operations(socket: WebSocket, user_id: i64, state: Arc<AppState>) {
    tracing::info!("Client connesso: user_id = {user_id}");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Crezione del canale MPSC per i messaggi diretti
    let mut direct_rx = state.message_service.add_client(user_id).await;

    state.start_trip(user_id).await;

    // Iscrizione al canale BROADCAST globale
    let mut broadcast_rx = state.message_service.subscribe_broadcast();

    let mut trip_finished = false;
    let mut close_after_response = false;

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
                                if let Err(err) = state.message_service.handle_client_message(user_id, &msg_text).await
                                    && !send_server_message(&mut ws_sender, &err.to_client_message()).await {
                                        tracing::error!("Errore durante l'invio della risposta al client");
                                        break;
                                    }
                            },
                            Ok(DirectTextAck { id }) => {
                                if let Err(e) = state.message_service.acknowledge_message(user_id, id).await {
                                    tracing::error!("Errore ACK per user_id {user_id}: {:?}", e);                                }
                            },
                            Ok(WsClientMessage::PositionUpdate { coordinata, elapsed_seconds }) => {
                                let response = match state.record_position(user_id, coordinata, elapsed_seconds).await {
                                    Some(Ok((user_state, points_received))) => {
                                        WsServerMessage::PositionAccepted {
                                            stato: user_state,
                                            coord_ricevute: points_received,
                                        }
                                    }
                                    Some(Err(error)) => WsServerMessage::Error {
                                        code: "invalid_position_time".to_string(),
                                        message: error.to_string(),
                                    },
                                    None => WsServerMessage::Error {
                                        code: "trip_not_found".to_string(),
                                        message: "Nessun tragitto attivo per l'utente".to_string(),
                                    },
                                };

                                if !send_server_message(&mut ws_sender, &response).await {
                                    tracing::error!("Errore durante l'invio della risposta al client");
                                    break;
                                }
                            }
                            Ok(WsClientMessage::TripCompleted) => {
                                let response = match state.finish_trip(user_id).await {
                                    Some(summary) => {
                                        close_after_response = true;
                                        let trip_date = Utc::now().date_naive();

                                        match state.save_trip(user_id, trip_date, summary).await {
                                            Ok(trip_id) => {
                                                trip_finished = true;
                                                let msg = format!(
                                                    "Tragitto {trip_id} completato per user_id {user_id}: {} punti, {:.2} km, movimento {}s, fermo {}s",
                                                    summary.points_received,
                                                    summary.distance_km,
                                                    summary.moving_seconds,
                                                    summary.stopped_seconds
                                                );
                                                //println!("{}", msg);
                                                tracing::info!("{}", msg);
                                                WsServerMessage::TripCompleted {
                                                    numero_coord: summary.points_received,
                                                    tempo_movimento: summary.moving_seconds,
                                                    tempo_fermo: summary.stopped_seconds,
                                                }
                                            }
                                            Err(error) => {
                                                tracing::error!(
                                                    "Impossibile salvare il tragitto di user_id {user_id}: {error}"
                                                );
                                                WsServerMessage::Error {
                                                    code: "trip_save_failed".to_string(),
                                                    message: "Impossibile salvare il tragitto".to_string(),
                                                }
                                            }
                                        }
                                    }
                                    None => WsServerMessage::Error {
                                        code: "trip_not_found".to_string(),
                                        message: "Nessun tragitto attivo per l'utente".to_string(),
                                    },
                                };

                                if !send_server_message(&mut ws_sender, &response).await {
                                    tracing::error!("Errore durante l'invio della risposta al client");
                                    break;
                                }

                                if close_after_response {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Errore nel parsing del messaggio: {e}");
                                let response = WsServerMessage::Error {
                                    code: "invalid_message".to_string(),
                                    message: format!("Messaggio WebSocket non valido: {e}"),
                                };

                                if !send_server_message(&mut ws_sender, &response).await {
                                    tracing::error!("Errore durante l'invio della risposta al client");
                                    break;
                                }
                            },
                        }
                    },
                    Some(Ok(Message::Ping(_))) => {},
                    Some(Ok(_)) => tracing::warn!("Messaggio non testuale ricevuto da user_id {user_id}"),
                    Some(Err(e)) => {
                        tracing::error!("Errore sul socket per user_id {user_id}: {e}");
                        break;
                    }
                }
            }

            // Handle messaggi diretti
            msg = direct_rx.recv() => {
                match msg {
                    Some(msg) => {
                        if !send_server_message(&mut ws_sender, &msg).await {
                            tracing::error!("Errore durante invio direct a user_id {user_id}");
                            break;
                        }
                    },
                    None => {
                        tracing::info!("Canale direct chiuso per user_id {user_id}");
                        break;
                    }
                }
            }

            // Handle messaggi broadcast
            msg = broadcast_rx.recv() => {
                match msg {
                    Ok(msg) => {
                        if !send_server_message(&mut ws_sender, &msg).await {
                            tracing::error!("Errore durante invio broadcast a user_id {user_id}");
                            break;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!("Client {user_id} in ritardo, persi {skipped} messaggi broadcast");
                    }
                    Err(RecvError::Closed) => {
                        tracing::info!("Canale broadcast chiuso per user_id {user_id}");
                        break;
                    }
                }
            }
        }
    }

    if !trip_finished {
        if let Some(summary) = state.finish_trip(user_id).await {
            let msg = format!(
                "Riepilogo user_id {user_id}: {} punti, {}s in movimento, {}s fermo",
                summary.points_received, 
                summary.moving_seconds, 
                summary.stopped_seconds
            );
            //println!("{}", msg);
            tracing::info!("{}", msg);
        }
    }

    state.message_service.remove_client(user_id).await;
    tracing::info!("Client disconnesso: user_id = {user_id}");
}

async fn send_server_message<S>(sender: &mut S, message: &WsServerMessage) -> bool
where
    S: futures_util::Sink<Message> + Unpin,
{
    let json = match serde_json::to_string(message) {
        Ok(json) => json,
        Err(error) => {
            tracing::error!("Errore durante la serializzazione della risposta: {error}");
            return false;
        }
    };

    sender.send(Message::Text(json)).await.is_ok()
}
