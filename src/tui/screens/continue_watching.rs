use crate::tui::{
    continue_watching::WatchEntry,
    state::AppState,
    theme::Theme,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};

pub fn draw(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], state, theme);
    draw_list(frame, chunks[1], state, theme);
    draw_footer(frame, chunks[2], state, theme);
}

fn draw_header(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let count = state.continue_items.len();
    let title = if count == 0 {
        " Continue Watching — nothing in progress ".to_string()
    } else {
        format!(
            " Continue Watching — {count} title{} ",
            if count == 1 { "" } else { "s" }
        )
    };

    let block = Block::default()
        .title(title)
        .title_alignment(Alignment::Center)
        .title_style(theme.title)
        .borders(Borders::ALL)
        .border_type(if state.basic_terminal {
            BorderType::Plain
        } else {
            BorderType::Rounded
        })
        .border_style(theme.border_focus);

    let hint = Line::from(vec![
        Span::styled("Resume where you left off", theme.text_dim),
        Span::styled("  ·  ", theme.muted),
        Span::styled("Enter plays at the saved minute", theme.sapphire),
    ]);

    frame.render_widget(
        Paragraph::new(hint).block(block).alignment(Alignment::Left),
        area,
    );
}

fn draw_list(frame: &mut Frame, area: Rect, state: &mut AppState, theme: &Theme) {
    let border = Block::default()
        .borders(Borders::ALL)
        .border_type(if state.basic_terminal {
            BorderType::Plain
        } else {
            BorderType::Rounded
        })
        .border_style(theme.border)
        .title(" In progress ")
        .title_style(theme.header);

    if state.continue_items.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Nothing in progress yet.",
                theme.text,
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Play a movie or episode — when you quit the player,", theme.text_dim),
            ]),
            Line::from(vec![
                Span::styled("it lands here so you can resume at the exact second.", theme.text_dim),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", theme.text_dim),
                Span::styled("Esc", theme.shortcut),
                Span::styled(" or ", theme.text_dim),
                Span::styled("Ctrl+W", theme.shortcut),
                Span::styled(" to go back.", theme.text_dim),
            ]),
        ])
        .block(border)
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = state
        .continue_items
        .iter()
        .map(|item| continue_row(item, theme))
        .collect();

    if let Some(sel) = state.continue_list_state.selected() {
        if sel >= state.continue_items.len() {
            state
                .continue_list_state
                .select(Some(state.continue_items.len().saturating_sub(1)));
        }
    } else if !state.continue_items.is_empty() {
        state.continue_list_state.select(Some(0));
    }

    let widths = [
        Constraint::Length(8),
        Constraint::Min(18),
        Constraint::Length(12),
        Constraint::Length(18),
        Constraint::Length(10),
    ];

    let header = Row::new(vec![
        Cell::from(Span::styled("Type", theme.header)),
        Cell::from(Span::styled("Title", theme.header)),
        Cell::from(Span::styled("Episode", theme.header)),
        Cell::from(Span::styled("Resume at", theme.header)),
        Cell::from(Span::styled("When", theme.header)),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .block(border)
        .row_highlight_style(
            Style::default()
                .fg(theme.highlight.fg.unwrap_or(ratatui::style::Color::Cyan))
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .highlight_symbol(if state.basic_terminal { "> " } else { "❯ " });

    frame.render_stateful_widget(table, area, &mut state.continue_list_state);
}

fn continue_row<'a>(item: &'a WatchEntry, theme: &'a Theme) -> Row<'a> {
    let kind_style = match item.kind {
        crate::tui::continue_watching::WatchKind::Movie => theme.teal,
        crate::tui::continue_watching::WatchKind::Series => theme.lavender,
        crate::tui::continue_watching::WatchKind::Local => theme.sapphire,
        crate::tui::continue_watching::WatchKind::LiveTv => theme.flamingo,
    };
    Row::new(vec![
        Cell::from(Span::styled(item.kind_label(), kind_style)),
        Cell::from(Span::styled(truncate(&item.title, 36), theme.text)),
        Cell::from(Span::styled(
            if item.detail.is_empty() {
                "—".into()
            } else {
                truncate(&item.detail, 12)
            },
            theme.subtext1,
        )),
        Cell::from(Span::styled(item.progress_label(), theme.accent)),
        Cell::from(Span::styled(relative_when(item.last_watched_unix), theme.text_dim)),
    ])
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let selected = state
        .continue_list_state
        .selected()
        .and_then(|i| state.continue_items.get(i));

    let resume_hint = selected
        .map(|item| {
            format!(
                "Resume \"{}\" at {}",
                item.display_line(),
                item.position_label()
            )
        })
        .unwrap_or_default();

    let hints = Line::from(vec![
        Span::styled("[↑↓]", theme.shortcut),
        Span::styled(" Move  ", theme.text_dim),
        Span::styled("[Enter]", theme.shortcut),
        Span::styled(" Resume  ", theme.text_dim),
        Span::styled("[d]", theme.shortcut),
        Span::styled(" Remove  ", theme.text_dim),
        Span::styled("[Esc]", theme.shortcut),
        Span::styled(" Back  ", theme.text_dim),
        Span::styled("[?]", theme.shortcut),
        Span::styled(" Help", theme.text_dim),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if state.basic_terminal {
            BorderType::Plain
        } else {
            BorderType::Rounded
        })
        .border_style(theme.border);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    frame.render_widget(Paragraph::new(hints).alignment(Alignment::Center), rows[0]);
    if !resume_hint.is_empty() && rows[1].height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                crate::tui::text::truncate_width(&resume_hint, rows[1].width as usize),
                theme.muted,
            )))
            .alignment(Alignment::Center),
            rows[1],
        );
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn relative_when(unix: u64) -> String {
    let now = crate::tui::continue_watching::now_unix();
    let Some(delta) = now.checked_sub(unix) else {
        return "now".into();
    };
    if delta < 60 {
        "just now".into()
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else if delta < 86400 * 7 {
        format!("{}d ago", delta / 86400)
    } else {
        format!("{}w ago", delta / (86400 * 7))
    }
}
