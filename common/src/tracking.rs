use serde::{Deserialize, Serialize};

/// Coordinate geografiche
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Coordinates{
    latitude: f64,
    longitude: f64,
}

impl Coordinates {
    // Latitudine compresa tra -90 e 90
    // Longitudine compresa tra -180 e 180

    pub fn new(latitude: f64, longitude: f64) -> Option<Self> {
        let latitude_is_valid = latitude.is_finite() && (-90.0..=90.0).contains(&latitude);
        let longitude_is_valid = longitude.is_finite() && (-180.0..=180.0).contains(&longitude);

        if !latitude_is_valid || !longitude_is_valid {
            return None;
        }

        Some(Self {latitude, longitude})
    }

    pub fn get_latitude(&self) -> f64 {
        self.latitude
    }

    pub fn get_longitude(&self) -> f64 {
        self.longitude
    }
}










// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_valid_coordinates() {
        let coordinates = Coordinates::new(45.0618513, 7.6606506);

        assert!(coordinates.is_some());
    }

    #[test]
    fn rejects_invalid_latitude() {
        assert!(Coordinates::new(91.0, 7.0).is_none());
        assert!(Coordinates::new(-91.0, 7.0).is_none());
    }

    #[test]
    fn rejects_invalid_longitude() {
        assert!(Coordinates::new(45.0, 181.0).is_none());
        assert!(Coordinates::new(45.0, -181.0).is_none());
    }

    #[test]
    fn rejects_non_finite_values() {
        assert!(Coordinates::new(f64::NAN, 7.0).is_none());
        assert!(Coordinates::new(45.0, f64::INFINITY).is_none());
    }
}