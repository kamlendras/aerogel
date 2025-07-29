# Aerogel

**AI-Powered Overlay Interface for Wayland**

Aerogel brings intelligent assistance directly to your Wayland desktop through a seamless overlay interface. Capture screenshots, record audio, type queries, and get AI-powered solutions without leaving your current workflow.

## ✨ Demo

https://github.com/user-attachments/assets/0d890ee1-3782-4278-9160-51f6989b6aa9

## 🚀 Quick Start

### 1. AI Model Configuration

**Option A: Cloud-based AI Services** ☁️

Create your environment configuration with API keys for cloud-based AI services:

```bash
# Create environment configuration
touch .env && echo -e "OPENROUTER_API_KEY=\nGEMINI_API_KEY=\nOPENAI_API_KEY=\nCLAUDE_API_KEY=\nXAI_API_KEY=" > .env
```
Configure your preferred cloud AI service by adding the corresponding API key to your `.env` file.  

**Option B: Local AI with Ollama** 🏠

Run AI models locally for privacy and offline capabilities:

```bash
# Install Ollama (if not already installed)
curl -fsSL https://ollama.ai/install.sh | sh

# Or using Docker
docker pull ollama/ollama
docker run -d --name ollama -p 11434:11434 ollama/ollama

# Install your preferred model (e.g., gemma3, llava, llama4)
docker exec ollama ollama pull gemma3:4b
docker exec -d ollama ollama run gemma3:4b
```

Then update your model configuration in `aerogel.toml`:
- Set your model name (e.g., `gemma3`, `llama4`)
- Adjust `max_tokens`, `temperature`, and `top_p` as needed

### 2. Build & Run

```bash
# Build the release version
cargo build --release

# Navigate to binary location
cd target/release

# Launch Aerogel
./aerogel
```

## ⚙️ Configuration

Customize your Aerogel experience by editing `aerogel.toml`. The configuration file is searched for in the following locations (in order):

1. `$XDG_CONFIG_HOME/aerogel/aerogel.toml`
2. `$XDG_CONFIG_HOME/aerogel.toml`
3. `$HOME/.config/aerogel/aerogel.toml`
4. `$HOME/.aerogel.toml`
5. `/etc/aerogel/aerogel.toml`

### Application Settings
- **Width**: Interface width in pixels (default: 870)
- **Max Height**: Maximum interface height in pixels (default: 810)
- **Scroll Speed**: Controls scrolling sensitivity (default: 0.4)
- **Border Radius**: Interface corner rounding in pixels (default: 8.0)

### Font Configuration
- **Path**: Font file location (default: JetBrains Mono)
- **Size**: Font size in points (default: 20.0)

### Color Scheme
- **RGB Values**: Customize interface colors (red, green, blue)
- **Alpha Multiplier**: Controls transparency (default: 230.0)

### AI Model Settings
- **Model Name**: Choose your preferred AI model (local or cloud)
- **Max Tokens**: Control response length
- **Temperature**: Adjust creativity level (0.0-1.0)
- **Top P**: Fine-tune response diversity (0.0-1.0)

### Custom Keybindings
All keyboard shortcuts are configurable in the `[keybindings]` section:

| Action | Default Shortcut | Config Key | Description |
|--------|------------------|------------|-------------|
| **Toggle Interface** | `Ctrl + \` | `show_hide` | Show or hide the overlay |
| **Text Input** | `Ctrl + I` | `type_text` | Open text input for queries |
| **Screenshot** | `Ctrl + H` | `take_screenshot` | Capture current screen |
| **Audio Recording** | `Ctrl + M` | `record_audio` | Record voice input |
| **Process Query** | `Ctrl + Enter` | `solve` | Send query to AI |
| **Clear Session** | `Ctrl + G` | `clear` | Reset current conversation |

## 🐛 Troubleshooting

### Logs
Aerogel writes detailed logs to `aerogel.log` in the current working directory. Check this file for debugging information and error messages.

## 🛠️ Requirements

- Wayland compositor
- Rust toolchain
- grim (for screenshots)
- alsa-lib-devel/libasound2-dev (for audio functionality)
- Ollama (optional, for local AI model setup)

---

*Built for modern Linux desktops running Wayland*