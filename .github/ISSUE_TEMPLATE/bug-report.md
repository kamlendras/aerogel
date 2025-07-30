---
name: Bug Report
about: Report a bug with the aerogel AI overlay for Wayland
title: "[BUG] "
labels: bug
assignees: ''

---

## Bug Description
**A clear and concise description of what the bug is.**

## Steps to Reproduce
**Steps to reproduce the behavior:**
1. Launch aerogel with `./aerogel`
2. Use keyboard shortcut '...' or action '...'
3. Observe the issue

## Expected Behavior
**A clear and concise description of what you expected to happen.**

## Actual Behavior
**A clear and concise description of what actually happened.**

## Screenshots/Videos
**If applicable, add screenshots or screen recordings to help explain the problem.**

## Environment Information
**Please complete the following information:**
- **OS Distribution**: [e.g. Ubuntu 22.04, Arch Linux, Fedora 39]
- **Wayland Compositor**: [e.g. Sway, Hyprland, River]
- **Aerogel Version**: [output of `git rev-parse HEAD` or release tag]
- **Rust Version**: [output of `rustc --version`]
- **Cargo Version**: [output of `cargo --version`]

## Dependencies Status
**Check which dependencies are installed:**
- [ ] alsa-lib-devel/libasound2-dev
- [ ] grim installed: [output of `grim --version` or `which grim`]
- [ ] Wayland session active: [output of `echo $WAYLAND_DISPLAY`]

## AI Configuration
**Which AI setup are you using?**
- [ ] Cloud-based AI (OpenRouter/Gemini/OpenAI/Claude/XAI)
- [ ] Local AI with Ollama

**If using cloud AI:**
- Service: [e.g. OpenAI, Claude, Gemini]
- API key configured: [Yes/No - don't share the actual key]

**If using Ollama:**
- Ollama version: [output of `ollama --version`]
- Model name: [e.g. gemma3:4b, llama4]
- Ollama running: [output of `ollama list`]

## Configuration File
**Share your aerogel.toml configuration:**
```
# paste your aerogel.toml here
```

## Error Logs
**Include any error messages from the aerogel.log:**
```
# Paste error logs here
```

## Keyboard Shortcuts
**Which keyboard shortcut was being used when the issue occurred?**
- [ ] Ctrl + \ (Toggle Interface)
- [ ] Ctrl + I (Text Input)
- [ ] Ctrl + H (Screenshot)
- [ ] Ctrl + M (Audio Recording)
- [ ] Ctrl + Enter (Process Query)
- [ ] Ctrl + G (Clear Session)
- [ ] Other: ___________

## Additional Context
**Add any other context about the problem:**
- Does this happen consistently or intermittently?
- Did this work before? When did it start failing?
- Any specific applications running when the bug occurs?
- Any compositor-specific settings that might be relevant?

## Workarounds
**Have you found any temporary workarounds for this issue?**

---

### Checklist
- [ ] I have searched for existing issues before creating this one
- [ ] I am running a Wayland session (not X11)
- [ ] I have verified grim is installed and working
- [ ] I have provided my aerogel.toml configuration
- [ ] I can reproduce this issue consistently
