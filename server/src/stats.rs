use chrono::{Datelike, Days, Utc};
use common::MovementStats;
use sqlx::{Row, SqlitePool};

pub async fn calculate_user_stats(
    pool: &SqlitePool,
    user_id: i64,
    period: &str,
) -> Result<MovementStats, sqlx::Error> {
    let now = Utc::now().date_naive();

    // determinazione della data di inizio per SQLite
    let start_date = match period {
        "day" => now.to_string(),
        "week" => {
            let days_from_monday = now.weekday().num_days_from_monday() as u64;
            (now - Days::new(days_from_monday)).to_string()
        }
        "month" => now.with_day(1).unwrap_or(now).to_string(),
        _ => now.to_string(),
    };

    // query aggregata
    let row = sqlx::query(
        r#"SELECT
            COALESCE(SUM(CAST(distance_km AS REAL)), 0.0) AS tot_dist,
            COALESCE(SUM(CAST(moving_seconds + stopped_seconds AS REAL)), 0.0) AS tot_time,
            COALESCE(SUM(CAST(stopped_seconds AS REAL)), 0.0) AS tot_pause,
            COALESCE(SUM(CAST(moving_seconds AS REAL)), 0.0) AS moving_time
        FROM trips
        WHERE user_id = ?
            AND date(trip_date) >= date(?)"#,
    )
    .bind(user_id)
    .bind(start_date)
    .fetch_one(pool)
    .await?;

    // estrazione dati
    let distance: f64 = row.try_get("tot_dist")?;
    let total_time: f64 = row.try_get("tot_time")?;
    let total_pause: f64 = row.try_get("tot_pause")?;
    let moving_time: f64 = row.try_get("moving_time")?;

    // La velocità media considera soltanto il tempo effettivamente in movimento.
    let avg_velocity = if moving_time > 0.0 {
        (distance / moving_time) * 3600.0
    } else {
        0.0
    };

    Ok(MovementStats {
        period: period.to_string(),
        distance,
        total_time,
        total_pause,
        avg_velocity,
    })
}

// ------------------------------
// ---------- TEST
// ------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use sqlx::sqlite::SqlitePoolOptions;

    // Funzione helper per creare un DB in memoria pulito ad ogni test
    async fn setup_memory_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Impossibile creare il DB in memoria");

        sqlx::query(
            r#"
            CREATE TABLE trips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                trip_date TEXT NOT NULL,
                distance_km REAL NOT NULL,
                moving_seconds INTEGER NOT NULL,
                stopped_seconds INTEGER NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Impossibile creare la tabella trips");

        pool
    }

    #[tokio::test]
    async fn test_calculate_user_stats_day() {
        let pool = setup_memory_db().await;
        let today_str = Utc::now().date_naive().to_string();

        // viaggio valido per oggi
        sqlx::query(
            "INSERT INTO trips (user_id, trip_date, distance_km, moving_seconds, stopped_seconds)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(1)
        .bind(&today_str)
        .bind(15.0) // distanza: 15
        .bind(1800) // tempo in movimento: 1800s
        .bind(1800) // tempo fermo: 1800s (totale = 3600s)
        .execute(&pool)
        .await
        .unwrap();

        let stats = calculate_user_stats(&pool, 1, "day").await.unwrap();

        assert_eq!(stats.period, "day");
        assert_eq!(stats.distance, 15.0);
        assert_eq!(stats.total_time, 3600.0);
        assert_eq!(stats.total_pause, 1800.0);

        // (15.0 / 1800.0) * 3600 = 30.0
        assert_eq!(stats.avg_velocity, 30.0);
    }

    #[tokio::test]
    async fn test_calculate_user_stats_ignores_other_users() {
        let pool = setup_memory_db().await;
        let today_str = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // viaggio per user2
        sqlx::query(
            "INSERT INTO trips (user_id, trip_date, distance_km, moving_seconds, stopped_seconds)
             VALUES (2, ?, 100.0, 3600.0, 0.0)",
        )
        .bind(&today_str)
        .execute(&pool)
        .await
        .unwrap();

        // richiediamo le stats per l'utente 1
        let stats = calculate_user_stats(&pool, 1, "day").await.unwrap();

        assert_eq!(stats.distance, 0.0);
        assert_eq!(stats.total_time, 0.0);
        assert_eq!(stats.avg_velocity, 0.0);
    }

    #[tokio::test]
    async fn test_calculate_user_stats_filters_by_date() {
        let pool = setup_memory_db().await;
        let now = Utc::now().date_naive();

        let today_str = now.to_string();
        // sottraiamo 40 giorni, in modo da essere sicuramente fuori dal mese corrente
        let past_str = (now - Duration::days(40)).to_string();

        // viaggio 1: oggi
        sqlx::query(
            "INSERT INTO trips (user_id, trip_date, distance_km, moving_seconds, stopped_seconds)
             VALUES (1, ?, 10.0, 1000.0, 100.0)",
        )
        .bind(&today_str)
        .execute(&pool)
        .await
        .unwrap();

        // viaggio 2: 40 giorni fa
        sqlx::query(
            "INSERT INTO trips (user_id, trip_date, distance_km, moving_seconds, stopped_seconds)
             VALUES (1, ?, 50.0, 1000.0, 100.0)",
        )
        .bind(&past_str)
        .execute(&pool)
        .await
        .unwrap();

        let stats = calculate_user_stats(&pool, 1, "month").await.unwrap();
        assert_eq!(stats.distance, 10.0);
    }
}
