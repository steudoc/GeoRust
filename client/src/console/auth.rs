use common::{LoginRequest, LoginResponse, RegisterRequest};
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;

use super::{runtime::TerminalSession, ui};

const SERVER_HTTP: &str = "http://127.0.0.1:3000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthAction {
    Login,
    Register,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthStep {
    ChooseAction,
    Username,
    Password,
    Submitting,
}

pub(super) struct AuthState {
    pub step: AuthStep,
    pub action: Option<AuthAction>,
    pub username: String,
    pub input: String,
    pub feedback: String,
}

impl AuthState {
    fn new() -> Self {
        Self {
            step: AuthStep::ChooseAction,
            action: None,
            username: String::new(),
            input: String::new(),
            feedback: "Scegli login oppure registrazione.".to_string(),
        }
    }

    pub fn prompt(&self) -> &'static str {
        match self.step {
            AuthStep::ChooseAction => "Login [l] o Registrazione [r]",
            AuthStep::Username => "Username",
            AuthStep::Password => "Password",
            AuthStep::Submitting => "Attendere",
        }
    }

    pub fn displayed_input(&self) -> String {
        if matches!(self.step, AuthStep::Password | AuthStep::Submitting) {
            "•".repeat(self.input.chars().count())
        } else {
            self.input.clone()
        }
    }

    fn reset_after_error(&mut self, message: String) {
        self.step = AuthStep::ChooseAction;
        self.action = None;
        self.username.clear();
        self.input.clear();
        self.feedback = message;
    }
}

pub(super) struct AuthenticatedUser {
    pub user_id: i64,
    pub username: String,
    pub token: String,
}

pub(super) async fn authenticate(
    terminal: &mut TerminalSession,
    events: &mut EventStream,
    http: &reqwest::Client,
) -> anyhow::Result<Option<AuthenticatedUser>> {
    let mut state = AuthState::new();

    loop {
        terminal
            .terminal_mut()
            .draw(|frame| ui::draw_auth(frame, &state))?;

        let Some(event) = events.next().await else {
            return Ok(None);
        };
        let event = event?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(None);
        }

        match key.code {
            KeyCode::Char(character) if state.step != AuthStep::Submitting => {
                state.input.push(character);
            }
            KeyCode::Backspace if state.step != AuthStep::Submitting => {
                state.input.pop();
            }
            KeyCode::Enter => match state.step {
                AuthStep::ChooseAction => match state.input.trim().to_lowercase().as_str() {
                    "l" | "login" => {
                        state.action = Some(AuthAction::Login);
                        state.step = AuthStep::Username;
                        state.input.clear();
                        state.feedback = "Inserisci il tuo username.".to_string();
                    }
                    "r" | "register" | "registrazione" => {
                        state.action = Some(AuthAction::Register);
                        state.step = AuthStep::Username;
                        state.input.clear();
                        state.feedback = "Scegli lo username da registrare.".to_string();
                    }
                    _ => {
                        state.input.clear();
                        state.feedback =
                            "Scelta non valida: usa 'l' per login oppure 'r' per registrarti."
                                .to_string();
                    }
                },
                AuthStep::Username => {
                    let username = state.input.trim();
                    if username.is_empty() {
                        state.feedback = "Lo username non può essere vuoto.".to_string();
                    } else {
                        state.username = username.to_string();
                        state.input.clear();
                        state.step = AuthStep::Password;
                        state.feedback = "Inserisci la password.".to_string();
                    }
                }
                AuthStep::Password => {
                    let password = state.input.trim().to_string();
                    if password.is_empty() {
                        state.feedback = "La password non può essere vuota.".to_string();
                        continue;
                    }

                    let action = state.action.expect("azione impostata prima della password");
                    state.step = AuthStep::Submitting;
                    state.feedback = match action {
                        AuthAction::Login => "Login in corso...".to_string(),
                        AuthAction::Register => "Registrazione in corso...".to_string(),
                    };
                    terminal
                        .terminal_mut()
                        .draw(|frame| ui::draw_auth(frame, &state))?;

                    match submit_credentials(http, action, &state.username, &password).await {
                        Ok(login) => {
                            return Ok(Some(AuthenticatedUser {
                                user_id: login.user_id,
                                username: state.username.clone(),
                                token: login.token,
                            }));
                        }
                        Err(message) => state.reset_after_error(message),
                    }
                }
                AuthStep::Submitting => {}
            },
            _ => {}
        }
    }
}

async fn submit_credentials(
    http: &reqwest::Client,
    action: AuthAction,
    username: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    if action == AuthAction::Register {
        let response = http
            .post(format!("{SERVER_HTTP}/register"))
            .json(&RegisterRequest {
                username: username.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .map_err(|error| {
                format!("Server non raggiungibile durante la registrazione: {error}")
            })?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::CONFLICT {
            return Err(format!(
                "Registrazione fallita: HTTP {}.",
                response.status()
            ));
        }
    }

    let response = http
        .post(format!("{SERVER_HTTP}/login"))
        .json(&LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        })
        .send()
        .await
        .map_err(|error| format!("Server non raggiungibile durante il login: {error}"))?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Credenziali non valide. Riprova.".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("Login fallito: HTTP {}.", response.status()));
    }

    response
        .json::<LoginResponse>()
        .await
        .map_err(|error| format!("Risposta di login non valida: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{AuthState, AuthStep};

    #[test]
    fn password_remains_masked_while_request_is_in_flight() {
        let mut state = AuthState::new();
        state.step = AuthStep::Submitting;
        state.input = "segreta".to_string();

        assert_eq!(state.displayed_input(), "•••••••");
    }
}
