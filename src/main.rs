use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dialoguer::{theme::ColorfulTheme, Input};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize, Debug, Default)]
struct Config {
    default_username: String,
}

struct TailscaleNode {
    name: String,
    ip: String,
    suggested_user: String,
    status: String,
}

/// App holds the state of the application
struct App {
    nodes: Vec<TailscaleNode>,
    filtered_nodes: Vec<usize>,
    filter: String,
    selection: usize,
    scroll_offset: usize,
}

impl App {
    fn new(nodes: Vec<TailscaleNode>) -> Self {
        let filtered_nodes = (0..nodes.len()).collect();
        Self {
            nodes,
            filtered_nodes,
            filter: String::new(),
            selection: 0,
            scroll_offset: 0,
        }
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.filtered_nodes = (0..self.nodes.len()).collect();
        } else {
            let lower_filter = self.filter.to_lowercase();
            self.filtered_nodes = (0..self.nodes.len())
                .filter(|&i| self.nodes[i].name.to_lowercase().contains(&lower_filter))
                .collect();
        }

        // Adjust selection if necessary
        if self.filtered_nodes.is_empty() {
            self.selection = 0;
        } else if self.selection >= self.filtered_nodes.len() {
            self.selection = self.filtered_nodes.len() - 1;
        }
    }

    fn move_selection_up(&mut self) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection + 1 < self.filtered_nodes.len() {
            self.selection += 1;
        }
    }

    fn move_selection_down(&mut self) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection > 0 {
            self.selection -= 1;
        }
    }

    fn move_page_up(&mut self, page_size: usize) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection + page_size < self.filtered_nodes.len() {
            self.selection += page_size;
        } else {
            self.selection = self.filtered_nodes.len() - 1;
        }
    }

    fn move_page_down(&mut self, page_size: usize) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection > page_size {
            self.selection -= page_size;
        } else {
            self.selection = 0;
        }
    }

    fn move_to_start(&mut self) {
        if !self.filtered_nodes.is_empty() {
            self.selection = 0;
        }
    }

    fn move_to_end(&mut self) {
        if !self.filtered_nodes.is_empty() {
            self.selection = self.filtered_nodes.len() - 1;
        }
    }

    fn get_selected_node(&self) -> Option<&TailscaleNode> {
        if self.filtered_nodes.is_empty() {
            None
        } else {
            Some(&self.nodes[self.filtered_nodes[self.selection]])
        }
    }
}

fn main() -> Result<()> {
    // Load configuration
    let mut config = load_config()?;
    
    // Run tailscale status to get list of nodes
    let nodes = get_tailscale_nodes().context("Failed to get Tailscale nodes")?;
    
    if nodes.is_empty() {
        println!("No Tailscale nodes found. Make sure Tailscale is connected.");
        return Ok(());
    }
    
    // Setup terminal
    let selected_node = run_tui(nodes)?;
    
    // Get the default username from config or fallback to "ubuntu"
    let default_username = if !config.default_username.is_empty() {
        config.default_username.clone()
    } else {
        "ubuntu".to_string()
    };
    
    // Username prompt with the saved default
    let username: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Enter username for {}", selected_node.name))
        .default(default_username)
        .interact_text()?;
    
    // Save the username for next time if it changed
    if username != config.default_username {
        config.default_username = username.clone();
        save_config(&config)?;
    }
    
    // Connect via SSH
    println!("Connecting to {}@{}...", username, selected_node.name);
    
    // Execute SSH command
    let status = Command::new("ssh")
        .arg(format!("{}@{}", username, selected_node.ip))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute SSH command")?;
    
    if !status.success() {
        println!("SSH connection ended with non-zero status: {}", status);
    }
    
    Ok(())
}

fn run_tui(nodes: Vec<TailscaleNode>) -> Result<TailscaleNode> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(nodes);

    // Main loop
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let result = loop {
        // Draw UI
        terminal.draw(|f| ui(f, &mut app))?;

        // Handle input
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Err(anyhow!("User cancelled"));
                    }
                    KeyCode::Enter => {
                        if let Some(node) = app.get_selected_node() {
                            // Make a copy of the selected node to return
                            let selected_node = TailscaleNode {
                                name: node.name.clone(),
                                ip: node.ip.clone(),
                                suggested_user: node.suggested_user.clone(),
                                status: node.status.clone(),
                            };
                            break Ok(selected_node);
                        }
                    }
                    // Swap keys for bottom-up display
                    KeyCode::Up => app.move_selection_up(), 
                    KeyCode::Down => app.move_selection_down(),
                    // Add j/k key support for vim users
                    KeyCode::Char('j') => app.move_selection_down(),
                    KeyCode::Char('k') => app.move_selection_up(),
                    KeyCode::PageUp => app.move_page_up(10),
                    KeyCode::PageDown => app.move_page_down(10),
                    KeyCode::Home => app.move_to_start(),
                    KeyCode::End => app.move_to_end(),
                    KeyCode::Backspace => {
                        app.filter.pop();
                        app.apply_filter();
                    }
                    KeyCode::Esc => {
                        app.filter.clear();
                        app.apply_filter();
                    }
                    KeyCode::Char(c) => {
                        app.filter.push(c);
                        app.apply_filter();
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Return result or propagate error
    result
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let size = f.size();

    // Create layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),    // Header
                Constraint::Min(3),       // List
                Constraint::Length(3),    // Footer/Search
            ]
            .as_ref(),
        )
        .split(size);

    // Header
    let header_text = vec![
        Line::from(vec![
            Span::styled(
                "Tailscale SSH - Select a Node",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )
        ]),
        Line::from(vec![
            Span::styled(
                format!("Found {} nodes", app.nodes.len()),
                Style::default().fg(Color::Gray),
            )
        ]),
    ];
    let header = Paragraph::new(header_text)
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, chunks[0]);

    // List of nodes from bottom to top
    if !app.filtered_nodes.is_empty() {
        // Create list items, but in reverse order for bottom-up display
        let mut items: Vec<ListItem> = Vec::new();
        
        for (i, &idx) in app.filtered_nodes.iter().enumerate().rev() {
            let node = &app.nodes[idx];
            
            let status_style = if node.status.contains("active") {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };
            
            let content = Line::from(vec![
                Span::raw(format!("{:30}", node.name)),
                Span::raw(format!("{:15}", node.ip)),
                Span::styled(&node.status, status_style),
            ]);
            
            items.push(ListItem::new(content));
        }
        
        // Display the list with selection
        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        
        // Since we reversed the items for display, we need to convert the actual selection index
        let display_selection = app.filtered_nodes.len() - 1 - app.selection;
        
        // Use stateful list to track selection
        let mut state = ratatui::widgets::ListState::default();
        state.select(Some(display_selection));
        
        f.render_stateful_widget(list, chunks[1], &mut state);
    } else if !app.filter.is_empty() {
        // No results for filter
        let no_results = Paragraph::new("No nodes match your filter")
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(no_results, chunks[1]);
    }

    // Footer with search bar
    let search_text = format!("Search: {}", app.filter);
    let search = Paragraph::new(search_text)
        .style(Style::default())
        .block(
            Block::default()
                .borders(Borders::TOP)
                .title("Enter: Connect  Esc: Clear filter  ↑/↓: Navigate  Ctrl+C: Exit"),
        );
    f.render_widget(search, chunks[2]);
}

/// Get the configuration directory path
fn get_config_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    let config_dir = home_dir.join(".config").join("ssh-tailscale");
    
    // Create the directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }
    
    Ok(config_dir)
}

/// Get the configuration file path
fn get_config_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("config.json"))
}

/// Load configuration from the config file
fn load_config() -> Result<Config> {
    let config_path = get_config_path()?;
    
    if config_path.exists() {
        let config_str = fs::read_to_string(config_path)?;
        Ok(serde_json::from_str(&config_str).unwrap_or_default())
    } else {
        // Return default config if file doesn't exist
        Ok(Config::default())
    }
}

/// Save configuration to the config file
fn save_config(config: &Config) -> Result<()> {
    let config_path = get_config_path()?;
    let config_str = serde_json::to_string_pretty(config)?;
    fs::write(config_path, config_str)?;
    Ok(())
}

fn get_tailscale_nodes() -> Result<Vec<TailscaleNode>> {
    // Run 'tailscale status' command
    let output = Command::new("tailscale")
        .arg("status")
        .output()
        .context("Failed to execute 'tailscale status'. Is tailscale installed and in your PATH?")?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "Tailscale status command failed: {}. Make sure Tailscale is connected.", 
            error
        ));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    
    // Parse the output to extract node information
    let mut nodes = Vec::new();
    
    // Regular expression to match node entries
    // The format is typically:
    // 100.74.180.3    testnet-staging-load-balancer-1 piotr@       linux   offline
    // [IP]            [HOSTNAME]                      [USERNAME@]  [OS]    [STATUS]
    let re = Regex::new(r"^(\d+\.\d+\.\d+\.\d+)\s+(\S+)\s+(\S*)\s+(\S+)\s+(\S+)")?;
    
    for line in output_str.lines() {
        if line.trim().is_empty() || line.contains("tagmap") || line.contains("subnet") {
            continue;
        }
        
        if let Some(captures) = re.captures(line) {
            let ip = captures.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let name = captures.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let suggested_user = captures.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let status = captures.get(5).map(|m| m.as_str().to_string()).unwrap_or_default();
            
            // Only add nodes with non-empty names and IPs
            if !name.is_empty() && !ip.is_empty() {
                nodes.push(TailscaleNode { 
                    name, 
                    ip, 
                    suggested_user,
                    status,
                });
            }
        }
    }
    
    // If we couldn't parse any nodes with the regex, try printing the output for debugging
    if nodes.is_empty() && !output_str.trim().is_empty() {
        println!("Warning: Could not parse tailscale status output. Raw output:\n{}", output_str);
    }
    
    Ok(nodes)
}
