use chrono::Local;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::io;
use std::path::Path;
use std::process::Stdio;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

const LOG_FILE_TO_WATCH: &str = ".event";
const TEXT_LOG_OUTPUT: &str = ".temp";
const AI_EXECUTABLE: &str = "./ai_manager";
const SCREENSHOT_DIR: &str = "screenshots";

async fn manage_ai_process(mut command_rx: mpsc::Receiver<String>) {
    loop {
        println!("[AI Manager] Spawning './ai_manager' process...");
        let mut child = match Command::new(AI_EXECUTABLE)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!(
                    "[AI Manager] CRITICAL: Failed to spawn './ai_manager': {}. Retrying in 10s.",
                    e
                );
                sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        // Ensure we have handles to stdin and stdout
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

        let print_task = tokio::spawn(async move {
            let mut line_buf = String::new();
            loop {
                match stdout.read_line(&mut line_buf).await {
                    Ok(0) => break,
                    Ok(_) => {
                        print!("[AI] {}", line_buf);
                        line_buf.clear();
                    }
                    Err(e) => {
                        eprintln!("[AI] Error reading stdout: {}", e);
                        break;
                    }
                }
            }
        });

        loop {
            tokio::select! {
                Some(command) = command_rx.recv() => {
                    println!("[AI Manager] Sending command: {}", command.lines().next().unwrap_or(""));
                    let command_with_newline = format!("{}\n", command);
                    if let Err(e) = stdin.write_all(command_with_newline.as_bytes()).await {
                        eprintln!("[AI Manager] Error writing to AI stdin: {}. Process may have died.", e);
                        break;
                    }
                    if let Err(e) = stdin.flush().await {
                         eprintln!("[AI Manager] Error flushing AI stdin: {}. Process may have died.", e);
                         break;
                    }
                }
                status = child.wait() => {
                    eprintln!("[AI Manager] AI process exited with status: {:?}", status);
                    break;
                }
            }
        }

        print_task.abort();
        println!("[AI Manager] AI process has terminated. Will attempt to respawn.");
        sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::create_dir_all(SCREENSHOT_DIR).await?;

    // Start overlay on startup
    start_overlay().await;

    let (ai_tx, ai_rx) = mpsc::channel::<String>(32);
    tokio::spawn(manage_ai_process(ai_rx));

    println!("Starting log watcher...");
    let log_path_str = LOG_FILE_TO_WATCH;

    if !Path::new(log_path_str).exists() {
        File::create(log_path_str).await?.shutdown().await?;
        println!("Created log file '{}'.", log_path_str);
    }
    let mut file_pos = std::fs::metadata(log_path_str)?.len();
    println!("Watching '{}' for new entries.", log_path_str);
    println!("Press Ctrl+H to take a screenshot and upload.");
    println!("Press Ctrl+\\ to toggle the overlay.");
    println!("Press Ctrl+Return to ask AI about the last context.");
    println!("Press Ctrl+W to START text recording (with real-time preview).");
    println!("Press Ctrl+G to CLEAR the text log.");

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
    let mut last_key_was_lctrl = false;
    let mut key_buffer = String::new();
    let mut log_snapshot_before_recording = String::new();

    while let Some(_) = rx.recv().await {
        let new_content = read_new_content(log_path_str, &mut file_pos).await?;
        for line in new_content.lines() {
            let trimmed_line = line.trim();
            if trimmed_line.is_empty() {
                continue;
            }

            if last_key_was_lctrl {
                last_key_was_lctrl = false;
                match trimmed_line {
                    "h" => {
                        println!("\n>>> Trigger: Screenshot (Ctrl+H)");

                        // Check if the overlay is running and stop it temporarily
                        let overlay_was_running = is_overlay_running().await;
                        if overlay_was_running {
                            stop_overlay().await;
                            // Give the system a moment to remove the overlay window
                            sleep(Duration::from_millis(0)).await;
                        }

                        let timestamp = Local::now().format("%Y%m%d-%H%M%S");
                        let filename = format!("screenshot-{}.jpeg", timestamp);
                        let path = Path::new(SCREENSHOT_DIR).join(filename);
                        let screenshot_result = take_screenshot(&path).await;

                        // Restart the overlay if it was running before
                        if overlay_was_running {
                            println!("    Restarting overlay...");
                            start_overlay().await;
                        }

                        if screenshot_result.is_ok() {
                            println!("    Screenshot saved to '{}'", path.display());
                            let command = format!("/upload {}", path.display());
                            if let Err(e) = ai_tx.send(command).await {
                                eprintln!("Error sending upload command to AI manager: {}", e);
                            }
                        } else {
                            eprintln!("    Error taking screenshot. Is 'grim' installed?");
                        }
                    }

                    "i" => {
                        if !in_recording_mode {
                            println!("\n>>> Trigger: Started Recording (Ctrl+W)");
                            in_recording_mode = true;
                            key_buffer.clear();

                            log_snapshot_before_recording =
                                tokio::fs::read_to_string(TEXT_LOG_OUTPUT)
                                    .await
                                    .unwrap_or_default();
                            if !log_snapshot_before_recording.is_empty()
                                && !log_snapshot_before_recording.ends_with('\n')
                            {
                                log_snapshot_before_recording.push('\n');
                            }
                            log_snapshot_before_recording.push('\n');

                            overwrite_text_log(&log_snapshot_before_recording).await?;
                        }
                    }
                    "\\" => {
                        println!("\n>>> Trigger: Toggle Overlay (Ctrl+\\)");
                        if is_overlay_running().await {
                            stop_overlay().await;
                        } else {
                            start_overlay().await;
                        }
                    }
                    "g" => {
                        println!("\n>>> Trigger: Cleared Text Log (Ctrl+G)");
                        clear_text_log().await?;
                        if in_recording_mode {
                            key_buffer.clear();
                            // The snapshot is now invalid, so clear it too.
                            log_snapshot_before_recording.clear();
                        }
                    }

                    "[retun]" => {
                        if in_recording_mode {
                            println!("\n>>> Trigger: Stopped Recording & Processing");
                            in_recording_mode = false;

                            if !key_buffer.is_empty() {
                                let final_entry_text = format!("{}  \n", key_buffer);
                                let final_log_content = format!(
                                    "{}{}",
                                    log_snapshot_before_recording, final_entry_text
                                );
                                overwrite_text_log(&final_log_content).await?;

                                // Send the command to the AI Manager
                                let final_text_for_ai = key_buffer.trim().to_string();
                                let command = format!("{}\n/ask", final_text_for_ai);
                                if let Err(e) = ai_tx.send(command).await {
                                    eprintln!("Error sending text buffer to AI manager: {}", e);
                                }
                                key_buffer.clear();
                                log_snapshot_before_recording.clear();
                            } else {
                                // If buffer was empty, just restore the log to its pre-recording state.
                                overwrite_text_log(&log_snapshot_before_recording).await?;
                                println!("    (Buffer was empty, sending standalone /ask)");
                                if let Err(e) = ai_tx.send("/ask".to_string()).await {
                                    eprintln!("Error sending '/ask' command to AI manager: {}", e);
                                }
                            }
                        } else {
                            println!("\n>>> Trigger: Standalone AI Ask (Ctrl+Return)");
                            if let Err(e) = ai_tx.send("/ask".to_string()).await {
                                eprintln!("Error sending '/ask' command to AI manager: {}", e);
                            }
                        }
                    }
                    _ => { /* Other Ctrl combos are ignored */ }
                }
            } else if trimmed_line == "[Lctrl]" {
                last_key_was_lctrl = true;
            } else if in_recording_mode {
                let mut needs_update = true;
                match trimmed_line {
                    "[space]" => key_buffer.push(' '),
                    "[backspace]" => {
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
                    overwrite_text_log(&preview_content).await?;
                }
            }
        }
        sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

async fn start_overlay() {
    println!("    Starting ./overlay...");
    match Command::new("./overlay").spawn() {
        Ok(_) => {
            println!("    Successfully launched ./overlay.");
        }
        Err(e) => {
            eprintln!("    Failed to start ./overlay: {}", e);
        }
    }
}

async fn stop_overlay() {
    println!("    Stopping ./overlay...");
    match Command::new("pkill")
        .arg("-x")
        .arg("overlay")
        .status()
        .await
    {
        Ok(status) if status.success() => {
            println!("    Successfully stopped ./overlay.");
        }
        Ok(_) => {
            println!("    ./overlay was already stopped or not found.");
        }
        Err(e) => {
            eprintln!(
                "    Error executing pkill to stop overlay: {}. Is pkill installed?",
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
                "    Could not check for overlay process: {}. Is pgrep installed?",
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

async fn overwrite_text_log(content: &str) -> io::Result<()> {
    tokio::fs::write(TEXT_LOG_OUTPUT, content.as_bytes()).await
}

async fn clear_text_log() -> io::Result<()> {
    overwrite_text_log("").await
}
