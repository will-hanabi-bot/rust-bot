use rust_bot::{logger, websocket, console, console::DebugCommand};
use std::{collections::HashMap, env, error::Error};
use futures::future::pending;
use tokio::sync::mpsc;

struct Args {
	index: usize
}

impl Args {
	fn parse(args: &[String]) -> Self {
		let mut hash_map = HashMap::new();

		for arg in args {
			let parts = arg.split('=').collect::<Vec<&str>>();

			if parts.len() != 2 {
				panic!("Invalid argument {arg}");
			}

			let key = parts[0];
			let value = parts[1];
			hash_map.insert(key.to_string(), value.to_string());
		}

		if hash_map.contains_key("index") {
			let index = hash_map["index"].parse().unwrap();
			Self { index }
		}
		else {
			Self { index: 1 }
		}
	}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	let _ = logger::init();
	let args = env::args().collect::<Vec<String>>();
	let Args { index } = Args::parse(&args[1..]);

	dotenvy::dotenv()?;

	let username = dotenvy::var(format!("HANABI_USERNAME{index}"))?;
	let password = dotenvy::var(format!("HANABI_PASSWORD{index}"))?;

	let params = [("username", username), ("password", password), ("version", "bot".to_string())];

	let client = reqwest::Client::new();
	let response = client.post("https://hanab.live:443/login")
		.header("Content-Type", "application/x-www-form-urlencoded")
		.form(&params)
		.send()
		.await?;

	let cookie = response.headers().get("set-cookie").expect("Failed to parse cookie").to_str().unwrap();

	let (debug_sender, debug_receiver) = mpsc::unbounded_channel::<DebugCommand>();
	console::spawn_console(debug_sender);

	websocket::connect(cookie, debug_receiver).await?;

	pending::<()>().await;
	Ok(())
}
