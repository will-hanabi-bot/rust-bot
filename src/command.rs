use log::{error, info, warn};
use serde::{Deserialize};
use serde_json::json;
use tokio::{spawn, sync::mpsc, time::sleep};
use std::sync::Arc;
use std::{collections::{HashMap, VecDeque}, time::Duration};

use crate::reactor::Reactor;
use crate::websocket::{send_chat, send_cmd, send_pm};
use crate::basics::{action::Action, game::Game, state::State, variant::VariantManager};
use crate::console::{DebugCommand, NavArg};

#[derive(Deserialize)]
struct ChatMessage {
	msg: String,
	who: String,
	room: String,
	recipient: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GameActionMessage {
	#[serde(rename="tableID")]
	table_id: u32,
	action: Action
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct Spectator {
	name: String,
	shadowing_player_index: i32,
	shadowing_player_username: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct Table {
	id: u32,
	name: String,
	password_protected: bool,
	joined: bool,
	owned: bool,
	running: bool,
	variant: String,
	options: TableOptions,
	shared_replay: bool,
	progress: u32,
	players: Vec<String>,
	spectators: Vec<Spectator>,
	max_players: usize,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct TableOptions {
	num_players: usize,
	starting_player: usize,
	variant_name: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct InitMessage {
	#[serde(rename="tableID")]
	table_id: u32,
	#[serde(rename="playerNames")]
	player_names: Vec<String>,
	#[serde(rename="ourPlayerIndex")]
	our_player_index: usize,
	replay: bool,
	seed: String,
	options: TableOptions,
}

struct Settings {
	convention: String,
}

#[derive(Clone, Deserialize)]
#[allow(dead_code)]
struct SelfData {
	#[serde(rename="userID")]
	user_id: u32,
	username: String,
	#[serde(rename="playingAtTables")]
	playing_at_tables: Vec<u32>,
	#[serde(rename="randomTableName")]
	random_table_name: String,
}

#[derive(Deserialize)]
struct TableIDMsg {
	#[serde(rename="tableID")]
	table_id: u32,
}

const CONVENTIONS: [&str; 1] = ["Reactor 1.0"];

pub struct BotClient {
	settings: Settings,
	info: Option<SelfData>,
	table_id: Option<u32>,
	pub game: Option<Game>,
	game_started: bool,
	last_sender: Option<String>,
	tables: HashMap<u32, Table>,
	ws: mpsc::UnboundedSender<String>,
	variant_manager: VariantManager,
}

impl BotClient {
	pub fn new(ws: mpsc::UnboundedSender<String>, variant_manager: VariantManager) -> Self {
		Self {
			settings: Settings { convention: CONVENTIONS[0].to_owned() },
			info: None,
			table_id: None,
			game: None,
			game_started: false,
			last_sender: None,
			tables: HashMap::new(),
			ws,
			variant_manager
		}
	}

	pub fn handle_debug_command(&mut self, command: DebugCommand) {
		match command {
			DebugCommand::Hand(player_name, from) => {
				if let Some(game) = &self.game {
					let state = &game.state;
					if let Some(hand) = state.player_names.iter().position(|name| *name == player_name).map(|i| &state.hands[i]) {
						let player = match &from {
							None => &game.common,
							Some(from_name) => match state.player_names.iter().position(|name| name == from_name) {
								None => {
									println!("Player {from_name} not found.");
									return;
								}
								Some(index) => &game.players[index]
							}
						};

						println!("viewing from {}", from.unwrap_or_else(|| "common".to_owned()));
						println!("====================");

						for &order in hand {
							let meta = &game.meta[order];
							let mut flags = Vec::new();
							if meta.focused {
								flags.push("focused");
							}
							if meta.trash {
								flags.push("trash");
							}
							if meta.urgent {
								flags.push("urgent");
							}
							if player.thoughts[order].reset {
								flags.push("reset");
							}

							println!("{}: {} {:?}", order, state.log_iden(&state.deck[order]), meta.status);
							println!("inferred: [{}]", player.str_infs(state, order));
							println!("possible: [{}]", player.str_poss(state, order));
							println!("reasoning: {:?}", meta.reasoning);
							if !flags.is_empty() {
								println!("flags: {flags:?}");
							}
							println!("====================");
						}
					}
					else {
						println!("Player {player_name} not found.");
					}
				} else {
					println!("No active game.");
				}
			}
			DebugCommand::Navigate(nav_arg) => {
				if let Some(game) = &mut self.game {
					if game.in_progress {
						warn!("Cannot navigate while game is in progress.");
					}
					else {
						let Game { state, .. } = game;

						let turn = match nav_arg {
							NavArg::Turn(turn) => turn,
							NavArg::NextRound => state.turn_count + state.num_players,
							NavArg::Next => state.turn_count + 1,
							NavArg::Prev => state.turn_count.saturating_sub(1),
							NavArg::PrevRound => state.turn_count.saturating_sub(state.num_players)
						};

						if turn < 1 || turn >= state.action_list.len() {
							error!("Turn {turn} does not exist.");
						}
						else {
							self.game = Some(game.navigate(turn));
						}
					}
				} else {
					println!("No active game.");
				}
			}
		}
	}

	fn assign_settings(&mut self, data: &ChatMessage, in_pm: bool) {
		let reply: Box<dyn Fn(&str)> = match in_pm {
			true => Box::new(|msg: &str| send_pm(&self.ws, &data.who, msg)),
			false => Box::new(|msg: &str| send_chat(&self.ws, &self.table_id.unwrap().to_string(), msg)),
		};

		reply(&format!("Currently playing with {} conventions.", self.settings.convention));
	}

	pub fn handle_msg(&mut self, data: String) {
		if let Some((command, args)) = data.split_once(" ") {
			// if command != "user" && command != "table" && command != "chat" {
			// 	println!("Command: {}, Args: {}", command, args);
			// }
			match command {
				"chat" => self.handle_chat(serde_json::from_str::<ChatMessage>(args).unwrap()),
				"gameAction" => {
					match serde_json::from_str::<GameActionMessage>(args) {
						Ok(action) => self.handle_action(action),
						Err(e) => println!("Error parsing game action: {e:?}"),
					}
				},
				"gameActionList" => {
					#[derive(Deserialize)]
					struct GameActionListMessage {
						#[serde(rename="tableID")]
						table_id: u32,
						list: VecDeque<Action>,
					}

					let GameActionListMessage { table_id, mut list } = serde_json::from_str::<GameActionListMessage>(args).unwrap();

					self.game.as_mut().unwrap().catchup = true;
					for _ in 0..list.len() - 1 {
						self.handle_action(GameActionMessage { table_id, action: list.pop_front().unwrap() })
					}
					self.game.as_mut().unwrap().catchup = false;
					self.handle_action(GameActionMessage { table_id, action: list.pop_front().unwrap() });

					send_cmd(&self.ws, "loaded", &json!({ "tableID": table_id }).to_string());
				},
				"joined" => {
					let TableIDMsg { table_id } = serde_json::from_str::<TableIDMsg>(args).unwrap();
					self.table_id = Some(table_id);
					self.game_started = false;
				},
				"init" => self.handle_init(serde_json::from_str::<InitMessage>(args).unwrap()),
				"left" => {
					self.table_id = None;
					self.game_started = false;
				},
				"table" => {
					let table = serde_json::from_str::<Table>(args).unwrap();
					self.tables.insert(table.id, table);
				},
				"tableGone" => {
					let json = serde_json::from_str::<TableIDMsg>(args).unwrap();
					self.tables.remove(&json.table_id);
				},
				"tableList" => {
					for table in serde_json::from_str::<Vec<Table>>(args).unwrap() {
						self.tables.insert(table.id, table);
					}
				},
				"tableStart" => {
					let json = serde_json::from_str::<TableIDMsg>(args).unwrap();

					send_cmd(&self.ws, "getGameInfo1", &json!({ "tableID": json.table_id }).to_string());
				},
				"warning" => {
					eprintln!("{args}");
				},
				"welcome" => {
					let info = serde_json::from_str::<SelfData>(args).unwrap();
					self.info = Some(info);
				},
				_ => {}
			}
		}
	}

	pub fn leave_room(&mut self) {
		let cmd = if self.game_started { "tableUnattend" } else { "tableLeave" };
		send_cmd(&self.ws, cmd, &json!({ "tableID": self.table_id }).to_string());

		self.table_id = None;
		self.game = None;
		self.game_started = false;
	}

	fn handle_init(&mut self, data: InitMessage) {
		let InitMessage { table_id, player_names, our_player_index, options, .. } = data;
		let variant = self.variant_manager.get_variant(&options.variant_name);
		let state = State::new(player_names, our_player_index, Arc::new(variant.clone()));

		self.table_id = Some(table_id);
		self.game = Some(Game::new(table_id, state, true, Arc::new(Reactor)));
		send_cmd(&self.ws, "getGameInfo2", &json!({ "tableID": self.table_id }).to_string());
	}

	fn handle_chat(&mut self, data: ChatMessage) {
		let ChatMessage { msg, recipient, room, who } = &data;
		let within_room = recipient.is_empty() && room.starts_with("table");

		if within_room {
			if msg.starts_with("/setall") {
				self.assign_settings(&data, false);
			}
			else if msg.starts_with("/leaveall") {
				self.leave_room();
			}
			return;
		}

		if recipient != &self.info.as_ref().unwrap().username {
			return;
		}

		if msg.starts_with("/join") {
			let table = self.tables.values().filter(|table|
					(table.players.contains(who) && !table.shared_replay) ||
					table.spectators.iter().any(|spectator| spectator.name == *who))
				.max_by_key(|table| table.id);

			match table {
				Some(table) => {
					if table.password_protected {
						let password = msg.split_whitespace().nth(1);
						match password {
							Some(password) =>
								send_cmd(&self.ws, "tableJoin", &json!({ "tableID": table.id, "password": password }).to_string()),
							None =>
								send_pm(&self.ws, who, "Room is password protected, please provide a password.")
						}
					} else {
						send_cmd(&self.ws, "tableJoin", &json!({ "tableID": table.id }).to_string())
					}
				}
				None => send_pm(&self.ws, who, "Could not join, as you are not in a room.")
			}
			return;
		}

		if msg.starts_with("/rejoin") {
			if self.game.is_some() {
				send_pm(&self.ws, who, "Could not rejoin, as the bot is already in a game.");
			}

			let table = &self.tables.values().filter(|table|
					table.players.contains(&self.info.as_ref().unwrap().username))
				.max_by_key(|table| table.id);

			match table {
				Some(table) => send_cmd(&self.ws, "tableReattend", &json!({ "tableID": table.id }).to_string()),
				None => send_pm(&self.ws, who, "Could not rejoin, as the bot is not a player in any currently open room.")
			}
			return;
		}

		if msg.starts_with("/version") {
			send_pm(&self.ws, who, "v0.7.0 (rust-bot)");
		}
	}

	pub fn handle_action(&mut self, data: GameActionMessage) {
		let GameActionMessage { action, .. } = data;
		if let Some(game) = &mut self.game {
			game.handle_action(&action);

			for (cmd, arg) in &game.queued_cmds {
				send_cmd(&self.ws, cmd, arg);
			}

			game.queued_cmds.clear();

			let Game { state, table_id, .. } = &game;
			let perform = !game.catchup && state.current_player_index == state.our_player_index &&
				!state.ended() &&
				match action {
					Action::Turn { .. } => true,
					Action::Draw { .. } => state.turn_count == 1,
					_ => false
				};

			if perform {
				let suggested_action = game.take_action();
				info!("Suggested action: {}", suggested_action.fmt(game));

				if game.in_progress {
					let ws = self.ws.clone();
					let arg = suggested_action.json(*table_id).to_string();

					spawn(async move {
						sleep(Duration::from_secs(2)).await;
						send_cmd(&ws, "action", &arg);
					});
				}
			}
		}
	}
}
