mod state;
mod auth;

use std::sync::Arc;
use axum::routing::{get, post};
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;

use crate::state::AppState;

const DB_PATH: &str = "db.sqlite";

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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server running at http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
