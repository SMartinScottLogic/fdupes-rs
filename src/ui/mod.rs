use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget};

pub struct UI {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    contents: Vec<String>,
}

impl Drop for UI {
    fn drop(&mut self) {
        // restore terminal
        disable_raw_mode().unwrap();
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
        self.terminal.show_cursor().unwrap();
    }
}

impl Default for UI {
    fn default() -> Self {
        Self::new()
    }
}

impl UI {
    pub fn new() -> Self {
        enable_raw_mode().unwrap();
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).unwrap();

        Self {
            terminal,
            contents: vec![String::new()],
        }
    }

    pub fn end(&mut self) {
        if let Some(s) = self.contents.last_mut() { 
            s.push('#')
        }
        log::info!("UI::END");
    }

    pub fn test_tui(&mut self) -> Result<(), io::Error> {
        // setup terminal

        let mut last_tick = Instant::now();
        let tick_rate = Duration::from_millis(250);

        loop {
            let normalize_case = |mut key: crossterm::event::KeyEvent| {
                let c = match key.code {
                    KeyCode::Char(c) => c,
                    _ => return key,
                };

                if c.is_ascii_uppercase() {
                    key.modifiers.insert(crossterm::event::KeyModifiers::SHIFT);
                } else if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::SHIFT)
                {
                    key.code = KeyCode::Char(c.to_ascii_uppercase())
                }
                key
            };

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if crossterm::event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.code == KeyCode::Esc => break,
                    Event::Key(key) if key.code == KeyCode::Enter => {
                        self.contents.push(String::new())
                    }
                    Event::Key(key) => {
                        log::info!("key: {:?}", key.code);
                        let key = normalize_case(key);
                        if let KeyCode::Char(c) = key.code {
                            if let Some(s) = self.contents.last_mut() { 
                                s.push(c)
                            }
                        }
                    }
                    Event::Mouse(event) => {
                        log::info!("mouse: {event:?}");
                    }
                    Event::Resize(x, y) => {
                        log::info!("resize: {x} {y}");
                    }
                    Event::FocusGained => {
                        log::info!("focus gained");
                    }
                    Event::FocusLost => {
                        log::info!("focus gained");
                    }
                    Event::Paste(p) => {
                        log::info!("paste {p}");
                    }
                }
            }
            if last_tick.elapsed() >= tick_rate {
                //app.on_tick();

                let text = self
                    .contents
                    .iter()
                    .map(|s| Spans::from(s.clone()))
                    .collect::<Vec<_>>();

                self.terminal.draw(|f| {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .margin(1)
                        .constraints([Constraint::Min(0), Constraint::Length(10)].as_ref())
                        .split(f.size());

                    let create_block = |title| {
                        Block::default()
                            .borders(Borders::ALL)
                            .style(Style::default().bg(Color::Black).fg(Color::Gray))
                            .title(Span::styled(
                                title,
                                Style::default().add_modifier(Modifier::BOLD),
                            ))
                    };

                    let text = Paragraph::new(text)
                        .block(create_block("Block"))
                        .wrap(Wrap { trim: false });

                    f.render_widget(text, chunks[0]);

                    let tui_w: TuiLoggerWidget = TuiLoggerWidget::default()
                        .block(create_block("Log"))
                        .output_separator('|')
                        .output_timestamp(Some("%F %H:%M:%S%.3f".to_string()))
                        .output_level(Some(TuiLoggerLevelOutput::Long))
                        .output_target(false)
                        .output_file(false)
                        .output_line(false)
                        .style(Style::default().fg(Color::White).bg(Color::Black));
                    f.render_widget(tui_w, chunks[1]);
                })?;
                last_tick = Instant::now();
            }
            /*
                        if start_tick.elapsed() > max_duration {
                            break;
                        }
            */
        }
        Ok(())
    }
}
