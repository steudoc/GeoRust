use super::RoutePoint;
use anyhow::{Result, bail};
use std::time::Duration;
use tokio::{sync::mpsc, time::sleep};

/// Riproduce temporalmente un percorso
///
/// `speed_factor` stabilisce la velocità:
///
/// - 1: tempo reale;
/// - 2: velocità doppia;
/// - 10: velocità dieci volte maggiore;
/// - 60: 30 secondi del percorso diventano 500 millisecondi.
pub struct RouteSimulator {
    route: Vec<RoutePoint>,
    speed_factor: u32,
}

impl RouteSimulator {
    pub fn new(route: Vec<RoutePoint>, speed_factor: u32) -> Result<Self> {
        if route.is_empty() {
            bail!("Errore: percorso vuoto");
        }

        if speed_factor == 0 {
            bail!("Fattore di velocità deve essere maggiore di zero");
        }

        Ok(Self {
            route,
            speed_factor,
        })
    }

    /// Riproduce il percorso e restituisce il numero di punti emessi
    ///
    /// La riproduzione termina anticipatamente se il ricevitore
    /// del canale viene chiuso
    pub async fn start(self, output: mpsc::Sender<RoutePoint>) -> usize {
        let mut previous_elapsed = Duration::ZERO;
        let mut emitted_points = 0;

        for point in self.route {
            // Attendiamo la differenza rispetto al punto precedente, non il
            // tempo assoluto del punto dall'inizio del percorso.
            let route_delay = point.elapsed.saturating_sub(previous_elapsed);

            let real_delay = route_delay / self.speed_factor;

            if !real_delay.is_zero() {
                sleep(real_delay).await;
            }

            previous_elapsed = point.elapsed;

            if output.send(point).await.is_err() {
                break;
            }

            emitted_points += 1;
        }

        emitted_points
    }
}







// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use common::tracking::Coordinates;

    fn route_point(elapsed_seconds: u64, latitude: f64) -> RoutePoint {
        RoutePoint {
            elapsed: Duration::from_secs(elapsed_seconds),
            coordinates: Coordinates::new(latitude, 7.0)
                .expect("le coordinate del test devono essere valide"),
        }
    }

    #[test]
    fn rejects_empty_route() {
        let result = RouteSimulator::new(Vec::new(), 1);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_zero_speed_factor() {
        let route = vec![route_point(0, 45.0)];

        let result = RouteSimulator::new(route, 0);

        assert!(result.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn emits_points_at_the_expected_times() {
        let route = vec![
            route_point(0, 45.0),
            route_point(30, 45.1),
            route_point(60, 45.2),
        ];

        let simulator =
            RouteSimulator::new(route, 1).expect("il simulatore dovrebbe essere valido");
        let (sender, mut receiver) = mpsc::channel(3);

        let simulator_task = tokio::spawn(simulator.start(sender));
        tokio::task::yield_now().await;

        let first = receiver
            .try_recv()
            .expect("il primo punto deve essere emesso immediatamente");
        assert_eq!(first.elapsed, Duration::ZERO);
        assert_eq!(first.coordinates.get_latitude(), 45.0);

        tokio::time::advance(Duration::from_secs(29)).await;
        tokio::task::yield_now().await;
        assert!(receiver.try_recv().is_err());

        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let second = receiver
            .try_recv()
            .expect("il secondo punto deve arrivare dopo 30 secondi");
        assert_eq!(second.elapsed, Duration::from_secs(30));
        assert_eq!(second.coordinates.get_latitude(), 45.1);

        tokio::time::advance(Duration::from_secs(30)).await;
        tokio::task::yield_now().await;
        let third = receiver
            .try_recv()
            .expect("il terzo punto deve arrivare dopo altri 30 secondi");
        assert_eq!(third.elapsed, Duration::from_secs(60));
        assert_eq!(third.coordinates.get_latitude(), 45.2);

        let emitted_points = simulator_task
            .await
            .expect("il task del simulatore non deve terminare con panic");
        assert_eq!(emitted_points, 3);
    }

    #[tokio::test(start_paused = true)]
    async fn speed_factor_accelerates_playback() {
        let route = vec![route_point(0, 45.0), route_point(30, 45.1)];

        // Con fattore 10, 30 secondi del percorso diventano 3 secondi.
        let simulator =
            RouteSimulator::new(route, 10).expect("il simulatore dovrebbe essere valido");
        let (sender, mut receiver) = mpsc::channel(2);

        let simulator_task = tokio::spawn(simulator.start(sender));
        tokio::task::yield_now().await;

        let first = receiver
            .try_recv()
            .expect("il primo punto deve essere emesso immediatamente");
        assert_eq!(first.elapsed, Duration::ZERO);

        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert!(receiver.try_recv().is_err());

        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let second = receiver
            .try_recv()
            .expect("il secondo punto deve arrivare dopo 3 secondi");
        assert_eq!(second.elapsed, Duration::from_secs(30));

        assert_eq!(simulator_task.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn stops_when_receiver_is_closed() {
        let route = vec![route_point(0, 45.0)];
        let simulator =
            RouteSimulator::new(route, 1).expect("il simulatore dovrebbe essere valido");
        let (sender, receiver) = mpsc::channel(1);

        drop(receiver);

        let emitted_points = simulator.start(sender).await;

        assert_eq!(emitted_points, 0);
    }
}
