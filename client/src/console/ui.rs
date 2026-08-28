use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::{
    app::{ClientApp, ConnectionStatus, InputMode, MessageKind, TripStatus},
    auth::{AuthState, AuthStep},
};

pub(super) fn draw_auth(frame: &mut Frame, state: &AuthState) {
    let area = frame.size();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(" GeoRust Client ")
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(header, chunks[0]);

    let card = centered_rect(70, 60, chunks[1]);
    frame.render_widget(Clear, card);

    let action = match state.action {
        Some(super::auth::AuthAction::Login) => "Login",
        Some(super::auth::AuthAction::Register) => "Registrazione",
        None => "Non selezionata",
    };
    let password_hint = if state.step == AuthStep::Password {
        "La password viene nascosta durante la digitazione."
    } else {
        "Dopo la registrazione verrà effettuato automaticamente il login."
    };
    let content = vec![
        Line::from(Span::styled(
            "Autenticazione richiesta",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Per continuare devi effettuare il login oppure registrarti."),
        Line::from("Digita 'l' / 'login' oppure 'r' / 'register'."),
        Line::from(""),
        Line::from(format!("Operazione: {action}")),
        Line::from(format!(
            "Username: {}",
            if state.username.is_empty() {
                "-"
            } else {
                &state.username
            }
        )),
        Line::from(""),
        Line::from(Span::styled(
            state.feedback.clone(),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            password_hint,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let panel = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Accesso ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, card);

    let displayed_input = state.displayed_input();
    let input = Paragraph::new(format!("> {displayed_input}"))
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .title(format!(" {} ", state.prompt()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
    frame.render_widget(input, chunks[2]);

    if state.step != AuthStep::Submitting {
        set_input_cursor(frame, chunks[2], &displayed_input);
    }
}

pub(super) fn draw_dashboard(frame: &mut Frame, state: &mut ClientApp) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.size());

    draw_header(frame, state, main_chunks[0]);
    draw_commands(frame, main_chunks[1]);

    let center_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[2]);

    draw_console(frame, state, center_chunks[0]);
    draw_messages(frame, state, center_chunks[1]);
    draw_input(frame, state, main_chunks[3]);
}

fn draw_header(frame: &mut Frame, state: &ClientApp, area: Rect) {
    let connection = match state.connection_status {
        ConnectionStatus::Connected => Span::styled(
            "CONNESSO",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        ConnectionStatus::Reconnecting => Span::styled(
            "RICONNESSIONE",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let mut header_spans = vec![
        Span::styled(
            " GeoRust User Console ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} (#{}): ", state.username, state.user_id),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        connection,
    ];

    let trip = match &state.trip_status {
        TripStatus::Idle => None,
        TripStatus::Running { route_name, .. } => Some(format!("Trip: {route_name}")),
        TripStatus::AwaitingCompletion { route_name } => {
            Some(format!("Completamento: {route_name}"))
        }
        TripStatus::WaitingReconnect => Some("Trip completato".to_string()),
    };
    if let Some(trip) = trip {
        header_spans.push(Span::styled(
            format!(" | {trip} "),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ));
    }

    let line = Line::from(header_spans);

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Cyan)),
        area,
    );
}

fn draw_commands(frame: &mut Frame, area: Rect) {
    let commands = Line::from(vec![
        Span::styled("help", command_style()),
        Span::raw(": guida   "),
        Span::styled("msg <testo>", command_style()),
        Span::raw(": messaggio al server   "),
        Span::styled("start", command_style()),
        Span::raw(": nuovo trip   "),
        Span::styled("exit", command_style()),
        Span::raw(": chiudi il client"),
    ]);

    let panel = Paragraph::new(commands)
        .block(
            Block::default()
                .title(" Comandi disponibili ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn draw_console(frame: &mut Frame, state: &mut ClientApp, area: Rect) {
    let line_count = state.console_lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);
    let max_offset = line_count.saturating_sub(visible_height);
    state.console_scroll = state.console_scroll.min(max_offset);
    let actual_scroll = max_offset.saturating_sub(state.console_scroll);

    let text = state.console_lines.join("\n");
    let panel = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Console ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .scroll((actual_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn draw_messages(frame: &mut Frame, state: &mut ClientApp, area: Rect) {
    let mut lines = Vec::new();
    if state.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Nessun messaggio",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for message in &state.messages {
            let (label, color) = match message.kind {
                MessageKind::Direct => ("Diretto", Color::Green),
                MessageKind::Broadcast => ("Broadcast", Color::Magenta),
                MessageKind::Sent => ("Tu → Server", Color::Yellow),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] [{label}] ", message.timestamp),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(message.text.clone()),
            ]));
        }
    }

    let line_count = lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);
    let max_offset = line_count.saturating_sub(visible_height);
    state.messages_scroll = state.messages_scroll.min(max_offset);
    let actual_scroll = max_offset.saturating_sub(state.messages_scroll);

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Messaggi ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        )
        .scroll((actual_scroll, 0))
        .wrap(Wrap { trim: true });
    frame.render_widget(panel, area);
}

fn draw_input(frame: &mut Frame, state: &ClientApp, area: Rect) {
    let title = match state.input_mode {
        InputMode::Command => " Comando ",
        InputMode::RouteSelection => " Seleziona percorso ",
        #[cfg(debug_assertions)]
        InputMode::SpeedSelection => " Fattore di velocità ",
    };
    let panel = Paragraph::new(format!("> {}", state.input))
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
    frame.render_widget(panel, area);
    set_input_cursor(frame, area, &state.input);
}

fn set_input_cursor(frame: &mut Frame, area: Rect, input: &str) {
    let desired_x = area
        .x
        .saturating_add(3)
        .saturating_add(u16::try_from(input.chars().count()).unwrap_or(u16::MAX));
    let max_x = area.right().saturating_sub(2);
    frame.set_cursor(desired_x.min(max_x), area.y.saturating_add(1));
}

fn command_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
