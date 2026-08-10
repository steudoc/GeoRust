use std::time::Duration;
use common::tracking::Coordinates;

// Rapresenta un punto del percorso simulato
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
            coordinates: Coordinates{
                latitude: 45.0575226,
                longitude: 7.6618322,
            }
        };

        assert_eq!(point.elapsed, Duration::from_secs(30));
        assert_eq!(point.coordinates.latitude, 45.0575226);
        assert_eq!(point.coordinates.longitude, 7.6618322);
    }
}