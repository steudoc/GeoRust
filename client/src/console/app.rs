use std::{fs, path::PathBuf};

use chrono::{DateTime, Local, Utc};

const DATA_DIRECTORY: &str = "../data";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteOption {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    Command,
    RouteSelection,
    #[cfg(debug_assertions)]
    SpeedSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TripStatus {
    Idle,
    Running {
        route_name: String,
        speed_factor: u32,
        sent_points: usize,
    },
    AwaitingCompletion {
        route_name: String,
    },
    WaitingReconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionStatus {
    Connected,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MessageKind {
    Direct,
    Broadcast,
    Sent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MessageEntry {
    pub kind: MessageKind,
    pub timestamp: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AppAction {
    None,
    SendText(String),
    StartTrip {
        route: RouteOption,
        speed_factor: u32,
    },
    Exit,
}

pub(super) struct ClientApp {
    pub user_id: i64,
    pub username: String,
    pub input: String,
    pub console_lines: Vec<String>,
    pub messages: Vec<MessageEntry>,
    pub input_mode: InputMode,
    pub trip_status: TripStatus,
    pub connection_status: ConnectionStatus,
    pub available_routes: Vec<RouteOption>,
    pub selected_route: Option<RouteOption>,
    pub console_scroll: u16,
    pub messages_scroll: u16,
}

impl ClientApp {
    pub fn new(user_id: i64, username: String) -> Self {
        Self {
            user_id,
            username,
            input: String::new(),
            console_lines: vec![
                "Autenticazione completata.".to_string(),
                "Connessione al server in corso...".to_string(),
            ],
            messages: Vec::new(),
            input_mode: InputMode::Command,
            trip_status: TripStatus::Idle,
            connection_status: ConnectionStatus::Reconnecting,
            available_routes: Vec::new(),
            selected_route: None,
            console_scroll: 0,
            messages_scroll: 0,
        }
    }

    pub fn submit_input(&mut self) -> AppAction {
        self.console_scroll = 0;
        let input = std::mem::take(&mut self.input);
        let value = input.trim();

        match self.input_mode {
            InputMode::Command => self.process_command(value),
            InputMode::RouteSelection => self.process_route_selection(value),
            #[cfg(debug_assertions)]
            InputMode::SpeedSelection => self.process_speed_selection(value),
        }
    }

    fn process_command(&mut self, value: &str) -> AppAction {
        if value.is_empty() {
            return AppAction::None;
        }

        self.push_console(format!("> {value}"));
        let mut parts = value.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or_default().to_lowercase();
        let argument = parts.next().unwrap_or_default().trim();

        match command.as_str() {
            "help" => {
                self.push_console("Comandi disponibili:");
                self.push_console("  help        mostra questa guida");
                self.push_console("  msg <testo> invia un messaggio al server");
                self.push_console("  start       configura e avvia un nuovo trip");
                self.push_console("  exit        chiude il client");
                AppAction::None
            }
            "msg" => {
                if argument.is_empty() {
                    self.push_console("Uso: msg <testo>");
                    AppAction::None
                } else if self.connection_status != ConnectionStatus::Connected {
                    self.push_console(
                        "Impossibile inviare: connessione al server non disponibile.",
                    );
                    AppAction::None
                } else {
                    AppAction::SendText(argument.to_string())
                }
            }
            "start" => self.prepare_route_selection(),
            "exit" => AppAction::Exit,
            _ => {
                self.push_console(format!("Comando sconosciuto: {command}. Digita 'help'."));
                AppAction::None
            }
        }
    }

    fn prepare_route_selection(&mut self) -> AppAction {
        if self.connection_status != ConnectionStatus::Connected {
            self.push_console("Attendi la riconnessione prima di avviare un trip.");
            return AppAction::None;
        }

        if self.trip_status != TripStatus::Idle {
            self.push_console("È già presente un trip in corso o in fase di completamento.");
            return AppAction::None;
        }

        match discover_routes() {
            Ok(routes) if routes.is_empty() => {
                self.push_console("Nessun percorso CSV disponibile nella cartella data.");
            }
            Ok(routes) => {
                self.available_routes = routes;
                self.input_mode = InputMode::RouteSelection;
                self.push_console("Percorsi disponibili:");
                let route_names: Vec<String> = self
                    .available_routes
                    .iter()
                    .enumerate()
                    .map(|(index, route)| format!("  {}. {}", index + 1, route.name))
                    .collect();
                for route_name in route_names {
                    self.push_console(route_name);
                }
                self.push_console("Inserisci il numero oppure partenza-destinazione:");
            }
            Err(error) => {
                self.push_console(format!("Impossibile leggere i percorsi: {error}"));
            }
        }

        AppAction::None
    }

    fn process_route_selection(&mut self, value: &str) -> AppAction {
        if value.eq_ignore_ascii_case("exit") {
            return AppAction::Exit;
        }

        let selected = value
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| self.available_routes.get(index))
            .cloned()
            .or_else(|| {
                let normalized = value.to_lowercase();
                self.available_routes
                    .iter()
                    .find(|route| route.name.to_lowercase() == normalized)
                    .cloned()
            });

        let Some(route) = selected else {
            self.push_console(
                "Percorso non disponibile. Inserisci un numero dell'elenco o il nome esatto.",
            );
            return AppAction::None;
        };

        self.push_console(format!("Percorso selezionato: {}", route.name));

        #[cfg(debug_assertions)]
        {
            self.selected_route = Some(route);
            self.input_mode = InputMode::SpeedSelection;
            self.push_console("Scegli il fattore di velocità:");
            self.push_console("  1  - tempo reale");
            self.push_console("  2  - velocità doppia");
            self.push_console("  10 - velocità dieci volte maggiore");
            self.push_console("  60 - 30 secondi diventano 500 millisecondi");
            self.push_console("Fattore [1]:");
            AppAction::None
        }

        #[cfg(not(debug_assertions))]
        {
            self.input_mode = InputMode::Command;
            AppAction::StartTrip {
                route,
                speed_factor: 1,
            }
        }
    }

    #[cfg(debug_assertions)]
    fn process_speed_selection(&mut self, value: &str) -> AppAction {
        if value.eq_ignore_ascii_case("exit") {
            return AppAction::Exit;
        }

        let speed_factor = if value.is_empty() {
            1
        } else {
            match value.parse::<u32>() {
                Ok(value) if value > 0 => value,
                _ => {
                    self.push_console(
                        "Il fattore deve essere un numero intero maggiore di zero. Riprova:",
                    );
                    return AppAction::None;
                }
            }
        };

        let Some(route) = self.selected_route.take() else {
            self.input_mode = InputMode::Command;
            self.push_console("Selezione del percorso non disponibile. Digita nuovamente 'start'.");
            return AppAction::None;
        };

        self.input_mode = InputMode::Command;
        AppAction::StartTrip {
            route,
            speed_factor,
        }
    }

    pub fn mark_connected(&mut self) {
        let was_reconnecting = self.connection_status == ConnectionStatus::Reconnecting;
        self.connection_status = ConnectionStatus::Connected;
        if self.trip_status == TripStatus::WaitingReconnect {
            self.trip_status = TripStatus::Idle;
        }
        if was_reconnecting {
            self.push_console("Connessione WebSocket attiva.");
        }
    }

    pub fn mark_disconnected(&mut self, message: impl Into<String>) {
        self.connection_status = ConnectionStatus::Reconnecting;
        self.push_console(message);
        self.push_console("Riconnessione automatica in corso...");
    }

    pub fn mark_trip_started(&mut self, route_name: String, speed_factor: u32) {
        self.trip_status = TripStatus::Running {
            route_name: route_name.clone(),
            speed_factor,
            sent_points: 0,
        };
        self.push_console(format!("Trip avviato: {route_name} ({speed_factor}x)."));
    }

    pub fn record_sent_position(&mut self, elapsed_seconds: u64, latitude: f64, longitude: f64) {
        let sent_points = match &mut self.trip_status {
            TripStatus::Running { sent_points, .. } => {
                *sent_points += 1;
                *sent_points
            }
            _ => return,
        };

        self.push_console(format!(
            "Posizione #{sent_points}: t={elapsed_seconds}s, lat={latitude:.7}, lon={longitude:.7}"
        ));
    }

    pub fn mark_route_finished(&mut self) {
        let route_name = match &self.trip_status {
            TripStatus::Running { route_name, .. } => route_name.clone(),
            _ => return,
        };
        self.trip_status = TripStatus::AwaitingCompletion { route_name };
        self.push_console("Tutte le coordinate sono state inviate. Attendo il server...");
    }

    pub fn mark_trip_completed(
        &mut self,
        points: usize,
        moving_seconds: u64,
        stopped_seconds: u64,
    ) {
        self.trip_status = TripStatus::WaitingReconnect;
        self.push_console(format!(
            "Trip completato: {points} punti, movimento {moving_seconds}s, pausa {stopped_seconds}s."
        ));
        self.push_console("Il server chiuderà la connessione; attendo la riconnessione.");
    }

    pub fn invalidate_active_trip(&mut self, reason: impl Into<String>) {
        if matches!(
            self.trip_status,
            TripStatus::Running { .. } | TripStatus::AwaitingCompletion { .. }
        ) {
            self.push_console(reason);
        }
        self.trip_status = TripStatus::Idle;
        self.input_mode = InputMode::Command;
        self.selected_route = None;
    }

    pub fn add_sent_message(&mut self, text: String) {
        self.messages.push(MessageEntry {
            kind: MessageKind::Sent,
            timestamp: Local::now().format("%H:%M:%S").to_string(),
            text,
        });
        self.messages_scroll = 0;
    }

    pub fn add_direct_message(&mut self, id: i64, text: String, timestamp: DateTime<Utc>) {
        self.messages.push(MessageEntry {
            kind: MessageKind::Direct,
            timestamp: timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string(),
            text,
        });
        self.messages_scroll = 0;
    }

    pub fn add_broadcast_message(&mut self, id: i64, text: String, timestamp: DateTime<Utc>) {
        self.messages.push(MessageEntry {
            kind: MessageKind::Broadcast,
            timestamp: timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string(),
            text,
        });
        self.messages_scroll = 0;
    }

    pub fn push_console(&mut self, message: impl Into<String>) {
        self.console_lines.push(message.into());
        self.console_scroll = 0;
    }

    pub fn scroll_up(&mut self) {
        self.console_scroll = self.console_scroll.saturating_add(3);
        self.messages_scroll = self.messages_scroll.saturating_add(3);
    }

    pub fn scroll_down(&mut self) {
        self.console_scroll = self.console_scroll.saturating_sub(3);
        self.messages_scroll = self.messages_scroll.saturating_sub(3);
    }
}

fn discover_routes() -> anyhow::Result<Vec<RouteOption>> {
    let data_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DATA_DIRECTORY);
    let mut routes = Vec::new();

    for entry in fs::read_dir(&data_directory)? {
        let entry = entry?;
        let path = entry.path();
        let is_csv = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"));
        if !entry.file_type()?.is_file() || !is_csv {
            continue;
        }

        let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };

        routes.push(RouteOption {
            name: name.to_string(),
            path,
        });
    }

    routes.sort_by_key(|route| route.name.to_lowercase());
    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected_app() -> ClientApp {
        let mut app = ClientApp::new(42, "utente".to_string());
        app.mark_connected();
        app
    }

    #[test]
    fn parses_message_command() {
        let mut app = connected_app();
        app.input = "msg ciao server".to_string();

        assert_eq!(
            app.submit_input(),
            AppAction::SendText("ciao server".to_string())
        );
    }

    #[test]
    fn message_command_remains_available_while_trip_is_running() {
        let mut app = connected_app();
        app.trip_status = TripStatus::Running {
            route_name: "Torino-Asti".to_string(),
            speed_factor: 1,
            sent_points: 3,
        };
        app.input = "msg sono ancora in viaggio".to_string();

        assert_eq!(
            app.submit_input(),
            AppAction::SendText("sono ancora in viaggio".to_string())
        );
    }

    #[test]
    fn exit_while_running_does_not_mark_the_trip_as_completed() {
        let mut app = connected_app();
        app.trip_status = TripStatus::Running {
            route_name: "Torino-Asti".to_string(),
            speed_factor: 1,
            sent_points: 3,
        };
        app.input = "exit".to_string();

        assert_eq!(app.submit_input(), AppAction::Exit);
        assert!(matches!(app.trip_status, TripStatus::Running { .. }));
    }

    #[test]
    fn start_discovers_routes_and_enters_selection() {
        let mut app = connected_app();
        app.input = "start".to_string();

        assert_eq!(app.submit_input(), AppAction::None);
        assert_eq!(app.input_mode, InputMode::RouteSelection);
        assert!(
            app.available_routes
                .iter()
                .any(|route| route.name == "Torino-Asti")
        );
    }

    #[test]
    fn route_can_be_selected_case_insensitively() {
        let mut app = connected_app();
        app.input_mode = InputMode::RouteSelection;
        app.available_routes = vec![RouteOption {
            name: "Torino-Asti".to_string(),
            path: PathBuf::from("Torino-Asti.csv"),
        }];
        app.input = "torino-asti".to_string();

        let action = app.submit_input();

        #[cfg(debug_assertions)]
        {
            assert_eq!(action, AppAction::None);
            assert_eq!(app.input_mode, InputMode::SpeedSelection);
        }
        #[cfg(not(debug_assertions))]
        assert_eq!(
            action,
            AppAction::StartTrip {
                route: RouteOption {
                    name: "Torino-Asti".to_string(),
                    path: PathBuf::from("Torino-Asti.csv"),
                },
                speed_factor: 1,
            }
        );
    }

    #[test]
    fn invalid_route_keeps_selection_active() {
        let mut app = connected_app();
        app.input_mode = InputMode::RouteSelection;
        app.available_routes = vec![RouteOption {
            name: "Torino-Asti".to_string(),
            path: PathBuf::from("Torino-Asti.csv"),
        }];
        app.input = "99".to_string();

        assert_eq!(app.submit_input(), AppAction::None);
        assert_eq!(app.input_mode, InputMode::RouteSelection);
    }

    #[test]
    fn start_is_rejected_while_trip_is_running() {
        let mut app = connected_app();
        app.trip_status = TripStatus::Running {
            route_name: "Torino-Asti".to_string(),
            speed_factor: 1,
            sent_points: 0,
        };
        app.input = "start".to_string();

        assert_eq!(app.submit_input(), AppAction::None);
        assert!(matches!(app.trip_status, TripStatus::Running { .. }));
    }
}
