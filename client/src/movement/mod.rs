mod csv_route;
pub use csv_route::load_route; // Permette al client di usare "use client::movement::load_route;" invece del percorso interno più lungo

use std::time::Duration;
use common::tracking::Coordinates;

/// Rapresenta un punto del percorso simulato
#[derive(Debug, Clone, PartialEq)]
pub struct RoutePoint{
    pub elapsed: Duration,  // indica quanto tempo è passato dall'inizio del percorso
    pub coordinates: Coordinates,
}






// Test di struttura
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn test_route_point(){
        let point = RoutePoint{
            elapsed: Duration::from_secs(30), // 30 secondi
            coordinates: Coordinates::new(45.0575226, 7.6618322).expect("Coordinate devono essere valide"),
        };

        assert_eq!(point.elapsed, Duration::from_secs(30));
        assert_eq!(point.coordinates.get_latitude(), 45.0575226);
        assert_eq!(point.coordinates.get_longitude(), 7.6618322);
    }
}