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
touch .env && echo -e "GEMINI_API_KEY=\nOPENAI_API_KEY=\nCLAUDE_API_KEY=\nXAI_API_KEY=" > .env
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

Customize your AI experience by editing `aerogel.toml`:

- **Model Name**: Choose your preferred AI model (local or cloud)
- **Max Tokens**: Control response length
- **Temperature**: Adjust creativity level (0.0-1.0)
- **Top P**: Fine-tune response diversity (0.0-1.0)

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
- grim (for screenshots)
- Audio recording capabilities (for voice input)
- Ollama (optional, for local AI model setup)

---

*Built for modern Linux desktops running Wayland*
