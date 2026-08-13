#[path = "../src/trip.rs"]
mod trip;

use chrono::{DateTime, Duration, TimeZone, Utc};
use common::tracking::Coordinata;
use trip::Trip;

fn coordinates(latitude: f64, longitude: f64) -> Coordinata {
    Coordinata::new(latitude, longitude).expect("coordinate dell'esempio valide")
}

fn main() -> anyhow::Result<()> {
    let user_id = 42;
    let start = Utc
        .timestamp_opt(0, 0)
        .single()
        .expect("timestamp iniziale valido");

    let samples = [
        (0, coordinates(45.0000, 7.0000)),
        (30, coordinates(45.1000, 7.0000)),
        (60, coordinates(45.1000, 7.0000)),
        (210, coordinates(45.1000, 7.0000)),
        (240, coordinates(45.2000, 7.0000)),
    ];

    let mut trip = Trip::new(user_id);

    println!("Simulazione tragitto per l'utente {}\n", trip.get_user_id());

    for (elapsed_seconds, coordinates) in samples {
        let received_at: DateTime<Utc> = start + Duration::seconds(elapsed_seconds);
        let state = trip.record_position(coordinates, received_at)?;

        println!(
            "t={elapsed_seconds:>3}s  lat={:.4}  lon={:.4}  stato={state:?}  movimento={}s  fermo={}s",
            coordinates.get_latitudine(),
            coordinates.get_longitudine(),
            trip.get_moving_seconds(),
            trip.get_stopped_seconds()
        );
    }

    trip.disconnect();

    println!("\nTragitto terminato");
    println!("Stato finale: {:?}", trip.get_state());
    println!("Posizioni conservate: {}", trip.get_positions().len());
    println!("Tempo in movimento: {}s", trip.get_moving_seconds());
    println!("Tempo da fermo: {}s", trip.get_stopped_seconds());

    Ok(())
}
