use serde::{Deserialize, Serialize};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugCommand {
	Hand(String, Option<String>),
	Navigate(NavArg),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NavArg {
	PrevRound, Prev, Next, NextRound, Turn(usize)
}

impl DebugCommand {
	pub fn parse(input: &str) -> Option<Self> {
		let parts: Vec<&str> = input.split_whitespace().collect();

		if parts.len() < 2 {
			return None;
		}

		match parts[0].to_lowercase().as_str() {
			"hand" | "h" => Some(DebugCommand::Hand(parts[1].to_string(), parts.get(2).map(|s| s.to_owned().to_owned()))),
			"navigate" | "nav" => {
				let arg = match parts[1] {
					"++" => NavArg::NextRound,
					"+" => NavArg::Next,
					"--" => NavArg::PrevRound,
					"-" => NavArg::Prev,
					x => NavArg::Turn(x.parse().unwrap()),
				};
				Some(DebugCommand::Navigate(arg))
			},
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
