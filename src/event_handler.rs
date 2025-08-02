use chrono::Local;
use config::{Config, File as ConfigFile};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

const LOG_FILE_TO_WATCH: &str = ".event";
const AI_EXECUTABLE: &str = "./ai_manager";
const SCREENSHOT_DIR: &str = "screenshots";
const WORKSPACE_STATE_FILE: &str = ".aerogel_workspace";

#[derive(Debug, Deserialize, Clone)]
struct Keybindings {
    show_hide: String,
    type_text: String,
    take_screenshot: String,
    record_audio: String,
    solve: String,
    clear: String,
    switch_to_workspace: String,
}

#[derive(Debug)]
struct CanonicalKeybindings {
    show_hide: String,
    type_text: String,
    take_screenshot: String,
    record_audio: String,
    solve: String,
    clear: String,
    switch_to_workspace: String,
}

#[derive(Debug, Deserialize)]
struct Settings {
    keybindings: Keybindings,
}

fn canonicalize_keybinding(kb_string: &str) -> String {
    if !kb_string.contains('+') {
        return kb_string.to_string();
    }
    let mut parts: Vec<&str> = kb_string.split('+').collect();

    let final_key = parts.pop().unwrap_or("");

    parts.sort_unstable();

    parts.push(final_key);
    parts.join("+")
}

// Generates the path for the temporary log file for a given workspace.
fn get_text_log_output_path(workspace_number: u32) -> String {
    format!(".tmp{}", workspace_number)
}

// Manages the lifecycle of a single AI subprocess.
async fn manage_ai_process(mut command_rx: mpsc::Receiver<String>, ai_arg: String) {
    loop {
        println!(
            "[event_handler] Spawning '{}' for workspace file '{}'...",
            AI_EXECUTABLE, ai_arg
        );
        let mut child = match Command::new(AI_EXECUTABLE)
            .arg(&ai_arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!(
                    "[event_handler] CRITICAL: Failed to spawn '{}' for '{}': {}. Retrying in 2s.",
                    AI_EXECUTABLE, ai_arg, e
                );
                continue;
            }
        };

        let mut stdin = child.stdin.take().expect("Failed to open child stdin");
        let stdout = child.stdout.take().expect("Failed to open child stdout");
        let stderr = child.stderr.take().expect("Failed to open child stderr");

        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line_buf = String::new();
            loop {
                match reader.read_line(&mut line_buf).await {
                    Ok(0) => break,
                    Ok(_) => {
                        print!("{}", line_buf);
                        io::stdout().flush().unwrap();
                        line_buf.clear();
                    }
                    Err(e) => {
                        eprintln!("[event_handler] Error reading from AI stdout: {}", e);
                        break;
                    }
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line_buf = String::new();
            loop {
                match reader.read_line(&mut line_buf).await {
                    Ok(0) => break,
                    Ok(_) => {
                        eprint!("{}", line_buf);
                        io::stderr().flush().unwrap();
                        line_buf.clear();
                    }
                    Err(e) => {
                        eprintln!("[event_handler] Error reading from AI stderr: {}", e);
                        break;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                Some(command) = command_rx.recv() => {
                    if command == "/restart" {
                        println!("[event_handler] Received restart command for '{}'. Terminating AI process...", ai_arg);
                        if let Err(e) = child.kill().await {
                            eprintln!("[event_handler] Failed to kill AI process for '{}': {}. It may have already exited.", ai_arg, e);
                        }
                        break;
                    }

                    println!("[event_handler] Sending command to '{}': {}", ai_arg, command.lines().next().unwrap_or(""));
                    let command_with_newline = format!("{}\n", command);
                    if let Err(e) = stdin.write_all(command_with_newline.as_bytes()).await {
                        eprintln!("[event_handler] Error writing to AI stdin for '{}': {}. Process may have died.", ai_arg, e);
                        break;
                    }
                    if let Err(e) = stdin.flush().await {
                         eprintln!("[event_handler] Error flushing AI stdin for '{}': {}. Process may have died.", ai_arg, e);
                         break;
                    }
                }
                status = child.wait() => {
                    eprintln!("[event_handler] AI process for '{}' exited with status: {:?}", ai_arg, status);
                    break;
                }
            }
        }

        stdout_task.abort();
        stderr_task.abort();
        eprintln!(
            "[event_handler] AI process for '{}' has terminated. Will attempt to respawn.",
            ai_arg
        );
    }
}

// Helper function to write the current workspace number to the state file.
async fn update_workspace_state_file(workspace_number: u32) -> io::Result<()> {
    tokio::fs::write(WORKSPACE_STATE_FILE, workspace_number.to_string()).await
}

// Handles the logic of switching to a new workspace.
async fn switch_workspace(
    new_workspace_str: &str,
    current_workspace: &mut u32,
    ai_process_senders: &mut HashMap<u32, mpsc::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(new_workspace_num) = new_workspace_str.parse::<u32>() {
        if new_workspace_num == 0 {
            println!("\n>>> Workspace 0 is not a valid target. Switch cancelled.");
            return Ok(());
        }
        if new_workspace_num == *current_workspace {
            // Silently ignore switching to the same workspace for a smoother UX
            return Ok(());
        }

        println!("\n>>> Switching to workspace {}", new_workspace_num);
        *current_workspace = new_workspace_num;

        update_workspace_state_file(*current_workspace).await?;

        if !ai_process_senders.contains_key(current_workspace) {
            println!(
                "First time on workspace {}, starting new AI manager.",
                current_workspace
            );
            let (new_tx, new_rx) = mpsc::channel(32);
            let log_path = get_text_log_output_path(*current_workspace);

            clear_text_log(&log_path).await?; // Ensure file is clean for new process

            tokio::spawn(manage_ai_process(new_rx, log_path.clone()));
            ai_process_senders.insert(*current_workspace, new_tx);
        }
    } else if !new_workspace_str.is_empty() {
        println!(
            "\n>>> Invalid workspace number: '{}'. Switch cancelled.",
            new_workspace_str
        );
    }
    // Ignore empty string case
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let search_paths: Vec<std::path::PathBuf> = {
        let mut paths = Vec::new();
        paths.push("../../aerogel.toml".into());
        paths.push("aerogel.toml".into());
        if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            let base = std::path::PathBuf::from(xdg_config);
            paths.push(base.join("aerogel/aerogel.toml"));
            paths.push(base.join("aerogel.toml"));
        }
        if let Ok(home) = std::env::var("HOME") {
            let base = std::path::PathBuf::from(home);
            paths.push(base.join(".config/aerogel/aerogel.toml"));
            paths.push(base.join(".aerogel.toml"));
        }
        paths.push("/etc/aerogel/aerogel.toml".into());
        paths
    };

    let config_path = search_paths.iter().find(|p| p.exists()).ok_or_else(|| {
        let path_list = search_paths
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n  - ");
        let err_msg = format!(
            "Could not find 'aerogel.toml' in any standard location.\nSearched paths:\n  - {}",
            path_list
        );
        Box::<dyn std::error::Error>::from(err_msg)
    })?;

    let settings = Config::builder()
        .add_source(ConfigFile::from(config_path.clone()).required(true))
        .build()?
        .try_deserialize::<Settings>()?;
    let raw_keybindings = settings.keybindings;

    let keybindings = CanonicalKeybindings {
        show_hide: canonicalize_keybinding(&raw_keybindings.show_hide),
        type_text: canonicalize_keybinding(&raw_keybindings.type_text),
        take_screenshot: canonicalize_keybinding(&raw_keybindings.take_screenshot),
        record_audio: canonicalize_keybinding(&raw_keybindings.record_audio),
        solve: canonicalize_keybinding(&raw_keybindings.solve),
        clear: canonicalize_keybinding(&raw_keybindings.clear),
        switch_to_workspace: canonicalize_keybinding(&raw_keybindings.switch_to_workspace),
    };

    tokio::fs::create_dir_all(SCREENSHOT_DIR).await?;

    let mut current_workspace: u32 = 1;
    let mut ai_process_senders = HashMap::<u32, mpsc::Sender<String>>::new();

    update_workspace_state_file(current_workspace).await?;

    // Start the initial AI process for workspace 1
    let (initial_tx, initial_rx) = mpsc::channel(32);
    let initial_log_path = get_text_log_output_path(current_workspace);
    tokio::spawn(manage_ai_process(initial_rx, initial_log_path.clone()));
    ai_process_senders.insert(current_workspace, initial_tx);
    clear_text_log(&initial_log_path).await?;

    start_overlay().await;

    println!("Starting log watcher...");
    let log_path_str = LOG_FILE_TO_WATCH;

    if !Path::new(log_path_str).exists() {
        File::create(log_path_str).await?.shutdown().await?;
        println!("Created log file '{}'.", log_path_str);
    }
    let mut file_pos = std::fs::metadata(log_path_str)?.len();
    println!("Watching '{}' for new entries.", log_path_str);
    println!("Keybindings loaded from {}:", config_path.display());
    println!("  - Show/Hide: {}", raw_keybindings.show_hide);
    println!("  - Type Text: {}", raw_keybindings.type_text);
    println!("  - Screenshot: {}", raw_keybindings.take_screenshot);
    println!("  - Audio Record: {}", raw_keybindings.record_audio);
    println!("  - Solve: {}", raw_keybindings.solve);
    println!("  - Clear: {}", raw_keybindings.clear);
    println!(
        "  - Switch Workspace: {}",
        raw_keybindings.switch_to_workspace
    );
    println!("\nCurrently on Workspace: {}", current_workspace);

    let (tx, mut rx) = mpsc::channel(1);
    let mut watcher: RecommendedWatcher = Watcher::new(
        move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if let notify::EventKind::Modify(_) = event.kind {
                    tx.blocking_send(()).expect("Failed to send event");
                }
            }
        },
        Default::default(),
    )?;
    watcher.watch(Path::new(log_path_str), RecursiveMode::NonRecursive)?;

    let mut in_recording_mode = false;
    let mut key_buffer = String::new();
    let mut log_snapshot_before_recording = String::new();
    let mut active_modifiers = Vec::<String>::new();

    // State for workspace switching
    let mut waiting_for_workspace_number = false;

    // Main Event Loop
    while let Some(_) = rx.recv().await {
        let new_content = read_new_content(log_path_str, &mut file_pos).await?;
        for line in new_content.lines() {
            let trimmed_line = line.trim();
            if trimmed_line.is_empty() {
                continue;
            }

            let current_log_path = get_text_log_output_path(current_workspace);
            let ai_tx = ai_process_senders
                .get(&current_workspace)
                .expect("BUG: No sender for current workspace");

            
            if waiting_for_workspace_number {
                let is_digit = trimmed_line.len() == 1
                    && trimmed_line.chars().next().unwrap().is_ascii_digit();

              
                waiting_for_workspace_number = false;

                if is_digit {
                    switch_workspace(
                        trimmed_line,
                        &mut current_workspace,
                        &mut ai_process_senders,
                    )
                    .await?;
                    active_modifiers.clear();
                    continue;
                } else {
               
                    println!("\n>>> Workspace switch cancelled (non-digit pressed).");
                }
            }
          

            let is_modifier = matches!(
                trimmed_line,
                "[CtrlL]" | "[CtrlR]" | "[AltL]" | "[AltR]" | "[ShiftL]" | "[ShiftR]" | "[SuperL]"
            );

            if is_modifier {
             
                let canonical_modifier = get_canonical_modifier(trimmed_line);
                let modifier_str = trimmed_line.to_string();

                
                if !active_modifiers.contains(&modifier_str) {
                    active_modifiers.push(modifier_str);
                }

               
                if !canonical_modifier.is_empty()
                    && canonical_modifier.eq_ignore_ascii_case(&keybindings.switch_to_workspace)
                {
                    waiting_for_workspace_number = true;
                }
              
            } else {
                if !active_modifiers.is_empty() {
                    let mut combo_parts: Vec<&str> = active_modifiers
                        .iter()
                        .map(|m| get_canonical_modifier(m.as_str()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    combo_parts.sort_unstable();
                    combo_parts.dedup();

                    let final_key = match trimmed_line {
                        "[Enter]" => "Enter",
                      "[Space]" => "Space", 
                        key => key.trim_start_matches("shift+"), 
                    };

                    let mut combo_string_parts = combo_parts;
                    combo_string_parts.push(final_key);
                    let combo_string = combo_string_parts.join("+");

                    let mut combo_matched = true;

                    if combo_string.eq_ignore_ascii_case(&keybindings.take_screenshot) {
                        println!(
                            "\n>>> Trigger: Screenshot ({}) on ws {}",
                            &combo_string, current_workspace
                        );
                        let overlay_was_running = is_overlay_running().await;
                        if overlay_was_running {
                            stop_overlay().await;
                        }

                        let timestamp = Local::now().format("%Y%m%d-%H%M%S");
                        let filename =
                            format!("screenshot-ws{}-{}.jpeg", current_workspace, timestamp);
                        let path = Path::new(SCREENSHOT_DIR).join(filename);

                        if take_screenshot(&path).await.is_ok() {
                            println!("Screenshot saved to '{}'", path.display());
                            let command = format!("/upload {}", path.display());
                            if let Err(e) = ai_tx.send(command).await {
                                eprintln!("Error sending upload command to AI manager: {}", e);
                            }
                        } else {
                            eprintln!("Error taking screenshot. Is 'grim' installed?");
                        }

                        if overlay_was_running {
                            start_overlay().await;
                        }
                    } else if combo_string.eq_ignore_ascii_case(&keybindings.type_text) {
                        if !in_recording_mode {
                            println!(
                                "\n>>> Trigger: Started Recording ({}) on ws {}",
                                &combo_string, current_workspace
                            );
                            in_recording_mode = true;
                            key_buffer.clear();
                            log_snapshot_before_recording =
                                tokio::fs::read_to_string(&current_log_path)
                                    .await
                                    .unwrap_or_default();
                            if !log_snapshot_before_recording.is_empty()
                                && !log_snapshot_before_recording.ends_with('\n')
                            {
                                log_snapshot_before_recording.push('\n');
                            }
                            log_snapshot_before_recording.push('\n');
                            overwrite_text_log(&log_snapshot_before_recording, &current_log_path)
                                .await?;
                        }
                    } else if combo_string.eq_ignore_ascii_case(&keybindings.show_hide) {
                        println!("\n>>> Trigger: Toggle Overlay ({})", &combo_string);
                        if is_overlay_running().await {
                            stop_overlay().await;
                        } else {
                            start_overlay().await;
                        }
                    } else if combo_string.eq_ignore_ascii_case(&keybindings.clear) {
                        println!(
                            "\n>>> Trigger: Clear & Restart AI ({}) on ws {}",
                            &combo_string, current_workspace
                        );
                        clear_text_log(&current_log_path).await?;
                        if in_recording_mode {
                            key_buffer.clear();
                            log_snapshot_before_recording.clear();
                        }
                        if let Err(e) = ai_tx.send("/restart".to_string()).await {
                            eprintln!("Error sending restart command to AI manager: {}", e);
                        }
                    } else if combo_string.eq_ignore_ascii_case(&keybindings.solve) {
                        if in_recording_mode {
                            println!(
                                "\n>>> Trigger: Stopped Recording & Processing ({}) on ws {}",
                                &combo_string, current_workspace
                            );
                            in_recording_mode = false;
                            if !key_buffer.is_empty() {
                                let final_log_content = format!(
                                    "{}{}{}\n",
                                    log_snapshot_before_recording, key_buffer, "  "
                                );
                                overwrite_text_log(&final_log_content, &current_log_path).await?;
                                let command = format!("{}\n/ask", key_buffer.trim());
                                if let Err(e) = ai_tx.send(command).await {
                                    eprintln!("Error sending text buffer to AI manager: {}", e);
                                }
                                key_buffer.clear();
                            } else {
                                overwrite_text_log(
                                    &log_snapshot_before_recording,
                                    &current_log_path,
                                )
                                .await?;
                                println!("(Buffer was empty, sending standalone /ask)");
                                if let Err(e) = ai_tx.send("/ask".to_string()).await {
                                    eprintln!("Error sending '/ask' command to AI manager: {}", e);
                                }
                            }
                            log_snapshot_before_recording.clear();
                        } else {
                            println!(
                                "\n>>> Trigger: Standalone AI Ask ({}) on ws {}",
                                &combo_string, current_workspace
                            );
                            if let Err(e) = ai_tx.send("/ask".to_string()).await {
                                eprintln!("Error sending '/ask' command to AI manager: {}", e);
                            }
                        }
                    } else {
                        combo_matched = false;
                    }

                    if combo_matched {
                        active_modifiers.clear();
                    }
                } else if in_recording_mode {
                    let mut needs_update = true;
                    match trimmed_line {
                         "[Space]" => key_buffer.push(' '),
                        "[Backspace]" => {
                            key_buffer.pop();
                        }
                        s if s.starts_with('[') && s.ends_with(']') => {
                            needs_update = false;
                        }
                        _ => {
                            key_buffer.push_str(trimmed_line);
                        }
                    }

                    if needs_update {
                        let preview_content =
                            format!("{}{}", log_snapshot_before_recording, key_buffer);
                        overwrite_text_log(&preview_content, &current_log_path).await?;
                    }
                }
                active_modifiers.clear();
            }
        }
    }
    Ok(())
}

// --- Helper Functions ---

fn get_canonical_modifier(modifier: &str) -> &'static str {
    match modifier {
        "[CtrlL]" | "[CtrlR]" => "Ctrl",
        "[AltL]" | "[AltR]" => "Alt",
        "[ShiftL]" | "[ShiftR]" => "Shift",
        "[SuperL]" => "Super",
        _ => "",
    }
}

async fn start_overlay() {
    println!("Starting ./overlay...");
    match Command::new("./overlay").spawn() {
        Ok(_) => {
            println!("Successfully launched ./overlay.");
        }
        Err(e) => {
            eprintln!("Failed to start ./overlay: {}", e);
        }
    }
}

async fn stop_overlay() {
    println!("Stopping ./overlay...");
    match Command::new("pkill")
        .arg("-x")
        .arg("overlay")
        .status()
        .await
    {
        Ok(status) if status.success() => {
            println!("Successfully stopped ./overlay.");
        }
        Ok(_) => {
            println!("./overlay was already stopped or not found.");
        }
        Err(e) => {
            eprintln!(
                "Error executing pkill to stop overlay: {}. Is pkill installed?",
                e
            );
        }
    }
}

async fn is_overlay_running() -> bool {
    match Command::new("pgrep")
        .arg("-x")
        .arg("overlay")
        .output()
        .await
    {
        Ok(output) => output.status.success(),
        Err(e) => {
            eprintln!(
                "Could not check for overlay process: {}. Is pgrep installed?",
                e
            );
            false // Assume not running if we can't check
        }
    }
}

async fn take_screenshot(path: &Path) -> io::Result<()> {
    let output = Command::new("grim").arg(path.as_os_str()).output().await?;
    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("grim failed: {}", error_message),
        ))
    } else {
        Ok(())
    }
}

async fn read_new_content(path: &str, last_pos: &mut u64) -> io::Result<String> {
    let mut file = File::open(path).await?;
    let current_len = file.metadata().await?.len();
    if current_len == *last_pos {
        return Ok(String::new());
    }
    if current_len < *last_pos {
        *last_pos = 0;
    }
    file.seek(io::SeekFrom::Start(*last_pos)).await?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).await?;
    *last_pos = current_len;
    Ok(buffer)
}

async fn overwrite_text_log(content: &str, path: &str) -> io::Result<()> {
    tokio::fs::write(path, content.as_bytes()).await
}

async fn clear_text_log(path: &str) -> io::Result<()> {
    overwrite_text_log("", path).await
}
