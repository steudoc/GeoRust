mod state;
mod auth;

use std::sync::Arc;
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

    let app = Router::new()
        .route("/register", post(auth::register))
        .route("/login", post( auth::login))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server running at http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}




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

    ws.on_upgrade(move |socket| do_server_side_socket_operations(socket, user_id))
        .into_response()
}


// Gestisce la connessione una volta "promossa" a WebSocket
async fn do_server_side_socket_operations(mut socket: WebSocket, user_id: i64) {
    println!("Client connesso: user_id = {user_id}");

    let mut interval = time::interval(Duration::from_secs(20));

    loop {
        tokio::select! {
            //TODO: here is the sending mechanism server-side
            // Current implementation: sending a random number every 20 seconds to the client. In the future we will use this channel to send messages to the client based on the state of the server.
            _ = interval.tick() => {
                let numero: u32 = rand::thread_rng().gen_range(0..1000);
                let msg = format!("update:{numero}");

                if socket.send(Message::Text(msg)).await.is_err() {
                    println!("Client disconnesso, chiudo il loop");
                    break;
                }
            }

            // intanto ascolta anche eventuali messaggi/chiusura dal client
            incoming = socket.recv() => {
                //TODO: here is the listening mechanism. For now we just print the messages received from the client and the customer name. 
                //In the future we will use this channel to receive messages from the client and make actions from the server accordingly.
                match incoming {
                    Some(Ok(Message::Close(_))) | None => {
                        println!("Connessione chiusa dal client");
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        println!("Ricevuto da user_id {user_id}: {text}");
                    }

                    Some(Ok(_)) => {
                        println!("Non text message received");
                    }
                    Some(Err(e)) => {
                        println!("Errore sul socket: {e}");
                        break;
                    }
                }
            }
        }
    }
}