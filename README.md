# Aerogel

**AI-Powered Overlay Interface for Wayland**

Aerogel brings intelligent assistance directly to your Wayland desktop through a seamless overlay interface. Capture screenshots, record audio, type queries, and get AI-powered solutions without leaving your current workflow.

## ✨ Demo

<video width="700" controls>
  <source src="https://raw.githubusercontent.com/kamlendras/aerogel/refs/heads/main/demo/video.mp4" type="video/mp4">
  <source src="demo/video.mp4" type="video/mp4">
  Your browser does not support the video tag.
</video>

## 🚀 Quick Start

1. **Environment Setup**
   ```bash
   # Create environment configuration
   touch .env && echo -e "OPENAI_API_KEY=\nCLAUDE_API_KEY=\nGEMINI_API_KEY=\nXAI_API_KEY=" > .env
   ```

2. **Build & Run**
   ```bash
   # Build the release version
   cargo build --release
   
   # Navigate to binary location
   cd target/release
   
   # Launch Aerogel
   ./aerogel
   ```

## ⚙️ Configuration

Customize your AI experience by editing `aerogel.toml`:

- **Model Name**: Choose your preferred AI model
- **Max Tokens**: Control response length
- **Temperature**: Adjust creativity level
- **Top P**: Fine-tune response diversity

## ⌨️ Keyboard Shortcuts

| Action | Shortcut | Description |
|--------|----------|-------------|
| **Toggle Interface** | `Ctrl + \` | Show or hide the overlay |
| **Text Input** | `Ctrl + I` | Open text input for queries |
| **Screenshot** | `Ctrl + H` | Capture current screen |
| **Audio Recording** | `Ctrl + M` | Record voice input |
| **Process Query** | `Ctrl + Enter` | Send query to AI |
| **Clear Session** | `Ctrl + G` | Reset current conversation |

## 🛠️ Requirements

- Wayland compositor
- Rust toolchain
- grim (for screenshot)
- Audio recording capabilities (for voice input)


---

*Built for modern Linux desktops running Wayland*
