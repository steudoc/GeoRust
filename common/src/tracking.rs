use serde::{
    Deserialize,
    Deserializer, // Lettore generico dei dati
    Serialize,
    de::Error,
}; // Metodo custom() per creare errori Serde

/// Coordinata geografica
#[derive(Serialize, Debug, Clone, Copy, PartialEq)] // Serialize si può derivare senza problemi perché riceve coordinate già validate da trasformare in json
pub struct Coordinata {
    latitudine: f64,
    longitudine: f64,
}

/*
Non deriviamo Deserialize in modo automatico perché derivandolo non verrebbe chiamato il costruttore Coordinata::new()
su cui c'è la verifica della validità delle coordinate: Serde ricevendo le coordinate in formato json dal client poteva
inserire valori di long e lat invalidi, quindi anche serde deve usare il costruttore di Coordinata
*/
#[derive(Deserialize)]
struct TempCoordinata {
    // Per valori letti dal json e non ancora validati
    latitudine: f64,
    longitudine: f64,
}

impl<'de> Deserialize<'de> for Coordinata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let temp = TempCoordinata::deserialize(deserializer)?;

        Coordinata::new(temp.latitudine, temp.longitudine)
            .ok_or_else(|| D::Error::custom("Coordinate geografiche non valide"))
    }
}

impl Coordinata {
    // Latitudine compresa tra -90 e 90
    // Longitudine compresa tra -180 e 180

    pub fn new(latitudine: f64, longitudine: f64) -> Option<Self> {
        let lat_is_valid = latitudine.is_finite() && (-90.0..=90.0).contains(&latitudine);
        let long_is_valid = longitudine.is_finite() && (-180.0..=180.0).contains(&longitudine);

        if !lat_is_valid || !long_is_valid {
            return None;
        }

        Some(Self {
            latitudine,
            longitudine,
        })
    }

    pub fn get_latitudine(&self) -> f64 {
        self.latitudine
    }

    pub fn get_longitudine(&self) -> f64 {
        self.longitudine
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
        let coordinates = Coordinata::new(45.0618513, 7.6606506);

        assert!(coordinates.is_some());
    }

    #[test]
    fn rejects_invalid_latitude() {
        assert!(Coordinata::new(91.0, 7.0).is_none());
        assert!(Coordinata::new(-91.0, 7.0).is_none());
    }

    #[test]
    fn rejects_invalid_longitude() {
        assert!(Coordinata::new(45.0, 181.0).is_none());
        assert!(Coordinata::new(45.0, -181.0).is_none());
    }

    #[test]
    fn rejects_non_finite_values() {
        assert!(Coordinata::new(f64::NAN, 7.0).is_none());
        assert!(Coordinata::new(45.0, f64::INFINITY).is_none());
    }

    #[test]
    fn rejects_invalid_coordinates_during_json_deserialization() {
        let json = r#"{"latitude": 91.0, "longitude": 7.0}"#;

        let result = serde_json::from_str::<Coordinata>(json);

        assert!(result.is_err());
    }
}
