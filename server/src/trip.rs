use std::{error::Error, fmt, time::Duration};
use chrono::{DateTime, Utc};
use common::{UserState, tracking::Coordinates};

const STILL_THRESHOLD: Duration = Duration::from_secs(3 * 60);  // 3 minuti soglia per passare da in movimento a fermo

/// Posizione del tragitto associata al tempo in cui il server l'ha ricevuta
#[derive(Debug, Clone, PartialEq)]
pub struct PositionSample {
    pub coordinates: Coordinates,
    pub received_at: DateTime<Utc>,
}

/// Errore prodotto quando i tempi non sono in ordine crescente
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripError {
    NonIncreasingTimestamp,
}

impl fmt::Display for TripError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonIncreasingTimestamp => {
                write!(
                    formatter,
                    "Il timestamp deve essere successivo al precedente"
                )
            }
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


    /// Registra una posizione e restituisce il nuovo stato dell'utente
    pub fn record_position(
        &mut self,
        coordinates: Coordinates,
        received_at: DateTime<Utc>,
    ) -> Result<UserState, TripError> {

        let Some(previous) = self.positions.last().cloned() else { // Se non ci sono ancora coordinate inviate, questa è la prima
            self.positions.push(PositionSample {
                coordinates,
                received_at,
            });
            self.state = UserState::Still; // Si passa allo stato fermo: la transizione da fermo a in movimento si ha al primo cambiamento di coordinata
            return Ok(self.state);
        };

        if received_at <= previous.received_at {
            return Err(TripError::NonIncreasingTimestamp);
        }

        let elapsed = received_at
            .signed_duration_since(previous.received_at)
            .to_std()
            .map_err(|_| TripError::NonIncreasingTimestamp)?;
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
            received_at,
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
    use chrono::{TimeZone, Utc};

    use super::*;

    fn coordinates(latitude: f64) -> Coordinates {
        Coordinates::new(latitude, 7.0).expect("coordinate del test valide")
    }

    fn timestamp(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0)
            .single()
            .expect("timestamp del test valido")
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

        let state = trip
            .record_position(coordinates(45.0), timestamp(0))
            .unwrap();

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_positions().len(), 1);
        assert_eq!(trip.get_moving_seconds(), 0);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn unchanged_position_while_still_increases_stopped_time() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();

        let state = trip
            .record_position(coordinates(45.0), timestamp(30))
            .unwrap();

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_stopped_seconds(), 30);
        assert_eq!(trip.get_moving_seconds(), 0);
    }

    #[test]
    fn changed_position_changes_state_to_moving() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();

        let state = trip
            .record_position(coordinates(45.1), timestamp(30))
            .unwrap();

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn moving_user_remains_moving_before_three_still_minutes() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(30))
            .unwrap();

        let state = trip
            .record_position(coordinates(45.1), timestamp(30 + 179))
            .unwrap();

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn moving_user_becomes_still_after_three_minutes() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(30))
            .unwrap();

        let state = trip
            .record_position(coordinates(45.1), timestamp(210))
            .unwrap();

        assert_eq!(state, UserState::Still);
        assert_eq!(trip.get_moving_seconds(), 30);
        assert_eq!(trip.get_stopped_seconds(), 180);
    }

    #[test]
    fn movement_before_three_minutes_assigns_pending_time_to_movement() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(30))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(60))
            .unwrap();

        let state = trip
            .record_position(coordinates(45.2), timestamp(90))
            .unwrap();

        assert_eq!(state, UserState::Moving);
        assert_eq!(trip.get_moving_seconds(), 90);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn disconnect_commits_unconfirmed_time_as_movement() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(0))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(30))
            .unwrap();
        trip.record_position(coordinates(45.1), timestamp(60))
            .unwrap();

        trip.disconnect();

        assert_eq!(trip.get_state(), UserState::Disconnected);
        assert_eq!(trip.get_moving_seconds(), 60);
        assert_eq!(trip.get_stopped_seconds(), 0);
    }

    #[test]
    fn rejects_non_increasing_timestamps_without_adding_samples() {
        let mut trip = Trip::new(42);
        trip.record_position(coordinates(45.0), timestamp(30))
            .unwrap();

        let result = trip.record_position(coordinates(45.1), timestamp(30));

        assert_eq!(result, Err(TripError::NonIncreasingTimestamp));
        assert_eq!(trip.get_positions().len(), 1);
    }
}
