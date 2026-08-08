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