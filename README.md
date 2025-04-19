# SSH Tailscale

A CLI utility for easily connecting to your Tailscale nodes via SSH.

## Features

- Lists all available Tailscale nodes
- Fuzzy search functionality to quickly find the node you want
- Simple user interface for selecting nodes and entering username
- Automatic SSH connection after selection

## Prerequisites

- Tailscale must be installed and configured
- SSH client must be installed

## Installation

1. Clone this repository
2. Build the project with `cargo build --release`
3. Add the binary to your PATH by creating a symlink:

```bash
# Option 1: Link to /usr/local/bin (requires sudo)
sudo ln -s "$(pwd)/target/release/ssh-tailscale" /usr/local/bin/ssh-tailscale

# Option 2: Link to ~/.local/bin (create directory if it doesn't exist)
mkdir -p ~/.local/bin
ln -s "$(pwd)/target/release/ssh-tailscale" ~/.local/bin/ssh-tailscale
# Make sure ~/.local/bin is in your PATH
```

## Usage

Simply run the command:

```bash
ssh-tailscale
```

1. Select a Tailscale node using fuzzy search
2. Enter the username for the SSH connection
3. The tool will connect you via SSH
