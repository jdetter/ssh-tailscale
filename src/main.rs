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
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Configuration for the SSH Tailscale app, stored between sessions
#[derive(Serialize, Deserialize, Debug, Default)]
struct Config {
    /// Default username to use for SSH connections
    default_username: String,
    /// Last selected node name for auto-selection next time
    last_selected_node: String,
}

/// Represents a Tailscale node from the 'tailscale status' command
struct TailscaleNode {
    /// Hostname of the node
    name: String,
    /// IP address of the node
    ip: String,
    /// Suggested username from tailscale status, if available
    suggested_user: String,
    /// Connection status (active, offline, etc.)
    status: String,
}

/// App state for the terminal UI
struct App {
    /// All available nodes
    nodes: Vec<TailscaleNode>,
    /// Indices of filtered nodes
    filtered_nodes: Vec<usize>,
    /// Current search filter text
    filter: String,
    /// Currently selected node index in filtered list
    selection: usize,
}

impl App {
    /// Create a new App with the provided nodes
    fn new(nodes: Vec<TailscaleNode>) -> Self {
        let filtered_nodes = (0..nodes.len()).collect();
        Self {
            nodes,
            filtered_nodes,
            filter: String::new(),
            selection: 0,
        }
    }

    /// Apply the current filter to the nodes list
    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            // Show all nodes when no filter is applied
            self.filtered_nodes = (0..self.nodes.len()).collect();
        } else {
            // Filter nodes based on case-insensitive name matching
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

    /// Move selection up (visually) - IMPORTANT: When rendering bottom-to-top, 
    /// moving "up" visually means INCREASING the index in the array
    fn move_selection_up(&mut self) {
        if !self.filtered_nodes.is_empty() && self.selection + 1 < self.filtered_nodes.len() {
            self.selection += 1;
        }
    }

    /// Move selection down (visually) - IMPORTANT: When rendering bottom-to-top,
    /// moving "down" visually means DECREASING the index in the array
    fn move_selection_down(&mut self) {
        if !self.filtered_nodes.is_empty() && self.selection > 0 {
            self.selection -= 1;
        }
    }

    /// Move selection up a full page
    fn move_page_up(&mut self, page_size: usize) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection >= page_size {
            self.selection -= page_size;
        } else {
            self.selection = 0;
        }
    }

    /// Move selection down a full page
    fn move_page_down(&mut self, page_size: usize) {
        if self.filtered_nodes.is_empty() {
            return;
        }

        if self.selection + page_size < self.filtered_nodes.len() {
            self.selection += page_size;
        } else {
            self.selection = self.filtered_nodes.len() - 1;
        }
    }

    /// Move to the first item in the list
    fn move_to_start(&mut self) {
        if !self.filtered_nodes.is_empty() {
            self.selection = 0;
        }
    }

    /// Move to the last item in the list
    fn move_to_end(&mut self) {
        if !self.filtered_nodes.is_empty() {
            self.selection = self.filtered_nodes.len() - 1;
        }
    }

    /// Get the currently selected node, if available
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
    
    // Run the terminal UI to select a node
    let selected_node = run_tui(nodes, &config.last_selected_node)?;
    
    // Save the selected node for next time
    config.last_selected_node = selected_node.name.clone();
    save_config(&config)?;
    
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

/// Run the terminal UI for node selection
fn run_tui(nodes: Vec<TailscaleNode>, last_selected_node: &str) -> Result<TailscaleNode> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state with initial selection
    let mut app = App::new(nodes);
    
    // Find and select the last used node if available
    if !last_selected_node.is_empty() {
        // Find the index of the last selected node
        if let Some((index, _)) = app.nodes.iter().enumerate()
            .find(|(_, node)| node.name == last_selected_node) {
            // Only update if the node is found
            app.selection = index;
        }
    }
    
    // Final result storage
    let result;

    // Main loop
    {
        let tick_rate = Duration::from_millis(100);
        let mut last_tick = Instant::now();
        
        // This loop runs until a node is selected or the user exits
        loop {
            // Draw the UI
            terminal.draw(|f| ui(f, &mut app))?;

            // Handle events with timeout
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
                
            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        // Exit on Ctrl+C or Ctrl+Q
                        KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            result = Err(anyhow!("User cancelled"));
                            break;
                        }
                        // Select current node on Enter
                        KeyCode::Enter => {
                            if let Some(node) = app.get_selected_node() {
                                // Make a copy of the selected node to return
                                let selected_node = TailscaleNode {
                                    name: node.name.clone(),
                                    ip: node.ip.clone(),
                                    suggested_user: node.suggested_user.clone(),
                                    status: node.status.clone(),
                                };
                                result = Ok(selected_node);
                                break;
                            }
                        }
                        // Navigation keys - correct visual direction
                        KeyCode::Up => app.move_selection_up(), 
                        KeyCode::Down => app.move_selection_down(),
                        // Vim keys - match visual direction
                        KeyCode::Char('k') => app.move_selection_up(),
                        KeyCode::Char('j') => app.move_selection_down(),
                        KeyCode::PageUp => app.move_page_up(10),
                        KeyCode::PageDown => app.move_page_down(10),
                        KeyCode::Home => app.move_to_start(),
                        KeyCode::End => app.move_to_end(),
                        // Filter text editing
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

            // Refresh timer
            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }
    }

    // Restore terminal state
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

/// Render the UI using Ratatui
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

    // Header with title and node count
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
        // Create list items in reverse order for bottom-up display
        let mut items: Vec<ListItem> = Vec::new();
        
        for &idx in app.filtered_nodes.iter().rev() {
            let node = &app.nodes[idx];
            
            // Color status based on online/offline
            let status_style = if node.status.contains("active") {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };
            
            // Format node information with improved spacing
            let content = Line::from(vec![
                Span::raw(format!("{:<55}", node.name)),  // Increase padding even more for hostname
                Span::raw(format!("{:<20}", node.ip)),    // Add more space for IP address
                Span::styled(&node.status, status_style),
            ]);
            
            items.push(ListItem::new(content));
        }
        
        // Display the list with selection
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::NONE)
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol("> ");
        
        // Since we reversed the items for display, we need to convert the selection index
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

    // Footer with search bar and help text
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

/// Parse the output of 'tailscale status' to get a list of nodes
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
