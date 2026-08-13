use common::tracking::Coordinata;
use std::time::Duration;

mod csv_route;
pub use csv_route::load_route; // Permette al client di usare "use client::movement::load_route;" invece del percorso interno più lungo

mod simulator;
pub use simulator::RouteSimulator;

/// Rapresenta un punto del percorso simulato
#[derive(Debug, Clone, PartialEq)]
pub struct RoutePoint {
    pub elapsed: Duration, // Indica quanto tempo è passato dall'inizio del viaggio: serve a decidere QUANDO inviare il punto (non trasmesso al server)
    pub coordinates: Coordinata,
}

// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_route_point() {
        let point = RoutePoint {
            elapsed: Duration::from_secs(30), // 30 secondi
            coordinates: Coordinata::new(45.0575226, 7.6618322)
                .expect("Coordinate devono essere valide"),
        };

        assert_eq!(point.elapsed, Duration::from_secs(30));
        assert_eq!(point.coordinates.get_latitudine(), 45.0575226);
        assert_eq!(point.coordinates.get_longitudine(), 7.6618322);
    }
}
