mod state;
mod auth;
mod stats;
mod info;
mod admin;

use std::sync::Arc;
use common::{WsClientMessage::{self, Text}, WsServerMessage};
use futures_util::{SinkExt, StreamExt};
use sqlx::sqlite::SqlitePoolOptions;

use crate::state::AppState;

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
use rand::Rng;
use std::time::Duration;
use tokio::time;
use tokio::sync::mpsc;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tracing_appender::rolling;
use tracing_subscriber::fmt::writer::MakeWriterExt;


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

    let state = AppState::new(pool.clone());

    let app = Router::new()
        .route("/register", post(auth::register))
        .route("/login", post( auth::login))
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
async fn do_server_side_socket_operations(
    socket: WebSocket, 
    user_id: i64,
    state: Arc<AppState>
) {
    tracing::info!("Client connesso: user_id = {user_id}");

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Canale MPSC per i messaggi diretti a questo client
    let (direct_tx, mut direct_rx) = mpsc::channel::<WsServerMessage>(100);
    
    state.register_client(user_id, direct_tx.clone()).await;

    // Iscrizione al canale BROADCAST globale
    let mut broadcast_rx = state.register_broadcast();

    let mut interval = time::interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            //TODO: here is the sending mechanism server-side
            // Current implementation: sending a random number every 20 seconds to the client. In the future we will use this channel to send messages to the client based on the state of the server.
            _ = interval.tick() => {
                let numero: u32 = rand::thread_rng().gen_range(0..1000);
                let msg = format!("update:{numero}");

                if ws_sender.send(Message::Text(msg)).await.is_err() {
                    tracing::warn!("Client disconnesso, chiudo il loop");
                    break;
                }
            }

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
                                tracing::info!("Messaggio ricevuto da user_id {}: {} alle {}", user_id, text, timestamp);
                            },
                            Err(e) => {
                                tracing::error!("Errore nel parsing: {}", e);
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

    state.unregister_client(user_id).await;
    tracing::info!("Client disconnesso: user_id = {user_id}");
}
