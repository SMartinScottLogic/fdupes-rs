use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    error::Error,
    fmt::Debug,
    io::{self, Stdout},
    process::Stdio,
    sync::{Arc, Mutex, mpsc::TryRecvError},
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget};
use std::sync::mpsc::Receiver;

use crate::{Config, DupeMessage};

use super::{DupeGroup, DupeGroupReceiver};

pub struct UIReceiver {
    rx: Receiver<DupeMessage>,
    config: Config,

    contents: Vec<String>,
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl DupeGroupReceiver for UIReceiver {
    fn run(&mut self) -> Result<(), io::Error> {
        let mut start_tick = Instant::now();
        let max_duration = Duration::from_millis(5000);

        let mut last_tick = Instant::now();
        let tick_rate = Duration::from_millis(250);

        loop {
            match self.rx.try_recv() {
                Ok((size, filenames)) => {
                    log::info!("recv {} {:?}", size, filenames);
                    //Self::handle_group(size, filenames, &self.config);
                }
                Err(TryRecvError::Empty) => log::trace!("empty"),
                Err(TryRecvError::Disconnected) => break,
            }

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
                            self.contents.last_mut().and_then(|s| Some(s.push(c)));
                        }
                    }
                    Event::Mouse(event) => {
                        log::info!("mouse: {:?}", event);
                    }
                    Event::Resize(x, y) => {
                        log::info!("resize: {} {}", x, y);
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
                        .output_target(true)
                        .output_file(true)
                        .output_line(true)
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

impl UIReceiver {
    pub fn new(rx: Receiver<DupeMessage>, config: Config) -> Self {
        enable_raw_mode().unwrap();
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).unwrap();

        Self { rx, config, contents: Vec::new(), terminal }
    }
}

impl Drop for UIReceiver {
    fn drop(&mut self) {
        // restore terminal
        disable_raw_mode().unwrap();
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .unwrap();
        self.terminal.show_cursor();
    }
}

