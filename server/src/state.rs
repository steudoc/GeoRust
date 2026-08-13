use chrono::{DateTime, Utc}; // Per calcolare il timestamp in cui il server ha acquisito una coordinata
use common::{UserState, WsServerMessage, tracking::Coordinata};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sqlx::SqlitePool;
use tokio::sync::{mpsc, broadcast, RwLock};
use crate::trip::{Trip, TripError};

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
    trips: RwLock<HashMap<UserId, Trip>>,   // associa ogni utente al suo tragitto (trip)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripSummary {
    // Riepilogo del tragitto
    pub points_received: usize,
    pub moving_seconds: u64,
    pub stopped_seconds: u64,
}

impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        let (broadcast_tx, _) = broadcast::channel(100); // buffer size 100
        Arc::new(Self {
            db,
            tokens: Mutex::new(HashMap::new()),
            clients: RwLock::new(HashMap::new()),
            trips: RwLock::new(HashMap::new()),
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

    pub async fn start_trip(&self, user_id: UserId) {
        let mut trips = self.trips.write().await;
        trips.insert(user_id, Trip::new(user_id));
    }

    pub async fn record_position(
        &self,
        user_id: UserId,
        coordinates: Coordinata,
        received_at: DateTime<Utc>,
    ) -> Option<Result<(UserState, usize), TripError>> {
        let mut trips = self.trips.write().await;
        let trip = trips.get_mut(&user_id)?;

        Some(
            trip.record_position(coordinates, received_at)// Aggiornamento del valore trip
                .map(|state| (state, trip.get_positions().len())),
        )
    }

    /// Termina il tragitto, ma lo conserva in memoria in attesa del database
    pub async fn finish_trip(&self, user_id: UserId) -> Option<TripSummary> {
        let mut trips = self.trips.write().await;
        let trip = trips.get_mut(&user_id)?;

        trip.disconnect();

        Some(TripSummary {
            points_received: trip.get_positions().len(),
            moving_seconds: trip.get_moving_seconds(),
            stopped_seconds: trip.get_stopped_seconds(),
        })
    }
}



// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use common::tracking::Coordinata;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    fn test_state() -> Arc<AppState> {
        let pool = SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .expect("pool SQLite in memoria valido");
        AppState::new(pool)
    }

    #[tokio::test]
    async fn associates_received_positions_with_the_authenticated_user() {
        let state = test_state();
        state.start_trip(42).await;
        let coordinates = Coordinata::new(45.0, 7.0).unwrap();
        let received_at = Utc.timestamp_opt(100, 0).single().unwrap();

        let (user_state, points_received) = state
            .record_position(42, coordinates, received_at)
            .await
            .expect("tragitto presente")
            .expect("posizione valida");

        assert_eq!(user_state, UserState::Still);
        assert_eq!(points_received, 1);

        let summary = state.finish_trip(42).await.unwrap();
        assert_eq!(summary.points_received, 1);
    }
}
