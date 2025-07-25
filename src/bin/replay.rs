use rust_bot::command::BotClient;
use tokio::sync::mpsc;
use serde::Deserialize;
use std::future::pending;
use std::{collections::HashMap, env, fs, sync::Arc};

use rust_bot::basics::action::{Action, DrawAction, GameOverAction, PerformAction, TurnAction};
use rust_bot::basics::{card::Identity, game::Game, state::State, variant::VariantManager, util};
use rust_bot::console::{self, DebugCommand};
use rust_bot::{logger, reactor::Reactor};

struct Args {
	id: Option<usize>,
	index: usize,
	file: Option<String>
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
			let id = hash_map.get("id").and_then(|e| e.parse().ok());
			let index = hash_map["index"].parse().unwrap();
			let file = hash_map.get("file").cloned();

			if id.is_none() && file.is_none() {
				panic!("Must provide either id or file argument.");
			}

			Self { id, index, file }
		}
		else {
			panic!("Missing required argument 'index'!");
		}
	}
}

#[derive(Debug, Deserialize)]
struct ReplayOptions {
	variant: String
}

#[derive(Debug, Deserialize)]
struct GameData {
	players: Vec<String>,
	deck: Vec<Identity>,
	actions: Vec<PerformAction>,
	options: Option<ReplayOptions>,
}

impl GameData {
	async fn fetch(id: usize) -> Self {
		let data = reqwest::get(format!("https://hanab.live/export/{id}")).await.expect("Failed to fetch variants.")
			.text().await.expect("Failed to parse variants response.");
		serde_json::from_str(&data).expect("Failed to deserialize game data")
	}

	fn from_file(file: String) -> Self {
		let data = fs::read_to_string(file).expect("Failed to read file");
		serde_json::from_str(&data).expect("Failed to deserialize game data")
	}
}

#[tokio::main]
async fn main() {
	let args = env::args().collect::<Vec<String>>();
	let Args { id, index, file } = Args::parse(&args[1..]);
	let _ = logger::init();

	let GameData { players, deck, actions, options } = match id {
		Some(id) => GameData::fetch(id).await,
		None => GameData::from_file(file.unwrap())
	};

	if index >= players.len() {
		panic!("Replay only has {} players!", players.len());
	}

	let variant_manager = VariantManager::new().await;
	let variant = variant_manager.get_variant(&options.map(|opts| opts.variant).unwrap_or("No Variant".to_string()));

	let (debug_sender, mut debug_receiver) = mpsc::unbounded_channel::<DebugCommand>();
	console::spawn_console(debug_sender);

	let state = State::new(players, index, variant);
	let mut game = Game::new(0, state, false, Arc::new(Reactor));
	game.catchup = true;

	for player_index in 0..game.state.num_players {
		for _ in 0..game.state.hand_size() {
			let order = game.state.card_order;
			game.handle_action(&Action::Draw(DrawAction {
				player_index,
				order,
				suit_index: if player_index == index { -1 } else { deck[order].suit_index as i32 },
				rank: if player_index == index { -1 } else { deck[order].rank as i32 }
			}));
		}
	}

	for action in actions {
		let mut player_index = game.state.current_player_index;
		game.handle_action(&util::perform_to_action(&game.state, &action, player_index, Some(&deck)));

		if game.state.card_order < deck.len() {
			match action {
				PerformAction::Play { .. } | PerformAction::Discard { .. } => {
					let player_index = player_index;
					let order = game.state.card_order;

					game.handle_action(&Action::Draw(DrawAction {
						player_index,
						order,
						suit_index: if player_index == index { -1 } else { deck[order].suit_index as i32 },
						rank: if player_index == index { -1 } else { deck[order].rank as i32 }
					}));
				}
				_ => {}
			}
		}

		if let PerformAction::Play { .. } = action {
			if game.state.strikes == 3 {
				game.handle_action(&Action::GameOver(GameOverAction { player_index, end_condition: 0 }));
			}
		}

		player_index = game.state.next_player_index(game.state.current_player_index);
		game.handle_action(&Action::Turn(TurnAction { num: game.state.turn_count, current_player_index: player_index as i32 }));
	}

	game.catchup = false;

	// Receiver task
	tokio::spawn(async move {
		let variant_manager = VariantManager::new().await;
		let (sender, _) = mpsc::unbounded_channel::<String>();
		let mut client = BotClient::new(sender, variant_manager);
		client.game = Some(game);

		loop {
			tokio::select! {
				debug_cmd = debug_receiver.recv() => {
					match debug_cmd {
						Some(cmd) => {
							client.handle_debug_command(cmd);
						}
						None => {
							println!("Debug command channel closed.");
							break;
						}
					}
				}
			}
		}
		println!("Receiver task ending.");
	});

	pending::<()>().await;
}
