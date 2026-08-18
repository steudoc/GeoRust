mod admin;
mod auth;
mod info;
mod state;
mod stats;
#[cfg_attr(not(test), allow(dead_code))]
mod trip;

use chrono::Utc;
use common::{
    WsClientMessage::{self, Text},
    WsServerMessage,
};
use futures_util::{SinkExt, StreamExt};
use sqlx::sqlite::SqlitePoolOptions;
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
use tokio::sync::mpsc;
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

    initialize_database(&pool).await?;

    let state = AppState::new(pool.clone());

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
    tokio::spawn(admin::start_admin_console(pool.clone()));

    axum::serve(listener, app).await?;

    Ok(())
}

/*
// Handler che intercetta la richiesta di upgrade a WebSocket
#[derive(Deserialize)]
struct WsQuery {
    token: String,
}*/

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

// Gestisce la connessione una volta "promossa" a WebSocket
async fn do_server_side_socket_operations(socket: WebSocket, user_id: i64, state: Arc<AppState>) {
    tracing::info!("Client connesso: user_id = {user_id}");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Canale MPSC per i messaggi diretti a questo client
    let (direct_tx, mut direct_rx) = mpsc::channel::<WsServerMessage>(100);

    state.register_client(user_id, direct_tx.clone()).await;

    state.start_trip(user_id).await;

    // Iscrizione al canale BROADCAST globale
    let mut broadcast_rx = state.register_broadcast();

    let mut trip_finished = false;
    let mut close_after_response = false;

    loop {
        tokio::select! {
            // intanto ascolta anche eventuali messaggi/chiusura dal client
            incoming = ws_receiver.next() => {
                //TODO: here is the listening mechanism. For now we just print the messages received from the client and the customer name.
                //In the future we will use this channel to receive messages from the client and make actions from the server accordingly.
                match incoming {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Connessione chiusa dal client");
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WsClientMessage>(&text) {
                            Ok(Text { text, timestamp }) => {
                              println!("Messaggio ricevuto da user_id {user_id}: {text} alle {timestamp}");
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
                                    println!("Errore durante l'invio della risposta al client");
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
                                                println!(
                                                    "Tragitto {trip_id} completato per user_id {user_id}: {} punti, {:.2} km, movimento {}s, fermo {}s",
                                                    summary.points_received,
                                                    summary.distance_km,
                                                    summary.moving_seconds,
                                                    summary.stopped_seconds
                                                );
                                                WsServerMessage::TripCompleted {
                                                    numero_coord: summary.points_received,
                                                    tempo_movimento: summary.moving_seconds,
                                                    tempo_fermo: summary.stopped_seconds,
                                                }
                                            }
                                            Err(error) => {
                                                println!(
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
                                    println!("Errore durante l'invio della risposta al client");
                                    break;
                                }

                                if close_after_response {
                                    break;
                                }
                            }
                            Err(e) => {
                                println!("Errore nel parsing del messaggio: {e}");
                                let response = WsServerMessage::Error {
                                    code: "invalid_message".to_string(),
                                    message: format!("Messaggio WebSocket non valido: {e}"),
                                };

                                if !send_server_message(&mut ws_sender, &response).await {
                                    println!("Errore durante l'invio della risposta al client");
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(_)) => {
                        tracing::debug!("Non text message received");
                    }
                    Some(Err(e)) => {
                        tracing::error!("Errore sul socket: {e}");
                        break;
                    }
                }
            }

            // Handle messaggi diretti
            Some(msg) = direct_rx.recv() => {
                let msg_json = serde_json::to_string(&msg).unwrap();
                if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                    println!("Client disconnesso durante invio direct, chiudo il loop");
                    break;
                }
            }

            // Handle messaggi broadcast
            msg = broadcast_rx.recv() => {
                match msg {
                    Ok(msg) => {
                        let msg_json = serde_json::to_string(&msg).unwrap();
                        if ws_sender.send(Message::Text(msg_json)).await.is_err() {
                            tracing::warn!("Client disconnesso durante invio direct/broadcast, chiudo il loop");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Errore nel ricevere broadcast: {e}");
                        break;
                    }
                }
            }
        }
    }

    if !trip_finished {
        if let Some(summary) = state.finish_trip(user_id).await {
            println!(
                "Riepilogo user_id {user_id}: {} punti, {}s in movimento, {}s fermo",
                summary.points_received, summary.moving_seconds, summary.stopped_seconds
            );
        }
    }

    state.unregister_client(user_id).await;
    println!("Client disconnesso: user_id = {user_id}");
}

async fn send_server_message<S>(sender: &mut S, message: &WsServerMessage) -> bool
where
    S: futures_util::Sink<Message> + Unpin,
{
    let json = match serde_json::to_string(message) {
        Ok(json) => json,
        Err(error) => {
            println!("Errore durante la serializzazione della risposta: {error}");
            return false;
        }
    };

    sender.send(Message::Text(json)).await.is_ok()
}
