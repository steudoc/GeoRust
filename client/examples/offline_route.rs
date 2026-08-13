#[path = "../src/movement/mod.rs"]
mod movement;

use std::path::Path;

use movement::{RouteSimulator, load_route};
use tokio::sync::mpsc;

const SPEED_FACTOR: u32 = 600;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let csv_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/Torino-Asti.csv");
    let route = load_route(&csv_path)?;

    println!("Percorso caricato da {}", csv_path.display());
    println!("Punti validi: {}", route.len());
    println!("Riproduzione accelerata: {SPEED_FACTOR}x\n");

    let simulator = RouteSimulator::new(route, SPEED_FACTOR)?;
    let (sender, mut receiver) = mpsc::channel(16);

    let simulator_task = tokio::spawn(simulator.start(sender));

    while let Some(point) = receiver.recv().await {
        println!(
            "t={:>4}s  lat={:.7}  lon={:.7}",
            point.elapsed.as_secs(),
            point.coordinates.get_latitudine(),
            point.coordinates.get_longitudine()
        );
    }

    let emitted_points = simulator_task.await?;
    println!("\nSimulazione terminata: {emitted_points} punti emessi");

    Ok(())
}
