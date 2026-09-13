use common::{POSITION_INTERVAL_SECONDS, UserState, tracking::Coordinata};
use std::{error::Error, fmt, time::Duration};

const STILL_THRESHOLD: Duration = Duration::from_secs(3 * 60); // 3 minuti soglia per passare da in movimento a fermo
const EARTH_RADIUS_KM: f64 = 6_371.0; // Per calcolare la distanza totale del tragitto

/// Posizione del tragitto associata al tempo logico trascorso nel CSV.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionSample {
    pub coordinates: Coordinata,
    pub elapsed_seconds: u64,
}

/// Errori prodotti da una sequenza temporale non conforme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripError {
    InvalidInitialTime { received: u64 },
    InvalidTimeInterval { expected: u64, received: u64 },
    ElapsedTimeOverflow,
}

impl fmt::Display for TripError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInitialTime { received } => {
                write!(
                    formatter,
                    "Il primo punto deve avere tempo logico 0 secondi, ricevuti {received}"
                )
            }
            Self::InvalidTimeInterval { expected, received } => write!(
                formatter,
                "Intervallo temporale non valido: attesi {expected} secondi, ricevuti {received}. Le posizioni devono essere distanziate di {POSITION_INTERVAL_SECONDS} secondi"
            ),
            Self::ElapsedTimeOverflow => write!(formatter, "Tempo logico troppo grande"),
        }
    }
}

impl Error for TripError {}

/// Tragitto attivo di un utente
#[derive(Debug)]
pub struct Trip {
    user_id: i64,
    positions: Vec<PositionSample>,
    state: UserState,
    moving_time: Duration,
    stopped_time: Duration,

    /// Tempo durante il quale le coordinate non sono cambiate, ma ancora sotto la
    /// soglia dei tre minuti necessari per passare a Fermo
    pending_still_time: Duration,
}

impl Trip {
    pub fn new(user_id: i64) -> Self {
        Self {
            user_id,
            positions: Vec::new(),
            state: UserState::Disconnected,
            moving_time: Duration::ZERO,
            stopped_time: Duration::ZERO,
            pending_still_time: Duration::ZERO,
        }
    }

    pub fn get_user_id(&self) -> i64 {
        self.user_id
    }

    pub fn get_positions(&self) -> &[PositionSample] {
        &self.positions
    }

    pub fn get_state(&self) -> UserState {
        self.state
    }

    pub fn get_moving_seconds(&self) -> u64 {
        self.moving_time.as_secs()
    }

    pub fn get_stopped_seconds(&self) -> u64 {
        self.stopped_time.as_secs()
    }

    /// Calcola la distanza totale sommando tutti i segmenti consecutivi e arrotonda a due cifre decimali
    pub fn get_distance_km(&self) -> f64 {
        let total_distance = self
            .positions
            .windows(2)
            .map(|segment| Self::distance_between(&segment[0].coordinates, &segment[1].coordinates))
            .sum::<f64>();

        (total_distance * 100.0).round() / 100.0
    }

    /// Calcola con la formula di Haversine la distanza tra due coordinate
    fn distance_between(first: &Coordinata, last: &Coordinata) -> f64 {
        let first_latitude = first.get_latitudine().to_radians();
        let last_latitude = last.get_latitudine().to_radians();
        let latitude_delta = last_latitude - first_latitude;
        let longitude_delta = (last.get_longitudine() - first.get_longitudine()).to_radians();

        let haversine = (latitude_delta / 2.0).sin().powi(2)
            + first_latitude.cos() * last_latitude.cos() * (longitude_delta / 2.0).sin().powi(2);
        let haversine = haversine.clamp(0.0, 1.0);
        let angular_distance = 2.0 * haversine.sqrt().atan2((1.0 - haversine).sqrt());

        EARTH_RADIUS_KM * angular_distance
    }

    /// Registra una posizione e restituisce il nuovo stato dell'utente
    pub fn record_position(
        &mut self,
        coordinates: Coordinata,
        elapsed_seconds: u64,
    ) -> Result<UserState, TripError> {
        let Some(previous) = self.positions.last().cloned() else {
            // Se non ci sono ancora coordinate inviate, questa è la prima e deve avere tempo zero
            if elapsed_seconds != 0 {
                return Err(TripError::InvalidInitialTime {
                    received: elapsed_seconds,
                });
            }

            self.positions.push(PositionSample {
                coordinates,
                elapsed_seconds,
            });
            self.state = UserState::Still; // Si passa allo stato fermo: la transizione da fermo a in movimento si ha al primo cambiamento di coordinata
            return Ok(self.state);
        };

        let expected = previous
            .elapsed_seconds
            .checked_add(POSITION_INTERVAL_SECONDS)
            .ok_or(TripError::ElapsedTimeOverflow)?; // I tempi devono avanzare di 30 secondi altrimenti errore 

        if elapsed_seconds != expected {
            return Err(TripError::InvalidTimeInterval {
                expected,
                received: elapsed_seconds,
            });
        }

        let elapsed = Duration::from_secs(POSITION_INTERVAL_SECONDS);
        let position_changed = coordinates != previous.coordinates;

        match self.state {
            UserState::Disconnected => {
                // Una nuova posizione dopo una disconnessione riapre il monitoraggio da zero
                self.state = UserState::Still;
                self.pending_still_time = Duration::ZERO;
            }
            UserState::Still => {
                if position_changed {
                    self.moving_time += elapsed;
                    self.state = UserState::Moving;
                } else {
                    self.stopped_time += elapsed;
                }
            }
            UserState::Moving => {
                if position_changed {
                    // Se la coordinata cambia prima dei tre minuti, questo tempo non viene considerato una pausa
                    self.moving_time += self.pending_still_time + elapsed;
                    self.pending_still_time = Duration::ZERO;
                } else {
                    self.pending_still_time += elapsed;

                    if self.pending_still_time >= STILL_THRESHOLD {
                        // Superata la soglia per il passaggio allo stato fermo:
                        // attribuiamo retroattivamente tutto il periodo al tempo di pausa
                        self.stopped_time += self.pending_still_time;
                        self.pending_still_time = Duration::ZERO;
                        self.state = UserState::Still;
                    }
                }
            }
        }

        self.positions.push(PositionSample {
            coordinates,
            elapsed_seconds,
        });

        Ok(self.state)
    }

    /// Chiude il tragitto dell'utente
    pub fn disconnect(&mut self) {
        if self.state == UserState::Moving {
            // Se la pausa non è stata confermata (non sono passi i 3 minuti),
            // aggiungiamo quel periodo pendente dove le coordinate non cambiano al tempo di movimento
            self.moving_time += self.pending_still_time;
        }

        self.pending_still_time = Duration::ZERO;
        self.state = UserState::Disconnected;
    }
}

// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn coordinates(latitude: f64) -> Coordinata {
        Coordinata::new(latitude, 7.0).expect("coordinate del test valide")
    }

    #[test]
    fn new_trip_is_disconnected_and_empty() {
        let trip = Trip::new(42);

        assert_eq!(trip.get_user_id(), 42);
        assert_eq!(trip.get_state(), UserState::Disconnected);
        assert!(trip.get_positions().is_empty());
        assert_eq!(trip.get_moving_seconds(), 0);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn first_position_changes_state_to_still() {
        let mut trip = Trip::new(42);

        let state = trip.record_position(coordinates(45.0), 0).unwrap();

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_positions().len(), 1);
        assert_eq!(trip.get_moving_seconds(), 0);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn unchanged_position_while_still_increases_stopped_time() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();

        let state = trip.record_position(coordinates(45.0), 30).unwrap();

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_stopped_seconds(), 30);
        assert_eq!(trip.get_moving_seconds(), 0);
    }

    #[test]
    fn changed_position_changes_state_to_moving() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();

        let state = trip.record_position(coordinates(45.1), 30).unwrap();

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn moving_user_remains_moving_before_three_still_minutes() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();
        trip.record_position(coordinates(45.1), 30).unwrap();

        let mut state = UserState::Moving;
        for elapsed_seconds in [60, 90, 120, 150, 180] {
            state = trip
                .record_position(coordinates(45.1), elapsed_seconds)
                .unwrap();
        }

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn moving_user_becomes_still_after_three_minutes() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();
        trip.record_position(coordinates(45.1), 30).unwrap();

        let mut state = UserState::Moving;
        for elapsed_seconds in [60, 90, 120, 150, 180, 210] {
            state = trip
                .record_position(coordinates(45.1), elapsed_seconds)
                .unwrap();
        }

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 180);
    }

    #[test]
    fn movement_before_three_minutes_assigns_pending_time_to_movement() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();
        trip.record_position(coordinates(45.1), 30).unwrap();
        trip.record_position(coordinates(45.1), 60).unwrap();

        let state = trip.record_position(coordinates(45.2), 90).unwrap();

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 90);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn disconnect_commits_unconfirmed_time_as_movement() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();
        trip.record_position(coordinates(45.1), 30).unwrap();
        trip.record_position(coordinates(45.1), 60).unwrap();

        trip.disconnect();

        assert_eq!(trip.get_state(), UserState::Disconnected);
        assert_eq!(trip.get_moving_seconds(), 60);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn rejects_initial_time_different_from_zero() {
        let mut trip = Trip::new(42);

        let result = trip.record_position(coordinates(45.0), 30);

        assert_eq!(result, Err(TripError::InvalidInitialTime { received: 30 }));
        assert!(trip.get_positions().is_empty());
    }

    #[test]
    fn rejects_intervals_different_from_thirty_seconds() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), 0).unwrap();

        let result = trip.record_position(coordinates(45.1), 45);

        assert_eq!(
            result,
            Err(TripError::InvalidTimeInterval {
                expected: 30,
                received: 45,
            })
        );
        assert_eq!(trip.get_positions().len(), 1);
    }

    #[test]
    fn distance_is_the_sum_of_all_consecutive_segments() {
        let mut trip = Trip::new(42);
        trip.record_position(Coordinata::new(0.0, 0.0).unwrap(), 0)
            .unwrap();
        trip.record_position(Coordinata::new(0.0, 1.0).unwrap(), 30)
            .unwrap();
        trip.record_position(Coordinata::new(0.0, 0.0).unwrap(), 60)
            .unwrap();

        assert_eq!(trip.get_distance_km(), 222.39);
    }

    #[test]
    fn distance_is_zero_with_fewer_than_two_coordinates() {
        let mut trip = Trip::new(42);
        assert_eq!(trip.get_distance_km(), 0.0);

        trip.record_position(coordinates(45.0), 0).unwrap();
        assert_eq!(trip.get_distance_km(), 0.0);
    }
}
