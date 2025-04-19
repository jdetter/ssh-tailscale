# SSH Tailscale

A CLI utility that makes it easy to connect to your Tailscale nodes via SSH using an interactive terminal interface.

## Features

- Interactive terminal UI with fuzzy search functionality
- Displays all Tailscale nodes with their connection status
- Remembers your last used username for SSH connections
- Keyboard navigation with Vim-style keys support (j/k, Page Up/Down)
- Intuitive bottom-up display that mimics typical terminal usage

## Prerequisites

- Tailscale must be installed and configured
- SSH client must be installed
- Rust and Cargo for installation from source

## Installation

### From Source

1. Clone this repository
2. Build and install:

```
cd ssh-tailscale
cargo install --path .
```

## Usage

Simply run the command:

```bash
ssh-tailscale
```

### Navigation

- **Up/Down arrows** or **k/j keys**: Navigate through the list of nodes
- **Page Up/Down**: Move up/down by page
- **Home/End**: Jump to the beginning/end of the list
- **Enter**: Select the current node and connect via SSH
- **Type text**: Filter nodes by hostname in real-time
- **Esc**: Clear the current filter
- **Ctrl+C**: Exit the application

## Configuration

The application stores configuration in `~/.config/ssh-tailscale/config.json`, which currently includes:

- `default_username`: The last username you used for SSH connections

## Development

The application is built with:

- [Rust](https://www.rust-lang.org/)
- [Ratatui](https://github.com/ratatui-org/ratatui) for the terminal UI
- [Crossterm](https://github.com/crossterm-rs/crossterm) for cross-platform terminal support
- [Anyhow](https://github.com/dtolnay/anyhow) for error handling
- [Regex](https://github.com/rust-lang/regex) for parsing Tailscale output
- [Serde](https://github.com/serde-rs/serde) for configuration serialization

## License

Licensed under the [Apache License, Version 2.0](LICENSE) (the "License"); you may not use this software except in compliance with the License.
