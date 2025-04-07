use color_eyre::Result;
use futures_util::{SinkExt, StreamExt};
use rand::seq::SliceRandom;
use rand::thread_rng;
use ratatui::prelude::Margin;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Position},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use reqwest;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// Message format for WebSocket communication
#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChatMessage {
    content: String,
    #[serde(rename = "authorId")]
    author_id: String,
    timestamp: u64,
}

const WEBSERVER_URL: &str = "http://localhost:3000";

// Add this function to generate fun usernames
fn generate_fun_username() -> String {
    let adjectives = [
        "Skibidi", "Rizz", "Gyatt", "Bussin", "Based", "Cringe", "Sheesh", "Vibing", "Slay",
        "Goated", "Lit", "Yeet", "Swag", "Drip", "Poggers", "Ratio", "Copium", "Hopium", "Mald",
        "Sigma",
    ];

    let nouns = [
        "Wizard",
        "Master",
        "Developer",
        "Titan",
        "Demon",
        "King",
        "Queen",
        "Chad",
        "Gigachad",
        "Npc",
        "Boss",
        "Legend",
        "Goat",
        "Vibe",
        "Mood",
        "Moment",
        "Energy",
        "Rizz",
        "Chamber",
        "Warrior",
    ];

    let mut rng = thread_rng();
    let adjective = adjectives.choose(&mut rng).unwrap();
    let noun = nouns.choose(&mut rng).unwrap();
    let n: u32 = rand::Rng::gen_range(&mut rng, 0..100);

    format!("{}_{}_{:02}", adjective, noun, n).to_lowercase()
}

// In main(), replace the UUID generation with:
fn main() -> Result<()> {
    color_eyre::install()?;

    // Generate a fun user ID for this client
    let user_id = generate_fun_username();
    // Clone it before moving into the async block
    let user_id_for_ws = user_id.clone();

    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;

    // Create channels for communication between UI and WebSocket
    let (ws_tx, mut ws_rx) = mpsc::channel::<String>(100);
    let (msg_tx, msg_rx) = mpsc::channel::<ChatMessage>(100);

    // Shared state for connection status
    let connection_status = Arc::new(Mutex::new(String::from("Connecting...")));
    let connection_status_clone = connection_status.clone();

    // In the main function, update the WebSocket connection part
    // Spawn the WebSocket client task
    rt.spawn(async move {
        // The URL format needs to match what the server expects
        let ws_url = format!(
            "ws://{}/?userId={}",
            WEBSERVER_URL.split("/").last().unwrap_or("localhost:3000"),
            user_id_for_ws
        );

        match connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                let (mut write, mut read) = ws_stream.split();

                // Update connection status
                {
                    let mut status = connection_status_clone.lock().unwrap();
                    *status = String::from("Connected");
                }

                // Handle incoming messages from the WebSocket
                let msg_tx_clone = msg_tx.clone();
                tokio::spawn(async move {
                    while let Some(message) = read.next().await {
                        if let Ok(msg) = message {
                            if let Message::Text(text) = msg {
                                if let Ok(chat_msg) = serde_json::from_str::<ChatMessage>(&text) {
                                    if let Err(_) = msg_tx_clone.send(chat_msg).await {
                                        break;
                                    }
                                }
                            }
                        } else {
                            break;
                        }
                    }
                });

                // Handle outgoing messages to the WebSocket
                while let Some(message) = ws_rx.recv().await {
                    let chat_msg = ChatMessage {
                        content: message.clone(),
                        author_id: user_id_for_ws.clone(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                    };

                    if let Ok(json) = serde_json::to_string(&chat_msg) {
                        if let Err(_) = write.send(Message::Text(json)).await {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let mut status = connection_status_clone.lock().unwrap();
                *status = format!("Failed: {}", e);
            }
        }
    });

    // Initialize the terminal UI
    let terminal = ratatui::init();

    // Create and run the app
    let app = App::new(ws_tx, msg_rx, connection_status, user_id);
    let connected_users = app.connected_users.clone();

    // Spawn a task to periodically update the user count
    rt.spawn(async move {
        loop {
            if let Ok(response) = reqwest::get(format!("{}/users", WEBSERVER_URL)).await {
                if let Ok(data) = response.json::<serde_json::Value>().await {
                    if let Some(count) = data["count"].as_u64() {
                        let mut users = connected_users.lock().unwrap();
                        *users = count as usize;
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });

    let app_result = app.run(terminal, rt);

    ratatui::restore();
    app_result
}

struct App {
    input: String,
    character_index: usize,
    input_mode: InputMode,
    messages: Vec<(String, bool, String)>,
    messages_scroll_state: ScrollbarState,
    messages_scroll: usize,
    ws_tx: mpsc::Sender<String>,
    msg_rx: mpsc::Receiver<ChatMessage>,
    connection_status: Arc<Mutex<String>>,
    user_id: String,
    connected_users: Arc<Mutex<usize>>,
}

enum InputMode {
    Normal,
    Editing,
}

impl App {
    fn new(
        ws_tx: mpsc::Sender<String>,
        msg_rx: mpsc::Receiver<ChatMessage>,
        connection_status: Arc<Mutex<String>>,
        user_id: String,
    ) -> Self {
        Self {
            input: String::new(),
            input_mode: InputMode::Editing,
            messages: Vec::new(),
            character_index: 0,
            messages_scroll_state: ScrollbarState::default(),
            messages_scroll: 0,
            ws_tx,
            msg_rx,
            connection_status,
            user_id,
            connected_users: Arc::new(Mutex::new(1)),
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            let after_char_to_delete = self.input.chars().skip(current_index);
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    fn submit_message(&mut self) {
        if !self.input.trim().is_empty() {
            // Send message to WebSocket
            let message = self.input.clone();
            if let Ok(_) = self.ws_tx.try_send(message) {
                // Clear input after sending
                self.input.clear();
                self.reset_cursor();
            }
        }
    }

    fn append_message(&mut self, message: String, from_user: bool, author_id: String) {
        self.messages.push((message, from_user, author_id));
        self.messages_scroll = self.messages.len().saturating_sub(1);
        self.messages_scroll_state = self.messages_scroll_state.position(self.messages_scroll);
    }

    fn run(mut self, mut terminal: DefaultTerminal, _rt: tokio::runtime::Runtime) -> Result<()> {
        // Remove the EventStream line that's causing the error
        // let mut event_reader = event::EventStream::new();

        loop {
            // Check for new messages from WebSocket
            if let Ok(msg) = self.msg_rx.try_recv() {
                let is_from_user = msg.author_id == self.user_id;
                self.append_message(msg.content, is_from_user, msg.author_id);
            }

            terminal.draw(|frame| self.draw(frame))?;

            // Use poll with a timeout to make the UI responsive without blocking
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match self.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Enter => {
                                self.input_mode = InputMode::Editing;
                            }
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                self.scroll_messages_up();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                self.scroll_messages_down();
                            }
                            _ => {}
                        },
                        InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter => self.submit_message(),
                            KeyCode::Backspace => self.delete_char(),
                            KeyCode::Left => self.move_cursor_left(),
                            KeyCode::Right => self.move_cursor_right(),
                            KeyCode::Up => {
                                self.scroll_messages_up();
                            }
                            KeyCode::Down => {
                                self.scroll_messages_down();
                            }
                            KeyCode::Esc => self.input_mode = InputMode::Normal,
                            KeyCode::Char(to_insert) => self.enter_char(to_insert),
                            _ => {}
                        },
                        InputMode::Editing => {}
                    }
                }
            }
        }
    }

    fn scroll_messages_up(&mut self) {
        if self.messages_scroll > 0 {
            self.messages_scroll -= 1;
            self.messages_scroll_state = self.messages_scroll_state.position(self.messages_scroll);
        }
    }

    fn scroll_messages_down(&mut self) {
        if self.messages_scroll < self.messages.len().saturating_sub(1) {
            self.messages_scroll += 1;
            self.messages_scroll_state = self.messages_scroll_state.position(self.messages_scroll);
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let vertical = Layout::vertical([
            Constraint::Length(5),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [title_area, messages_area, input_area, help_area] = vertical.areas(frame.area());

        let bg_block = Block::default().style(Style::default().bg(Color::Rgb(13, 20, 24)));
        frame.render_widget(bg_block, frame.area());

        // Get connection status
        let status = self.connection_status.lock().unwrap().clone();

        // Create a two-line title with status and user ID on separate lines
        let title_text = vec![
            Line::from(vec![Span::styled(
                "💬 Global Chat 💬",
                Style::default()
                    .fg(Color::Rgb(0, 230, 118))
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::White)),
                Span::styled(&status, Style::default().fg(Color::Rgb(0, 230, 118))),
            ]),
            Line::from(vec![
                Span::styled("Your ID: ", Style::default().fg(Color::White)),
                Span::styled(&self.user_id, Style::default().fg(Color::Rgb(0, 230, 118))),
            ]),
        ];

        let title = Paragraph::new(title_text)
            .style(Style::default().bg(Color::Rgb(17, 27, 33)))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(69, 90, 100))),
            )
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(title, title_area);

        let messages_len = self.messages.len();
        let mut messages_scroll_state = self.messages_scroll_state.content_length(messages_len);

        let visible_height = messages_area.height.saturating_sub(2) as usize;

        // Calculate total lines needed for all messages to determine proper scrolling
        let mut total_lines = 0;
        let messages_with_line_counts: Vec<(usize, &(String, bool, String))> = self
            .messages
            .iter()
            .map(|msg| {
                let available_width = messages_area.width.saturating_sub(10) as usize;
                let wrapped_lines = textwrap::wrap(&msg.0, available_width).len();
                // Count the message lines plus spacing
                let line_count = wrapped_lines + 1; // +1 for spacing
                total_lines += line_count;
                (line_count, msg)
            })
            .collect();

        // Determine which messages to show based on scroll position
        let mut lines_from_top = 0;
        let mut start_idx = 0;

        // Find the starting message based on scroll position
        for (i, (line_count, _)) in messages_with_line_counts.iter().enumerate() {
            if lines_from_top + line_count > self.messages_scroll {
                start_idx = i;
                break;
            }
            lines_from_top += line_count;
        }

        let visible_messages = self
            .messages
            .iter()
            .skip(start_idx)
            .take(visible_height)
            .enumerate()
            .map(|(_, (m, from_user, author))| {
                let is_right_aligned = *from_user;
                let available_width = messages_area.width.saturating_sub(10) as usize;
                let wrapped_message = textwrap::wrap(m, available_width);
                let mut list_item_spans = Vec::new();

                // Add a blank line before each message for spacing
                list_item_spans.push(Line::from(""));

                // Check if this is a system message or history loaded message
                let is_system_message = author == "system";
                let is_history_loaded = author == "history_loaded";

                if is_history_loaded {
                    // History loaded message - display centered dashed line
                    let line = "--- Recent Messages ---";
                    let padding = (messages_area.width as usize)
                        .saturating_sub(line.len())
                        .saturating_div(2);

                    let mut centered_spans = Vec::new();
                    centered_spans.push(Span::raw(" ".repeat(padding)));
                    centered_spans.push(Span::styled(line, Style::default().fg(Color::DarkGray)));

                    list_item_spans.push(Line::from(centered_spans));
                } else if is_system_message {
                    // System message - display centered with special styling
                    for line in wrapped_message {
                        let line_spans = vec![Span::styled(
                            format!(" {} ", line),
                            Style::default()
                                .bg(Color::Rgb(25, 38, 45))
                                .fg(Color::Rgb(102, 187, 106)),
                        )];

                        // Center the system message
                        let padding = (messages_area.width as usize)
                            .saturating_sub(line.len() + 2) // +2 for the spaces
                            .saturating_div(2);

                        let mut centered_spans = Vec::new();
                        centered_spans.push(Span::raw(" ".repeat(padding)));
                        centered_spans.extend(line_spans);

                        list_item_spans.push(Line::from(centered_spans));
                    }
                } else {
                    // Regular user message - keep existing formatting
                    for (line_idx, line) in wrapped_message.iter().enumerate() {
                        let mut line_spans = Vec::new();

                        if line_idx == 0 {
                            if is_right_aligned {
                                let padding = (messages_area.width as usize)
                                    .saturating_sub(line.len())
                                    .saturating_sub(10);

                                line_spans.push(Span::raw(" ".repeat(padding)));
                                line_spans.push(Span::styled(
                                    format!(" {} ", line),
                                    Style::default().bg(Color::Rgb(0, 92, 75)).fg(Color::White),
                                ));
                                line_spans.push(Span::styled(
                                    format!(" 🫵 "),
                                    Style::default().fg(Color::DarkGray),
                                ));
                            } else {
                                line_spans.push(Span::styled(
                                    format!(" 🤘 "),
                                    Style::default().fg(Color::DarkGray),
                                ));
                                line_spans.push(Span::styled(
                                    format!(" {} ", line),
                                    Style::default().bg(Color::Rgb(38, 45, 49)).fg(Color::White),
                                ));
                            }
                        } else {
                            if is_right_aligned {
                                let padding = (messages_area.width as usize)
                                    .saturating_sub(line.len())
                                    .saturating_sub(6);

                                line_spans.push(Span::raw(" ".repeat(padding)));
                                line_spans.push(Span::styled(
                                    format!(" {} ", line),
                                    Style::default().bg(Color::Rgb(0, 92, 75)).fg(Color::White),
                                ));
                            } else {
                                line_spans.push(Span::raw("    "));
                                line_spans.push(Span::styled(
                                    format!(" {} ", line),
                                    Style::default().bg(Color::Rgb(38, 45, 49)).fg(Color::White),
                                ));
                            }
                        }

                        list_item_spans.push(Line::from(line_spans));
                    }
                }

                // Add a small margin after each message
                list_item_spans.push(Line::from(""));

                ListItem::new(list_item_spans)
            })
            .collect::<Vec<_>>();

        // Update the messages list title to show user count
        let messages_list = List::new(visible_messages)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(69, 90, 100)))
                    .style(Style::default().bg(Color::Rgb(17, 27, 33)))
                    .padding(Padding::new(1, 1, 0, 0))
                    .title(format!(
                        " 💬 Live Human Specimens Chatting ({} spotted) ",
                        self.connected_users.lock().unwrap()
                    ))
                    .title_style(Style::default().fg(Color::Rgb(0, 230, 118))),
            )
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(messages_list, messages_area);

        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(Color::Rgb(0, 150, 136)))
                .track_style(Style::default().fg(Color::Rgb(38, 45, 49))),
            messages_area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut messages_scroll_state,
        );

        let input = Paragraph::new(self.input.as_str())
            .style(match self.input_mode {
                InputMode::Normal => Style::default().fg(Color::Gray),
                InputMode::Editing => Style::default().fg(Color::White),
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Rgb(69, 90, 100)))
                    .style(Style::default().bg(Color::Rgb(17, 27, 33)))
                    .title(" 📝 Drop Your Message Here 📝 ")
                    .title_style(Style::default().fg(Color::Rgb(0, 230, 118))),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(input, input_area);

        let (msg, style) = match self.input_mode {
            InputMode::Normal => (
                vec![
                    "Smash ".into(),
                    "Enter".bold(),
                    " to type, ".into(),
                    "Up/Down".bold(),
                    " to scroll, ".into(),
                    "q".bold(),
                    " to rage quit".into(),
                ],
                Style::default().fg(Color::Gray).bg(Color::Rgb(17, 27, 33)),
            ),
            InputMode::Editing => (
                vec![
                    "Hit ".into(),
                    "Esc".bold(),
                    " to stop, ".into(),
                    "Up/Down".bold(),
                    " to scroll, ".into(),
                    "Enter".bold(),
                    " to unleash".into(),
                ],
                Style::default().fg(Color::Gray),
            ),
        };
        let text = Text::from(Line::from(msg)).patch_style(style);
        let help_message = Paragraph::new(text).style(Style::default().bg(Color::Rgb(17, 27, 33)));
        frame.render_widget(help_message, help_area);

        match self.input_mode {
            InputMode::Normal => {}
            InputMode::Editing => frame.set_cursor_position(Position::new(
                input_area.x + self.character_index as u16 + 1,
                input_area.y + 1,
            )),
        }
    }
}
