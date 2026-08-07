use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::time::{Duration, Instant};

use crate::engine::roll_parser::Script;

pub struct TuiApp {
    pub scripts: Vec<Script>,
    pub selected_index: usize,
    pub preview_mode: PreviewMode,
    pub last_update: Instant,
    pub fuzzy_query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Text,
    Image,
    Video,
    None,
}

impl TuiApp {
    pub fn new() -> Result<Self> {
        let mut app = Self {
            scripts: Vec::new(),
            selected_index: 0,
            preview_mode: PreviewMode::None,
            last_update: Instant::now(),
            fuzzy_query: String::new(),
        };
        app.load_scripts()?;
        Ok(app)
    }

    fn load_scripts(&mut self) -> Result<()> {
        // Load .roll scripts from the current directory
        let mut scripts = Vec::new();
        for entry in std::fs::read_dir(".")? {
            let entry = entry?;
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "roll" {
                    if let Ok(script) = crate::engine::roll_parser::parse_roll_script(&path) {
                        scripts.push(script);
                    }
                }
            }
        }
        self.scripts = scripts;
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => {
                // Exit the application
            }
            KeyCode::Char('/') => {
                // Start fuzzy search
            }
            KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected_index < self.scripts.len().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            KeyCode::Enter => {
                // Open preview
                self.preview_mode = PreviewMode::Image;
            }
            _ => {}
        }
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_update) > Duration::from_millis(100) {
            self.last_update = now;
            // Update fuzzy search results
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        
        // Create a layout with two panels
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);
        
        // Left panel: script list
        let script_list = self.scripts.iter().enumerate().map(|(i, script)| {
            let style = if i == self.selected_index {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            
            ListItem::new(Line::from(format!("{}. {}", i + 1, script.title.as_deref().unwrap_or("Untitled")))
                .style(style))
        }).collect::<Vec<_>>();
        
        frame.render_widget(
            List::new(script_list)
                .block(Block::default().title("Scripts").borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            layout[0],
        );
        
        // Right panel: preview
        let preview_area = layout[1];
        match self.preview_mode {
            PreviewMode::Image => {
                // Display image preview
                // This would use ratatui-image or similar
                frame.render_widget(
                    Paragraph::new("Image Preview")
                        .block(Block::default().title("Preview").borders(Borders::ALL))
                        .alignment(Alignment::Center),
                    preview_area,
                );
            }
            PreviewMode::Text => {
                // Display script text
                if let Some(script) = self.scripts.get(self.selected_index) {
                    let text = format!("{:#?}", script);
                    frame.render_widget(
                        Paragraph::new(text)
                            .block(Block::default().title("Script Details").borders(Borders::ALL))
                            .wrap(Wrap { hardware: true }),
                        preview_area,
                    );
                }
            }
            _ => {
                frame.render_widget(
                    Paragraph::new("Select a script and press Enter to preview")
                        .block(Block::default().title("Help").borders(Borders::ALL))
                        .alignment(Alignment::Center),
                    preview_area,
                );
            }
        }
    }
}