use colored::Colorize;
use itertools::Itertools;
use log::{info, LevelFilter};
use serde_json::json;
use std::collections::{HashMap};
use std::sync::Arc;

use super::action::{Action, ClueAction, DiscardAction, PerformAction, PlayAction, TurnAction};
use crate::basics::card::{CardStatus, ConvData};
use crate::basics::identity_set::IdentitySet;
use crate::basics::{self, on_draw};
use crate::basics::player::Link;
use super::card::{Identifiable, Identity};
use super::player::Player;
use crate::reactor::ReactorInterp;
use super::state::State;
use super::variant::all_ids;
use self::frame::Frame;

pub mod frame;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Interp {
	Reactor(ReactorInterp),
}

#[derive(Debug, Clone, Default)]
pub struct Note {
	pub turn: usize,
	pub last: String,
	pub full: String,
}

#[derive(Clone)]
pub struct Game {
	pub table_id: u32,
	pub state: State,
	pub players: Vec<Player>,
	pub common: Player,
	pub meta: Vec<ConvData>,
	pub base: (State, Vec<Player>, Player),
	pub in_progress: bool,
	pub catchup: bool,
	pub convention: Arc<dyn Convention + Send + Sync>,
	pub notes: HashMap<usize, Note>,
	pub last_move: Option<Interp>,
	pub queued_cmds: Vec<(String, String)>,
	pub history: Vec<Game>,
}

const HAND_SIZE: [usize; 7] = [0, 0, 5, 5, 4, 4, 3];

impl Game {
	pub fn new(table_id: u32, state: State, in_progress: bool, convention: Arc<dyn Convention + Send + Sync>) -> Self {
		let num_players = state.num_players;
		let all_possible = IdentitySet::from_iter(all_ids(&state.variant));
		let hypo_stacks = vec![0; state.variant.suits.len()];

		let players: Vec<Player> = (0..num_players)
			.map(|i| Player::new(Some(i), all_possible, hypo_stacks.clone()))
			.collect();
		let common = Player::new(None, all_possible, hypo_stacks);

		Self {
			table_id,
			state: state.clone(),
			players: players.clone(),
			common: common.clone(),
			meta: Vec::new(),
			base: (state, players, common),
			in_progress,
			catchup: false,
			convention,
			notes: HashMap::new(),
			last_move: None,
			queued_cmds: Vec::new(),
			history: Vec::new(),
		}
	}

	pub fn hash(&self) -> String {
		let state = self.state.hash();
		let hash_player = |player: &Player| {
			(0..player.thoughts.len()).map(|i| player.str_infs(&self.state, i)).join(",")
		};
		let player_thoughts = self.players.iter().map(hash_player).join(",");
		let common_thoughts = hash_player(&self.common);
		let action_list = self.state.action_list.iter().map(|action| format!("{action:?}")).join(",");

		format!("{state},{player_thoughts},{common_thoughts},{action_list}")
	}

	pub fn frame(&self) -> Frame {
		Frame::new(&self.state, &self.meta)
	}

	pub fn me(&self) -> &Player {
		&self.players[self.state.current_player_index]
	}

	pub fn me_mut(&mut self) -> &mut Player {
		&mut self.players[self.state.current_player_index]
	}

	pub fn handle_action(&mut self, action: &Action) {
		let prev = &self.clone();
		self.state.action_list.push(action.clone());
		match action {
			Action::Clue(clue) => {
				info!("{}", format!("Turn {}: {}", self.state.turn_count, action.fmt(&self.state)).yellow());
				self.handle_clue(prev, clue);

				for order in &clue.list {
					self.state.deck[*order].newly_clued = false;
				}
			}
			Action::Discard(discard) => {
				info!("{}", format!("Turn {}: {}", self.state.turn_count, action.fmt(&self.state)).yellow());

				basics::on_discard(self, discard);
				Arc::clone(&self.convention).interpret_discard(prev, self, discard);
			},
			Action::Play(play) => {
				info!("{}", format!("Turn {}: {}", self.state.turn_count, action.fmt(&self.state)).yellow());

				basics::on_play(self, play);
				Arc::clone(&self.convention).interpret_play(prev, self, play);
			},
			Action::Draw(draw) => {
				on_draw(self, draw);

				if self.state.turn_count == 0 && self.state.hands.iter().all(|hand| hand.len() == HAND_SIZE[self.state.num_players]) {
					self.state.turn_count += 1;
				}
			},
			Action::GameOver(_) => {
				self.in_progress = false;
				info!("Game over!");
			}
			Action::Turn(turn) => {
				let TurnAction { num, current_player_index } = turn;

				if *current_player_index >= 0 {
					self.state.current_player_index = *current_player_index as usize;
				}
				self.state.turn_count = num + 1;

				Arc::clone(&self.convention).update_turn(prev, self, turn);
				self.update_notes();
			},
			_ => (),
		}
	}

	pub fn handle_clue(&mut self, copy: &Game, action: &ClueAction) {
		basics::on_clue(self, action);
		basics::elim(self, true);
		Arc::clone(&self.convention).interpret_clue(copy, self, action);
	}

	pub fn take_action(&self) -> PerformAction {
		self.convention.take_action(self)
	}

	pub fn simulate_clean(&self) -> Self {
		let mut hypo_game = self.clone();
		hypo_game.catchup = true;
		// Don't rewind
		for hand in &self.state.hands {
			for &order in hand {
				hypo_game.state.deck[order].newly_clued = false;
			}
		}
		hypo_game
	}

	pub fn simulate_clue(&self, action: &ClueAction) -> Self {
		// let level = log::max_level();
		// log::set_max_level(LevelFilter::Off);

		let mut hypo_game = self.simulate_clean();
		hypo_game.handle_clue(self, action);

		// log::set_max_level(level);

		hypo_game.catchup = false;
		hypo_game.state.turn_count += 1;
		hypo_game
	}

	pub fn simulate_action(&self, action: &Action) -> Self {
		let level = log::max_level();
		log::set_max_level(LevelFilter::Off);

		let mut hypo_game = self.simulate_clean();
		hypo_game.handle_action(action);

		match action {
			Action::Play(PlayAction { player_index, .. }) |
			Action::Discard(DiscardAction { player_index, .. }) => {
				hypo_game.handle_action(&Action::turn(hypo_game.state.turn_count, *player_index as i32));

				if hypo_game.state.cards_left > 0 {
					let order = hypo_game.state.card_order;
					match hypo_game.state.deck.get(order).and_then(|card| card.id()) {
						Some(Identity { suit_index, rank }) => {
							hypo_game.handle_action(&Action::draw(
								*player_index,
								order,
								suit_index as i32,
								rank as i32,
							));
						}
						None => {
							hypo_game.handle_action(&Action::draw( *player_index, order, -1, -1));
						}
					}
				}
			}
			_ => {}
		}

		log::set_max_level(level);

		hypo_game.catchup = false;
		hypo_game
	}

	pub fn navigate(&mut self, turn: usize) -> Self {
		info!("{}", format!("------- NAVIGATING (turn {turn}) -------").green());

		let (state, players, common) = &self.base;
		let mut new_game = Game::new(self.table_id, state.clone(), self.in_progress, Arc::clone(&self.convention));
		new_game.players = players.clone();
		new_game.common = common.clone();

		let actions = &self.state.action_list;

		if turn == 1 && state.our_player_index == 0 {
			let mut action = &actions[new_game.state.action_list.len()];

			while let Action::Draw(_) = action {
				new_game.handle_action(action);
				action = &actions[new_game.state.action_list.len()]
			}
		}
		else {
			let level = log::max_level();
			log::set_max_level(LevelFilter::Off);

			while new_game.state.turn_count < turn - 1 {
				new_game.handle_action(&actions[new_game.state.action_list.len()]);
			}

			log::set_max_level(level);

			while let Some(action) = actions.get(new_game.state.action_list.len()) {
				if new_game.state.turn_count < turn {
					new_game.handle_action(action);
				}
				else {
					break;
				}
			}
		}

		new_game.catchup = self.catchup;

		if !new_game.catchup && new_game.state.current_player_index == state.our_player_index {
			let perform = new_game.take_action();
			info!("{}", format!("Suggested action: {}", perform.fmt(&new_game)).blue());
		}
		new_game.state.action_list = actions.clone();
		new_game
	}

	pub fn update_notes(&mut self) {
		let Game { common, state, meta, notes, .. } = self;

		for order in state.hands.concat() {
			let frame = Frame::new(state, meta);
			let card = &state.deck[order];
			let meta = &meta[order];

			if !card.clued && meta.status == CardStatus::None {
				continue;
			}

			let mut note: String = frame.get_note(common, order);
			let link_note = common.links.iter().filter_map(|link| match link {
				Link::Promised { orders, id, .. } => orders.contains(&order).then_some(state.log_id(*id)),
				_ => None,
			}).join("? ");

			if !link_note.is_empty() {
				if note.contains("]") {
					note.push('?');
				}
				else {
					note = format!("[{note}] {link_note}?");
				}
			}

			let prev_note = notes.get(&order);
			let write_note = match prev_note {
				Some(prev_note) => note != prev_note.last && state.turn_count > prev_note.turn,
				None => true
			};

			if write_note {
				let prev_note = notes.remove(&order);
				let mut full = prev_note.map(|n| format!("{} | ", n.full)).unwrap_or_else(|| "".to_owned());
				full.push_str(&format!("t{}: {note}", state.turn_count));
				let new_note = Note {
					last: note,
					turn: state.turn_count,
					full: full.to_string()
				};
				notes.insert(order, new_note);

				if !self.catchup && self.in_progress {
					self.queued_cmds.push((
						"note".to_string(),
						json!({ "tableID": self.table_id, "order": order, "note": full }).to_string()
					));
				}
			}
		}
	}
}

pub trait Convention {
	fn interpret_clue(&self, prev: &Game, game: &mut Game, action: &ClueAction);
	fn interpret_discard(&self, prev: &Game, game: &mut Game, action: &DiscardAction);
	fn interpret_play(&self, prev: &Game, game: &mut Game, action: &PlayAction);
	fn take_action(&self, game: &Game) -> PerformAction;
	fn update_turn(&self, prev: &Game, game: &mut Game, action: &TurnAction);

	fn find_all_clues(&self, game: &Game, player_index: usize) -> Vec<PerformAction>;
	fn find_all_discards(&self, game: &Game, player_index: usize) -> Vec<PerformAction>;
}
