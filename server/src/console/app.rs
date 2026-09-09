
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    Command,
    MessageSelection,   // scelta diretto (1) o broadcast (2)
    TargetSelection,    // solo per msg diretti
    MessageWriting,
    StatsSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageKind {
    Direct,
    Broadcast,
    Received,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageEntry {
    pub kind: MessageKind,
    pub timestamp: String,
    pub target_username: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AppAction {
    None,
    SendDirect {
        username: String,
        text: String,
    },
    SendBroadcast {
        text: String
    },
    CalculateStats {
        username: String,
        interval: String,
    },
    SeeUsers,
    SeeLogs {
        username: String
    },
    Exit,
}

pub(super) struct ServerApp {
    pub input: String,
    pub console_lines: Vec<String>,
    pub messages: Vec<MessageEntry>,
    pub input_mode: InputMode,
    pub pending_username: Option<String>,
    pub pending_msg_kind: Option<MessageKind>,
    pub console_scroll: u16,
    pub messages_scroll: u16,
    pub active_users: Vec<(i64, String)>,
}

impl ServerApp {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            console_lines: vec![
                "Avvio del server in corso...".to_string(),
            ],
            messages: Vec::new(),
            input_mode: InputMode::Command,
            pending_username: None,
            pending_msg_kind: None,
            console_scroll: 0,
            messages_scroll: 0,
            active_users: Vec::new(),
        }
    }

    pub fn submit_input(&mut self) -> AppAction {
        self.console_scroll = 0;
        let input = std::mem::take(&mut self.input);
        let value = input.trim();

        if value.is_empty() {
            return AppAction::None;
        }

        self.push_console(format!("> {value}"));

        match self.input_mode {
            InputMode::Command => self.process_command(value),
            InputMode::MessageSelection => self.process_msg_selection(value),
            InputMode::TargetSelection => self.process_target_selection(value),
            InputMode::MessageWriting => self.process_msg_writing(value),
            InputMode::StatsSelection => self.process_stats_selection(value),
        }
    }

    fn process_command(&mut self, value: &str) -> AppAction {
        let mut parts = value.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or_default().to_lowercase();
        let argument = parts.next().unwrap_or_default().trim();

        match command.as_str() {
            "help" => {
                self.push_console("Comandi disponibili:");
                self.push_console("-----------------------------------------------------------");
                self.push_console("  users              Utenti registrati");
                self.push_console("  logs <username>    Lista messaggi utente");
                self.push_console("  stats <username>   Analisi statistiche di viaggio");
                self.push_console("  msg                Invia messaggio (diretto / broadcast)");
                self.push_console("  exit               Chiude il server");
                self.push_console("-----------------------------------------------------------");
                AppAction::None
            },
            "users" => AppAction::SeeUsers,
            "logs" => {
                let username = argument.to_string();
                AppAction::SeeLogs { username }
            },
            "stats" => {
                if !argument.is_empty() {
                    let username = argument.to_string();
                    self.pending_username = Some(username);
                    self.input_mode = InputMode::StatsSelection;
                    self.push_console("Seleziona l'intervallo temporale:");
                    self.push_console("  day | week | month ");
                    AppAction::None
                } else {
                    self.push_console("Errore: specifica un username valido");
                    AppAction::None
                }
            }
            "msg" => {
                self.input_mode = InputMode::MessageSelection;
                self.push_console("Scegli il tipo di messaggio:");
                self.push_console("  1. Diretto a un utente");
                self.push_console("  2. Broadcast (a tutti)");
                AppAction::None
            },
            "exit" => AppAction::Exit,
            _ => {
                self.push_console(format!("Comando sconosciuto: {command}. Digita 'help'."));
                AppAction::None
            }
        }
    }

    fn process_msg_selection(&mut self, value: &str) -> AppAction {
        if value.eq_ignore_ascii_case("exit") {
            return AppAction::Exit;
        }

        match value {
            "1" => {
                self.pending_msg_kind = Some(MessageKind::Direct);
                self.input_mode = InputMode::TargetSelection;
                self.push_console("Inserisci l'username dell'utente destinatario:");
            }
            "2" => {
                self.pending_msg_kind = Some(MessageKind::Broadcast);
                self.input_mode = InputMode::MessageWriting;
                self.push_console("Inserisci il testo del messaggio broadcast:");
            }
            _ => {
                self.push_console("Scelta non valida. Digita 1 o 2 (o 'annulla').");
            }
        }
        AppAction::None
    }

    fn process_target_selection(&mut self, value: &str) -> AppAction {
        if !value.is_empty() {
            let username = value.to_string();
            self.pending_username = Some(username);
            self.input_mode = InputMode::MessageWriting;
            self.push_console(format!("Testo del messaggio per l'utente {value}:"));
        } else {
            self.push_console("Errore: username non valido. Riprova.");
        }
        AppAction::None
    }

    fn process_msg_writing(&mut self, value: &str) -> AppAction {
        self.input_mode = InputMode::Command; // Finito, si torna al menu
        
        let text = value.to_string();
        let kind = self.pending_msg_kind.take();
        let username = self.pending_username.take();

        match (kind, username.clone()) {
            (Some(MessageKind::Direct), Some(username)) => AppAction::SendDirect { username, text },
            (Some(MessageKind::Broadcast), _) => AppAction::SendBroadcast { text },
            _ => {
                self.push_console("Errore interno di stato. Messaggio annullato.");
                AppAction::None
            }
        }
    }
    
    fn process_stats_selection(&mut self, value: &str) -> AppAction {
        let interval = value.to_lowercase();
        if ["day", "week", "month"].contains(&interval.as_str()) {
            self.input_mode = InputMode::Command; // Torna al menu
            let username = self.pending_username.take().unwrap_or("None".to_string());
            
            AppAction::CalculateStats { username, interval }
        } else {
            self.push_console("Valore non valido. Digita 'day', 'week' o 'month'.");
            AppAction::None
        }
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