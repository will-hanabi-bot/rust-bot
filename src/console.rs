use serde::{Deserialize, Serialize};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugCommand {
    Hand(String),
    Navigate(usize),
}

impl DebugCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let parts: Vec<&str> = input.split_whitespace().collect();

        if parts.len() < 2 {
            return None;
        }

        match parts[0].to_lowercase().as_str() {
            "hand" | "h" => Some(DebugCommand::Hand(parts[1].to_string())),
            "navigate" | "nav" => Some(DebugCommand::Navigate(parts[1].parse().unwrap())),
            _ => None,
        }
    }
}

pub fn spawn_console(debug_sender: mpsc::UnboundedSender<DebugCommand>) {
    tokio::spawn(async move {
        let stdin = io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            match DebugCommand::parse(line) {
                Some(cmd) => {
                    if let Err(e) = debug_sender.send(cmd) {
                        eprintln!("Error: Debug channel closed {}", e);
                        break;
                    }
                }
                None => {
                    println!("Unknown command.");
                }
            }
        }
    });
}
