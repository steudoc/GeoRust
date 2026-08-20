use crate::trip::{Trip, TripError};
use anyhow::Context;
use chrono::NaiveDate;
use common::{UserState, tracking::Coordinata};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::messaging::MessageService;

type UserId = i64;

// Crea le strutture necessarie se il database non è ancora inizializzato
pub async fn initialize_database(db: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS trips (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            trip_date TEXT NOT NULL,
            distance_km REAL NOT NULL CHECK (distance_km >= 0),
            moving_seconds INTEGER NOT NULL CHECK (moving_seconds >= 0),
            stopped_seconds INTEGER NOT NULL CHECK (stopped_seconds >= 0),
            FOREIGN KEY (user_id) REFERENCES users(id)
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_trips_user_id ON trips(user_id)")
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS trips_points (
            trip_id INTEGER NOT NULL,
            point_index INTEGER NOT NULL CHECK (point_index >= 0),
            elapsed_seconds INTEGER NOT NULL CHECK (elapsed_seconds >= 0),
            latitude REAL NOT NULL CHECK (latitude >= -90 AND latitude <= 90),
            longitude REAL NOT NULL CHECK (longitude >= -180 AND longitude <= 180),
            PRIMARY KEY (trip_id, point_index),
            FOREIGN KEY (trip_id) REFERENCES trips(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sender_id INTEGER, -- NULL per i messaggi inviati dal server
            recipient_id INTEGER, -- NULL per i messaggi broadcast
            kind TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            is_read INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE,
            FOREIGN KEY (recipient_id) REFERENCES users(id) ON DELETE CASCADE
        );
        "#
    )
    .execute(db)
    .await?;

    Ok(())
}

/*
* Stato condiviso tra tutti gli handler axum.
* Arc<AppState> viene clonato e passato ad ogni handler tramite axum::extract::State
*/
pub struct AppState {
    pub db: SqlitePool,
    pub tokens: Mutex<HashMap<String, UserId>>, // token -> user_id
    pub message_service: MessageService,        // servizio di persistenza e gestione dei messaggi
    trips: RwLock<HashMap<UserId, Trip>>,       // associa ogni utente al suo tragitto (trip)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TripSummary {
    // Riepilogo del tragitto
    pub points_received: usize,
    pub moving_seconds: u64,
    pub stopped_seconds: u64,
    pub distance_km: f64,
}

impl AppState {
    pub fn new(db: SqlitePool) -> Arc<Self> {
        Arc::new(Self {
            db: db.clone(),
            tokens: Mutex::new(HashMap::new()),
            message_service: MessageService::new(db.clone()),
            trips: RwLock::new(HashMap::new()),
        })
    }

    pub async fn start_trip(&self, user_id: UserId) {
        let mut trips = self.trips.write().await;
        trips.insert(user_id, Trip::new(user_id));
    }

    pub async fn record_position(
        &self,
        user_id: UserId,
        coordinates: Coordinata,
        elapsed_seconds: u64,
    ) -> Option<Result<(UserState, usize), TripError>> {
        let mut trips = self.trips.write().await;
        let trip = trips.get_mut(&user_id)?;

        Some(
            trip.record_position(coordinates, elapsed_seconds) // Aggiornamento del valore trip
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
            distance_km: trip.get_distance_km(),
        })
    }

    /// Salva un tragitto concluso e tutte le sue coordinate
    pub async fn save_trip(
        &self,
        user_id: UserId,
        trip_date: NaiveDate,
        summary: TripSummary,
    ) -> anyhow::Result<i64> {
        let positions = {
            let trips = self.trips.read().await;
            trips
                .get(&user_id)
                .context("Nessun tragitto in memoria per l'utente")?
                .get_positions()
                .to_vec()
        };

        if positions.len() != summary.points_received {
            anyhow::bail!("Il riepilogo non corrisponde alle coordinate del tragitto");
        }

        let moving_seconds = i64::try_from(summary.moving_seconds)
            .context("Il tempo di movimento supera il limite di SQLite")?;
        let stopped_seconds = i64::try_from(summary.stopped_seconds)
            .context("Il tempo di pausa supera il limite di SQLite")?;

        let mut transaction = self.db.begin().await?;

        let result = sqlx::query(
            r#"
            INSERT INTO trips (
                user_id,
                trip_date,
                distance_km,
                moving_seconds,
                stopped_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(user_id)
        .bind(trip_date)
        .bind(summary.distance_km)
        .bind(moving_seconds)
        .bind(stopped_seconds)
        .execute(&mut *transaction)
        .await?;

        let trip_id = result.last_insert_rowid();

        for (point_index, position) in positions.iter().enumerate() {
            let point_index = i64::try_from(point_index)
                .context("Il numero di coordinate supera il limite di SQLite")?;
            let elapsed_seconds = i64::try_from(position.elapsed_seconds)
                .context("Il tempo logico supera il limite di SQLite")?;

            sqlx::query(
                r#"
                INSERT INTO trips_points (
                    trip_id,
                    point_index,
                    elapsed_seconds,
                    latitude,
                    longitude
                )
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
            )
            .bind(trip_id)
            .bind(point_index)
            .bind(elapsed_seconds)
            .bind(position.coordinates.get_latitudine())
            .bind(position.coordinates.get_longitudine())
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;

        Ok(trip_id)
    }
    
    pub async fn get_connected_users(&self) -> Vec<i64> {
        let trips_guard = self.trips.read().await;
        trips_guard.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common::tracking::Coordinata;
    use sqlx::Row;
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
        let elapsed_seconds = 0;

        let (user_state, points_received) = state
            .record_position(42, coordinates, elapsed_seconds)
            .await
            .expect("tragitto presente")
            .expect("posizione valida");

        assert_eq!(user_state, UserState::Still);
        assert_eq!(points_received, 1);

        let summary = state.finish_trip(42).await.unwrap();
        assert_eq!(summary.points_received, 1);
    }

    #[tokio::test]
    async fn saves_completed_trips_with_incrementing_ids() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO users (id, username) VALUES (42, 'test-user')")
            .execute(&pool)
            .await
            .unwrap();
        initialize_database(&pool).await.unwrap();

        let state = AppState::new(pool);
        state.start_trip(42).await;
        state
            .record_position(42, Coordinata::new(0.0, 0.0).unwrap(), 0)
            .await
            .unwrap()
            .unwrap();
        state
            .record_position(42, Coordinata::new(0.0, 1.0).unwrap(), 30)
            .await
            .unwrap()
            .unwrap();

        let summary = state.finish_trip(42).await.unwrap();
        let trip_date = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
        let first_id = state.save_trip(42, trip_date, summary).await.unwrap();
        let second_id = state.save_trip(42, trip_date, summary).await.unwrap();

        assert_eq!(first_id, 1);
        assert_eq!(second_id, 2);

        let row = sqlx::query(
            r#"
            SELECT user_id, trip_date, distance_km, moving_seconds, stopped_seconds
            FROM trips
            WHERE id = ?1
            "#,
        )
        .bind(first_id)
        .fetch_one(&state.db)
        .await
        .unwrap();

        assert_eq!(row.get::<i64, _>("user_id"), 42);
        assert_eq!(row.get::<NaiveDate, _>("trip_date"), trip_date);
        assert_eq!(row.get::<f64, _>("distance_km"), 111.19);
        assert_eq!(row.get::<i64, _>("moving_seconds"), 30);
        assert_eq!(row.get::<i64, _>("stopped_seconds"), 0);

        let points = sqlx::query(
            r#"
            SELECT point_index, elapsed_seconds, latitude, longitude
            FROM trips_points
            WHERE trip_id = ?1
            ORDER BY point_index
            "#,
        )
        .bind(first_id)
        .fetch_all(&state.db)
        .await
        .unwrap();

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].get::<i64, _>("point_index"), 0);
        assert_eq!(points[0].get::<i64, _>("elapsed_seconds"), 0);
        assert_eq!(points[0].get::<f64, _>("latitude"), 0.0);
        assert_eq!(points[0].get::<f64, _>("longitude"), 0.0);
        assert_eq!(points[1].get::<i64, _>("point_index"), 1);
        assert_eq!(points[1].get::<i64, _>("elapsed_seconds"), 30);
        assert_eq!(points[1].get::<f64, _>("latitude"), 0.0);
        assert_eq!(points[1].get::<f64, _>("longitude"), 1.0);

        let sequence: i64 =
            sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name = 'trips'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(sequence, 2);
    }
}
