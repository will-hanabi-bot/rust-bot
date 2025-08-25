use log::LevelFilter;
use serde_json::json;
use std::{collections::HashMap, env, fs, sync::Arc};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rand::SeedableRng;

use rust_bot::basics::action::{Action, DrawAction, PerformAction, TurnAction};
use rust_bot::basics::{card::Identity, game::Game, state::State, util};
use rust_bot::basics::variant::{all_ids, card_count, Variant, VariantManager};
use rust_bot::logger;
use rust_bot::reactor::Reactor;

struct Args {
	num_games: usize,
	seed: usize,
	variant: String
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

		let num_games = hash_map.get("games").and_then(|e| e.parse().ok()).unwrap_or(1);
		let seed = hash_map.get("seed").and_then(|e| e.parse().ok()).unwrap_or(0);
		let variant = hash_map.get("variant").and_then(|e| e.parse().ok()).unwrap_or("No Variant".to_owned());
		Self { num_games, seed, variant }
	}
}

#[derive(Debug)]
enum GameResult {
	Perfect, Strikeout, DiscardedCrit, OutOfPace
}

struct GameSummary {
	score: usize,
	result: GameResult,
	actions: Vec<PerformAction>,
	notes: Vec<Vec<String>>
}

fn simulate_game(deck: &[Identity], variant: &Variant) -> GameSummary {
	let mut games = Vec::new();

	for i in 0..3 {
		let player_names = vec!["Alice".to_string(), "Bob".to_string(), "Cathy".to_string()];
		let state = State::new(player_names, i, Arc::new(variant.clone()));
		let mut game = Game::new(0, state, false, Arc::new(Reactor));
		game.catchup = true;

		for player_index in 0..game.state.num_players {
			for _ in 0..game.state.hand_size() {
				let order = game.state.card_order;
				game.handle_action(&Action::Draw(DrawAction {
					player_index,
					order,
					suit_index: if player_index == i { -1 } else { deck[order].suit_index as i32 },
					rank: if player_index == i { -1 } else { deck[order].rank as i32 }
				}));
			}
		}
		games.push(game);
	}

	let mut actions = Vec::new();

	while !games[0].state.ended() {
		let current_player_index = games[0].state.current_player_index;
		let current_game = &games[current_player_index];
		let perform = current_game.take_action();
		actions.push(perform);

		for game in &mut games {
			let Game { state, .. } = game;
			let action = util::perform_to_action(state, &perform, current_player_index, Some(deck));

			game.handle_action(&action);

			if game.state.ended() {
				break;
			}

			if game.state.card_order < deck.len() {
				match perform {
					PerformAction::Play { .. } | PerformAction::Discard { .. } => {
						let player_index = current_player_index;
						let order = game.state.card_order;

						game.handle_action(&Action::Draw(DrawAction {
							player_index,
							order,
							suit_index: if player_index == game.state.our_player_index { -1 } else { deck[order].suit_index as i32 },
							rank: if player_index == game.state.our_player_index { -1 } else { deck[order].rank as i32 }
						}));
					}
					_ => {}
				}
			}

			game.handle_action(&Action::Turn(TurnAction {
				num: game.state.turn_count,
				current_player_index: game.state.next_player_index(current_player_index) as i32 }));
		}
	}

	let target = games[0].state.last_player_index(games[0].state.current_player_index);
	actions.push(PerformAction::Terminate {  target, value: 0 });

	let State { strikes, max_ranks, .. } = &games[0].state;

	let result = if *strikes == 3 {
		GameResult::Strikeout
	} else if games[0].state.score() == variant.suits.len() * 5 {
		GameResult::Perfect
	} else if max_ranks.iter().any(|max| *max != 5) {
		GameResult::DiscardedCrit
	} else {
		GameResult::OutOfPace
	};

	GameSummary {
		actions,
		score: games[0].state.score(),
		result,
		notes: games.iter().map(|game| (0..game.state.card_order).map(|i|
			game.notes.get(&(i as u64)).map_or("".to_owned(), |note| note.full.to_owned())).collect()
		).collect()
	}
}

#[tokio::main]
async fn main() {
	let args = env::args().collect::<Vec<String>>();
	let Args { num_games, seed, variant } = Args::parse(&args[1..]);
	let _ = logger::init();
	log::set_max_level(LevelFilter::Error);

	let mut variant_manager = VariantManager::new().await;
	let variant = variant_manager.get_variant(&variant);

	let deck = all_ids(&variant).flat_map(|i| vec![i; card_count(&variant, i)]).collect::<Vec<_>>();

	for i in seed..(seed+num_games) {
		let mut rng = ChaCha8Rng::seed_from_u64(i as u64);
		let mut seeded_deck = deck.clone();
		seeded_deck.shuffle(&mut rng);

		let GameSummary { score, result, actions, notes } = simulate_game(&seeded_deck, &variant);

		let actions_json = actions.iter().map(|a| a.json(0)).collect::<Vec<_>>();

		let data = json!({
			"players": ["Alice", "Bob", "Cathy"],
			"deck": seeded_deck,
			"actions": actions_json,
			"notes": notes,
			"options": { "variant": variant.name }
		}).to_string();
		if let Err(e) = fs::create_dir_all("seeds") {
			log::error!("Could not create seeds/ directory: {e:?}");
		}
		fs::write(format!("seeds/{i}.json"), data).unwrap_or_else(|_| panic!("Should be able to write to `seeds/{i}.json`"));

		println!("Seed {i}: Score: {score}, Result: {result:?}");
	}

	std::process::exit(0);
}
