use common::MovementStats;
use sqlx::{Row, SqlitePool};
use chrono::{Utc, Datelike, Days};

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
        },
        "month" => now.with_day(1).unwrap_or(now).to_string(),
        _ => now.to_string(),
    };

    // query aggregata
    let row = sqlx::query(
        r#"SELECT
            SUM(distance) as tot_dist,
            SUM(total_time) as tot_time,
            SUM(pause_time) as tot_pause
        FROM trips
        WHERE user_id = ? 
            AND date(date_trip) >= ?"#
    )
    .bind(user_id)
    .bind(start_date)
    .fetch_one(pool)
    .await?;

    // estrazione dati
    let distance: f64 = row.try_get("tot_dist")?;
    let total_time: f64 = row.try_get("tot_time")?;
    let total_pause: f64 = row.try_get("tot_pause")?;

    // calcolo velocità e durata movimento
    let moving_time = total_time - total_pause;
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
        avg_velocity 
    })
}


// ------------------------------
// ---------- TEST
// ------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use chrono::{Duration, Utc};

    // Funzione helper per creare un DB in memoria pulito ad ogni test
    async fn setup_memory_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("Impossibile creare il DB in memoria");

        sqlx::query(
            r#"
            CREATE TABLE trips (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                date_trip DATETIME NOT NULL,
                distance REAL NOT NULL,
                total_time REAL NOT NULL,
                pause_time REAL NOT NULL
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
        let now = Utc::now();
        
        // formattiamo la data di oggi (es: "2026-08-08 12:00:00")
        let today_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

        // viaggio valido per oggi
        sqlx::query(
            "INSERT INTO trips (user_id, date_trip, distance, total_time, pause_time) 
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(1)
        .bind(&today_str)
        .bind(15.0)   // distanza: 15
        .bind(3600.0) // tempo totale: 3600s
        .bind(1800.0) // tempo fermo: 1800s (moving = 1800s)
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
            "INSERT INTO trips (user_id, date_trip, distance, total_time, pause_time) 
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
        let now = Utc::now();
        
        let today_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
        // sottraiamo 40 giorni, in modo da essere sicuramente fuori dal mese corrente
        let past_str = (now - Duration::days(40)).format("%Y-%m-%d %H:%M:%S").to_string();

        // viaggio 1: oggi
        sqlx::query(
            "INSERT INTO trips (user_id, date_trip, distance, total_time, pause_time) 
             VALUES (1, ?, 10.0, 1000.0, 100.0)",
        )
        .bind(&today_str)
        .execute(&pool)
        .await
        .unwrap();

        // viaggio 2: 40 giorni fa
        sqlx::query(
            "INSERT INTO trips (user_id, date_trip, distance, total_time, pause_time) 
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