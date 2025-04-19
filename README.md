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
2. Build and install via `cargo install --path .`

## Usage

Simply run the command:

```bash
ssh-tailscale
```

1. Select a Tailscale node using fuzzy search
2. Enter the username for the SSH connection
3. The tool will connect you via SSH
