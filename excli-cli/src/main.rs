use std::process;

use excli_cli::cli::build_cli;
use excli_cli::commands::dispatch;
use excli_cli::output::format_error_json;

fn main() {
    let cli = build_cli();

    match dispatch(cli) {
        Ok(()) => {
            // success
        }
        Err(err) => {
            let use_emoji = true; // Could be derived from CLI args
            if use_emoji {
                eprintln!("❌ {}", err);
            } else {
                eprintln!("[ERROR] {}", err);
            }
            eprintln!("{}", format_error_json(&err));
            process::exit(err.code().code());
        }
    }
}
