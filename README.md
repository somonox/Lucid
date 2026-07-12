# Lucid

A powerful and elegant CLI tool for accessing DreamHack features directly from your terminal.

## ✨ Features

- 🔐 **Seamless Authentication**: Login to DreamHack using Google OAuth
- 👤 **User Profile Management**: View your stats and achievements
- 🎯 **Challenge Browser**: List and explore wargame challenges
- 💾 **Persistent Sessions**: Secure session storage for convenience
- 🎨 **Beautiful CLI**: Colored output and interactive prompts

## 📦 Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/lucid.git
cd lucid

# Build with Cargo
cargo build --release

# Install globally
cargo install --path .
```

### Usage

Once installed, you can use the `lucid` command from anywhere in your terminal.

## 🚀 Getting Started

### Authentication

Lucid provides two convenient authentication methods:

#### Browser Authentication (Recommended)

```bash
lucid login
```

1. Select "Browser Authentication" from the menu
2. Your browser will automatically open to DreamHack's login page
3. Sign in with your Google account
4. After successful login, open Developer Tools (F12)
5. Navigate to Application/Storage → Cookies → `https://dreamhack.io`
6. Copy the `sessionid` and `csrftoken` values when prompted

#### Manual Cookie Input

```bash
lucid login
```

1. Select "Manual Cookie Input"
2. Paste your cookie string in the format: `sessionid=xxx; csrftoken=yyy`

### Available Commands

#### View Your Profile
```bash
lucid me
```
Displays your username, points, and ranking.

#### Browse Challenges
```bash
# List all challenges
lucid challenges

# Filter by category
lucid challenges --category web
lucid challenges --category pwn
lucid challenges --category reversing
```

#### Logout
```bash
lucid logout
```
Clears your saved session data.

## 🔧 How It Works

Lucid interfaces with DreamHack's API to provide a seamless command-line experience:

1. **OAuth Integration**: Initiates Google OAuth flow for secure authentication
2. **Session Management**: Stores authentication tokens locally for persistent access
3. **API Communication**: Interacts with DreamHack's endpoints to fetch data
4. **Local Storage**: Securely saves credentials in your system's config directory

## 🔒 Security

Your credentials are stored securely in your system's configuration directory:

- **Linux**: `~/.config/lucid/`
- **macOS**: `~/Library/Application Support/com.dreamhack.lucid/`
- **Windows**: `%APPDATA%\lucid\`

Session data is stored locally and never transmitted to third parties.

## 🛠️ Development

### Project Structure

```
lucid/
├── src/
│   ├── main.rs       # CLI entry point and command routing
│   ├── auth.rs       # Authentication logic and flows
│   ├── api.rs        # DreamHack API client
│   └── config.rs     # Configuration and session management
├── Cargo.toml        # Project dependencies
└── README.md         # Documentation
```

### Dependencies

- `reqwest` - HTTP client for API requests
- `tokio` - Async runtime
- `clap` - Command-line argument parsing
- `serde` - JSON serialization/deserialization
- `dialoguer` - Interactive terminal prompts
- `webbrowser` - Cross-platform browser launcher
- `colored` - Terminal color output

### Building from Source

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with debug output
RUST_LOG=debug cargo run -- <command>
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## ⚠️ Disclaimer

Lucid is an unofficial tool and is not affiliated with or endorsed by DreamHack. Use responsibly and in accordance with DreamHack's terms of service.

## 🙏 Acknowledgments

- DreamHack for providing an excellent platform for learning cybersecurity
- The Rust community for amazing tools and libraries
- All contributors who help improve Lucid

---

**Lucid** - Making DreamHack accessible from your terminal ✨