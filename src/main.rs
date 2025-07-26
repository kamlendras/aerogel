use colored::*;
use std::fs::File;
use std::io::{self, Write};
use std::process::{exit, Command, Stdio};

fn main() {
    println!("Starting the event listener which requires root privileges.");
    io::stdout().flush().expect("Failed to flush stdout.");

    let listener_status = Command::new("sudo")
        .arg("-b")
        .arg("./event_listener")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to execute command. Is 'sudo' installed and in your PATH?");

    if !listener_status.success() {
        eprintln!(
            "\n{}",
            "Error: Failed to launch './event_listener' with root privileges.".red()
        );
        eprintln!("- Make sure './event_listener' exists in this directory and is executable.");
        eprintln!("- You may have entered the wrong password or cancelled the sudo prompt.");
        exit(1);
    }

    println!("\n{}", "'./event_listener' launched successfully.".green());

    let handler_log_file = File::create("aerogel.log").expect("Failed to create aerogel.log");

    match Command::new("./event_handler")
        .stdout(Stdio::from(
            handler_log_file
                .try_clone()
                .expect("Failed to clone handle"),
        ))
        .stderr(Stdio::from(handler_log_file))
        .spawn()
    {
        Ok(_) => {
            println!("{}", "'./event_handler' launched successfully.".green());
        }
        Err(e) => {
            eprintln!(
                "\n{}",
                format!("Error: Failed to launch './event_handler': {}", e).red()
            );
            eprintln!("- Make sure './event_handler' exists in this directory and is executable.");
            exit(1);
        }
    }
}
