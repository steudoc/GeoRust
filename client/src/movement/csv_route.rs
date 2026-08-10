use super::RoutePoint;
use anyhow::{Context, Result, bail};
use common::tracking::Coordinates;
use serde::Deserialize;
use std::{fs::File, io::Read, path::Path, time::Duration};

/// Corrisponde direttamente alle colonne nel CSV:
/// time, latitude, longitude
#[derive(Debug, Deserialize)]
struct CsvRecord {
    time: String,
    latitude: f64,
    longitude: f64,
}

/// Carica un percorso da un file CSV con path valido
///
/// È separata da `load_route` per poterla testare usando stringhe e file temporanei
pub fn load_route(path: impl AsRef<Path>) -> Result<Vec<RoutePoint>> {
    let path = path.as_ref();

    let file = File::open(path)
        .with_context(|| format!("impossibile aprire il file {}", path.display()))?;

    load_route_from_reader(file)
        .with_context(|| format!("percorso non valido nel file {}", path.display()))
}

/// Legge e valida un tragitto da un file CSV
fn load_route_from_reader<R: Read>(reader: R) -> Result<Vec<RoutePoint>> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(reader);

    let mut route: Vec<RoutePoint> = Vec::new();

    for (row_index, result) in csv_reader.deserialize::<CsvRecord>().enumerate() {
        // Prima riga contiene intestazione, si inizia dalla seconda
        let line = row_index + 2;

        let record = result.with_context(|| format!("Riga {line}: formato CSV non valido"))?;

        let elapsed = parse_elapsed_time(&record.time)
            .with_context(|| format!("Riga {line}: tempo non valido"))?;

        let coordinates =
            Coordinates::new(record.latitude, record.longitude).with_context(|| {
                format!(
                    "Riga {line}: coordinates non valide ({}, {})",
                    record.latitude, record.longitude
                )
            })?;

        if let Some(previous) = route.last()
            && elapsed <= previous.elapsed
        {
            bail!(
                "Riga {line}: il tempo deve essere successivo al tempo del punto precedente (tempo deve essere crescente)"
            );
        }

        route.push(RoutePoint {
            elapsed,
            coordinates,
        });
    }

    if route.is_empty() {
        bail!("Il percorso non contiene punti");
    }

    Ok(route)
}

/// Converte la stringa nel file CSV dal fomato 'MM:SS' in Duration
fn parse_elapsed_time(time: &str) -> Result<Duration> {
    let (minutes_text, seconds_text) = time
        .split_once(':')
        .with_context(|| format!("Formato atteso MM:SS, ricevuto {time:?}"))?;

    if minutes_text.is_empty() || seconds_text.len() != 2 || seconds_text.contains(':') {
        bail!("Formato atteso MM:SS, ricevuto {time:?}");
    }

    let minutes: u64 = minutes_text
        .parse()
        .with_context(|| format!("Minnuti non validi in {time:?}"))?;
    let seconds: u64 = seconds_text
        .parse()
        .with_context(|| format!("Secondi non validi in {time:?}"))?;

    if seconds >= 60 {
        bail!("I secondi devono essere compresi tra 00 e 59");
    }

    // Trasformo tutto in secondi per avere una durata
    let total_seconds = minutes
        .checked_mul(60)
        .and_then(|value| value.checked_add(seconds))
        .context("Tempo troppo grande")?;

    Ok(Duration::from_secs(total_seconds))
}

// ---------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_valid_route() {
        let csv = "\
time,latitude,longitude
00:00,45.0618513,7.6606506
00:30,45.0575226,7.6618322
01:00,45.0531939,7.6630137
";

        let route =
            load_route_from_reader(csv.as_bytes()).expect("il percorso dovrebbe essere valido");

        assert_eq!(route.len(), 3);
        assert_eq!(route[0].elapsed, Duration::from_secs(0));
        assert_eq!(route[1].elapsed, Duration::from_secs(30));
        assert_eq!(route[2].elapsed, Duration::from_secs(60));

        assert_eq!(route[0].coordinates.get_latitude(), 45.0618513);
        assert_eq!(route[0].coordinates.get_longitude(), 7.6606506);
    }

    #[test]
    fn rejects_invalid_latitude() {
        let csv = "\
time,latitude,longitude
00:00,100.0,7.6606506
";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn rejects_invalid_longitude() {
        let csv = "\
time,latitude,longitude
00:00,45.0,200.0
";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn rejects_invalid_seconds() {
        let csv = "\
time,latitude,longitude
00:75,45.0,7.0
";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn rejects_invalid_time_format() {
        let csv = "\
time,latitude,longitude
30,45.0,7.0
";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn rejects_non_increasing_times() {
        let csv = "\
time,latitude,longitude
00:30,45.0,7.0
00:30,45.1,7.1
";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn accepts_repeated_coordinates() {
        let csv = "\
time,latitude,longitude
00:00,45.0,7.0
00:30,45.0,7.0
";

        let route = load_route_from_reader(csv.as_bytes())
            .expect("coordinate uguali rappresentano una possibile pausa");

        assert_eq!(route.len(), 2);
        assert_eq!(route[0].coordinates, route[1].coordinates);
    }

    #[test]
    fn rejects_empty_route() {
        let csv = "time,latitude,longitude\n";

        assert!(load_route_from_reader(csv.as_bytes()).is_err());
    }

    #[test]
    fn loads_torino_asti_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/Torino-Asti.csv");

        let route = load_route(path).expect("Torino-Asti.csv dovrebbe essere valido");

        assert_eq!(route.len(), 96);
        assert_eq!(route.first().unwrap().elapsed, Duration::from_secs(0));
        assert_eq!(
            route.last().unwrap().elapsed,
            Duration::from_secs(47 * 60 + 30)
        );
    }
}
