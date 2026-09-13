use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::app::{InputMode, MessageKind, ServerApp};

pub(super) fn draw_dashboard(frame: &mut Frame, state: &mut ServerApp) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(4), // command
            Constraint::Min(0),    // main content
            Constraint::Length(3), // footer
        ])
        .split(frame.size());

    draw_header(frame, main_chunks[0]);
    draw_commands(frame, main_chunks[1]);

    let center_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(35),
            Constraint::Percentage(15),
        ])
        .split(main_chunks[2]);

    draw_console(frame, state, center_chunks[0]);
    draw_messages(frame, state, center_chunks[1]);
    draw_users(frame, state, center_chunks[2]);
    draw_input(frame, state, main_chunks[3]);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let header_spans = Span::styled(
        " GeoRust Admin Dashboard ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let line = Line::from(header_spans);

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Cyan)),
        area,
    );
}

fn draw_commands(frame: &mut Frame, area: Rect) {
    let commands = Line::from(vec![
        Span::styled("   help", command_style()),
        Span::raw(": guida   "),
        Span::styled("users", command_style()),
        Span::raw(": utenti registrati   "),
        Span::styled("logs <username>", command_style()),
        Span::raw(": ultimi 10 messaggi dell'utente   "),
        Span::styled("stats <username>", command_style()),
        Span::raw(": analisi viaggi   "),
        Span::styled("msg", command_style()),
        Span::raw(": invia messaggi singoli o broadcast   "),
        Span::styled("exit", command_style()),
        Span::raw(": chiudi il server"),
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

fn draw_console(frame: &mut Frame, state: &mut ServerApp, area: Rect) {
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

fn draw_messages(frame: &mut Frame, state: &mut ServerApp, area: Rect) {
    let mut lines = Vec::new();
    if state.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Nessun messaggio",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for message in &state.messages {
            let (label, color) = match message.kind {
                MessageKind::Direct => (
                    format!(
                        "A: {}",
                        message.target_username.as_deref().unwrap_or("Sconosciuto")
                    ),
                    Color::Green,
                ),
                MessageKind::Broadcast => ("Broadcast".to_string(), Color::Magenta),
                MessageKind::Received => (
                    format!(
                        "Da: {}",
                        message.target_username.as_deref().unwrap_or("Sconosciuto")
                    ),
                    Color::Yellow,
                ),
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

fn draw_users(frame: &mut Frame, state: &ServerApp, area: Rect) {
    let mut lines = Vec::new();

    if state.active_users.is_empty() {
        lines.push(Line::from(Span::styled(
            "Nessuno",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (id, username) in &state.active_users {
            lines.push(Line::from(vec![
                Span::styled("🟢 ", Style::default().fg(Color::Green)),
                Span::raw(format!("#{id} - {username}")).style(
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    let panel = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" Online users ({}) ", state.active_users.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green)),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(panel, area);
}

fn draw_input(frame: &mut Frame, state: &ServerApp, area: Rect) {
    let title = match state.input_mode {
        InputMode::Command => " Comando ",
        InputMode::MessageSelection => " Messaggio [1] diretto, [2] broadcast ",
        InputMode::TargetSelection => " Destinatario ",
        InputMode::MessageWriting => " Testo ",
        InputMode::StatsTypeSelection => {
            " Tipo Interrogazione [1] Tragitto, [2] Velocità, [3] Durate, [4] Tutto "
        }
        InputMode::TemporalSelection => " Finestra temporale [ day | week | month ] ",
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
