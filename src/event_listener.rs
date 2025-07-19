mod os;
use clap::Parser;
use std::fs;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    logfile: String,
    #[clap(short, long)]
    timeout: u64,

    #[clap(short, long, default_value_t = 1)]
    count: u8,
}

fn main() {
    if let Err(e) = fs::File::create(".event") {
        eprintln!("Warning: Failed to create or clear .event file: {}", e);
    }
    let _res = os::start_eventlistener(".event".to_owned(), 0);
}