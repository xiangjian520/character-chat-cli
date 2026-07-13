use std::io;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

pub struct QqConfig {
    pub app_id: String,
    pub app_secret: String,
}

#[derive(PartialEq)]
enum Focus {
    AppId,
    AppSecret,
    Confirm,
}

pub fn run_config_tui(current_app_id: &str, current_app_secret: &str) -> Option<QqConfig> {
    enable_raw_mode().ok()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).ok()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).ok()?;

    let mut app_id = current_app_id.to_string();
    let mut app_secret = current_app_secret.to_string();
    let mut focus = Focus::AppId;
    let mut status = String::new();

    let res = run_loop(
        &mut terminal,
        &mut app_id,
        &mut app_secret,
        &mut focus,
        &mut status,
    );

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    let _ = terminal.show_cursor();

    res
}

fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app_id: &mut String,
    app_secret: &mut String,
    focus: &mut Focus,
    status: &mut String,
) -> Option<QqConfig> {
    loop {
        let _ = terminal.draw(|f| render(f, app_id, app_secret, focus, status));

        if let Ok(true) = event::poll(std::time::Duration::from_millis(100)) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => return None,
                        KeyCode::Tab => next_focus(focus, true),
                        KeyCode::BackTab => next_focus(focus, false),
                        KeyCode::Up => next_focus(focus, false),
                        KeyCode::Down => next_focus(focus, true),
                        KeyCode::Enter => {
                            if *focus == Focus::Confirm {
                                if app_id.trim().is_empty() || app_secret.trim().is_empty() {
                                    *status = "App ID 和 App Secret 不能为空".to_string();
                                } else {
                                    return Some(QqConfig {
                                        app_id: app_id.trim().to_string(),
                                        app_secret: app_secret.trim().to_string(),
                                    });
                                }
                            } else {
                                next_focus(focus, true);
                            }
                        }
                        KeyCode::Char(c) => match focus {
                            Focus::AppId => app_id.push(c),
                            Focus::AppSecret => app_secret.push(c),
                            Focus::Confirm => {}
                        },
                        KeyCode::Backspace => match focus {
                            Focus::AppId => {
                                app_id.pop();
                            }
                            Focus::AppSecret => {
                                app_secret.pop();
                            }
                            Focus::Confirm => {}
                        },
                        _ => {}
                    }
                    status.clear();
                }
            }
        }
    }
}

fn next_focus(focus: &mut Focus, forward: bool) {
    *focus = match (&focus, forward) {
        (Focus::AppId, true) => Focus::AppSecret,
        (Focus::AppSecret, true) => Focus::Confirm,
        (Focus::Confirm, true) => Focus::AppId,
        (Focus::AppId, false) => Focus::Confirm,
        (Focus::AppSecret, false) => Focus::AppId,
        (Focus::Confirm, false) => Focus::AppSecret,
    };
}

fn render(f: &mut Frame, app_id: &str, app_secret: &str, focus: &Focus, status: &str) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(center_rect(area, 62, 14));

    let title = Paragraph::new(Line::from(Span::styled(
        " QQ 机器人登录配置 ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(title, chunks[0]);

    let active_style = Style::default().fg(Color::Yellow);
    let dim_style = Style::default().fg(Color::White);
    let label_style = Style::default().fg(Color::Gray);

    let app_id_style = if *focus == Focus::AppId {
        active_style
    } else {
        dim_style
    };
    let app_id_display = if *focus == Focus::AppId {
        format!("{}_", app_id)
    } else if app_id.is_empty() {
        "<未设置>".to_string()
    } else {
        app_id.to_string()
    };
    let app_id_widget = Paragraph::new(Line::from(vec![
        Span::styled(" App ID:     ", label_style),
        Span::styled(app_id_display, app_id_style),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(app_id_widget, chunks[1]);

    let secret_style = if *focus == Focus::AppSecret {
        active_style
    } else {
        dim_style
    };
    let secret_display: String = if *focus == Focus::AppSecret {
        format!("{}_", app_secret)
    } else if app_secret.is_empty() {
        "<未设置>".to_string()
    } else {
        app_secret.chars().next().map(|c| c.to_string()).unwrap_or_default()
    };
    let secret_widget = Paragraph::new(Line::from(vec![
        Span::styled(" App Secret: ", label_style),
        Span::styled(secret_display, secret_style),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(secret_widget, chunks[2]);

    let confirm_style = if *focus == Focus::Confirm {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let confirm = Paragraph::new(Line::from(Span::styled(" [ 确认登录 ] ", confirm_style)))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(confirm, chunks[3]);

    let hint_text = if status.is_empty() {
        "Tab 切换字段 · Enter 确认 · Esc 取消"
    } else {
        status
    };
    let hint_style = if status.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Red)
    };
    let hint = Paragraph::new(Line::from(Span::styled(hint_text, hint_style)));
    f.render_widget(hint, chunks[4]);
}

fn center_rect(r: Rect, width: u16, height: u16) -> Rect {
    let x = if r.width > width {
        (r.width - width) / 2
    } else {
        0
    };
    let y = if r.height > height {
        (r.height - height) / 2
    } else {
        0
    };
    Rect::new(r.x + x, r.y + y, width.min(r.width), height.min(r.height))
}
