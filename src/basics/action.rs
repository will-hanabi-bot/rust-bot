use std::hash::Hasher;

use crate::basics::card::Identity;
use crate::basics::{clue::{Clue, ClueKind}, game::Game, state::State};
use crate::reactor::ClueInterp;

use super::clue::BaseClue;
use ahash::AHasher;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusAction {
	pub clues: usize,
	pub score: usize,
	pub max_score: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnAction {
	pub num: usize,
	pub current_player_index: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ClueAction {
	pub giver: usize,
	pub target: usize,
	pub list: Vec<usize>,
	pub clue: BaseClue
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32,
	pub failed: bool
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrikeAction {
	pub num: usize,
	pub turn: usize,
	pub order: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOverAction {
	pub end_condition: usize,
	pub player_index: usize
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InterpAction {
	pub interp: ClueInterp
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
	#[serde(rename = "status")]
	Status(StatusAction),
	#[serde(rename = "turn")]
	Turn(TurnAction),
	#[serde(rename = "clue")]
	Clue(ClueAction),
	#[serde(rename = "draw")]
	Draw(DrawAction),
	#[serde(rename = "play")]
	Play(PlayAction),
	#[serde(rename = "discard")]
	Discard(DiscardAction),
	#[serde(rename = "strike")]
	Strike(StrikeAction),
	#[serde(rename = "gameOver")]
	GameOver(GameOverAction),
	Interp(InterpAction)
}

impl Action {
	pub fn play(player_index: usize, order: usize, suit_index: i32, rank: i32) -> Self {
		Action::Play(PlayAction { player_index, order, suit_index, rank })
	}

	pub fn discard(player_index: usize, order: usize, suit_index: i32, rank: i32, failed: bool) -> Self {
		Action::Discard(DiscardAction { player_index, order, suit_index, rank, failed })
	}

	pub fn clue(giver: usize, target: usize, clue: BaseClue, list: Vec<usize>) -> Self {
		Action::Clue(ClueAction { giver, target, clue, list })
	}

	pub fn draw(player_index: usize, order: usize, suit_index: i32, rank: i32) -> Self {
		Action::Draw(DrawAction { player_index, order, suit_index, rank })
	}

	pub fn turn(num: usize, current_player_index: i32) -> Self {
		Action::Turn(TurnAction { num, current_player_index })
	}

	pub fn status(clues: usize, score: usize, max_score: usize) -> Self {
		Action::Status(StatusAction { clues, score, max_score })
	}

	pub fn game_over(end_condition: usize, player_index: usize) -> Self {
		Action::GameOver(GameOverAction { end_condition, player_index })
	}

	pub fn interp(interp: ClueInterp) -> Self {
		Action::Interp(InterpAction { interp })
	}

	pub fn hash(&self) -> u64 {
		let mut hasher = AHasher::default();

		match self {
			Action::Clue(ClueAction { giver, target, clue, .. }) => {
				hasher.write_u8(0);
				hasher.write_u32(*giver as u32);
				hasher.write_u32(*target as u32);
				hasher.write_u8(clue.kind as u8);
				hasher.write_u8(clue.value as u8);
			}
			Action::Play(PlayAction { player_index, suit_index, rank, order }) => {
				hasher.write_u8(1);
				hasher.write_u32(*player_index as u32);
				hasher.write_u32(*suit_index as u32);
				hasher.write_u8(*rank as u8);
				hasher.write_u8(*order as u8);
			}
			Action::Discard(DiscardAction { player_index, suit_index, rank, failed, order }) => {
				hasher.write_u8(2);
				hasher.write_u32(*player_index as u32);
				hasher.write_u32(*suit_index as u32);
				hasher.write_u8(*rank as u8);
				hasher.write_u8(*failed as u8);
				hasher.write_u8(*order as u8);
			}
			Action::Draw(DrawAction { player_index, suit_index, rank, order, .. }) => {
				hasher.write_u8(3);
				hasher.write_u32(*player_index as u32);
				hasher.write_u32(*suit_index as u32);
				hasher.write_u8(*rank as u8);
				hasher.write_u8(*order as u8);
			}
			Action::Turn(_) => (),
			Action::Status(_) => (),
			Action::GameOver(_) => (),
			Action::Strike(_) => (),
			Action::Interp(_) => ()
		};

		hasher.finish()
	}

	pub fn fmt(&self, state: &State) -> String {
		let log_id = |suit_index: &i32, rank: &i32|
			if *suit_index == -1 || *rank == -1 {
				"xx".to_string()
			} else {
				state.log_id(Identity { suit_index: *suit_index as usize, rank: *rank as usize })
			};

		match self {
			Action::Clue(ClueAction { giver, target, clue, .. }) => {
				let value = match clue.kind {
					ClueKind::COLOUR => state.variant.colourable_suits[clue.value].to_lowercase(),
					ClueKind::RANK => clue.value.to_string(),
				};
				format!("{} clues {} to {}", state.player_names[*giver], value, state.player_names[*target])
			}
			Action::Play(PlayAction { player_index, suit_index, rank, order }) => {
				format!("{} plays {} ({})", state.player_names[*player_index], log_id(suit_index, rank), order)
			}
			Action::Discard(DiscardAction { player_index, suit_index, rank, failed, order }) => {
				format!("{} {} {} ({})", state.player_names[*player_index], if *failed { "bombs" } else { "discards" }, log_id(suit_index, rank), order)
			}
			Action::Draw(DrawAction { player_index, suit_index, rank, order, .. }) => {
				format!("{} draws {} ({})", state.player_names[*player_index], log_id(suit_index, rank), order)
			}
			Action::Turn(TurnAction { num, current_player_index }) => {
				format!("Turn {} ({})", num, state.player_names[*current_player_index as usize])
			}
			Action::Status(StatusAction { clues, score, max_score }) => {
				format!("Status! Clues: {clues}, Score: {score}/{max_score}")
			}
			Action::GameOver(GameOverAction { .. }) => {
				"Game over!".to_string()
			}
			_ => { "".to_string() }
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PerformAction {
	Play { target: usize },
	Discard { target: usize },
	Colour { target: usize, value: usize },
	Rank { target: usize, value: usize },
	Terminate { target: usize, value: usize }
}

impl<'de> Deserialize<'de> for PerformAction {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = Value::deserialize(deserializer)?;
		match s {
			Value::Object(ref map) => {
				let target = map.get("target").unwrap().to_string().parse().unwrap();

				match map.get("type").unwrap() {
					Value::Number(n) => match n.as_u64().unwrap() {
						0 => Ok(PerformAction::Play { target }),
						1 => Ok(PerformAction::Discard { target }),
						2 => Ok(PerformAction::Colour { target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						3 => Ok(PerformAction::Rank { target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						4 => Ok(PerformAction::Terminate { target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						_ => panic!("Invalid action type {s:?}")
					},
					_ => panic!("Invalid action type {s:?}")
				}
			}
			_ => Err(serde::de::Error::unknown_variant(&s.to_string(), &["number"])),
		}
	}
}

impl PerformAction {
	pub fn is_clue(&self) -> bool {
		matches!(self, PerformAction::Colour { .. } | PerformAction::Rank { .. })
	}

	pub fn fmt(&self, game: &Game) -> String {
		let Game { common, state, .. } = game;

		match self {
			PerformAction::Play { target, .. } => {
				let slot = state.our_hand().iter().position(|o| o == target).unwrap() + 1;

				format!("Play slot {}, inferences {}", slot, common.str_infs(state, *target))
			}
			PerformAction::Discard { target, .. } => {
				let slot = state.our_hand().iter().position(|o| o == target).unwrap() + 1;
				format!("Discard slot {}, inferences {}", slot, common.str_infs(state, *target))
			}
			PerformAction::Colour { target, value, .. } => {
				(Clue { kind: ClueKind::COLOUR, value: *value, target: *target }).fmt(state)
			}
			PerformAction::Rank { target, value, .. } => {
				(Clue { kind: ClueKind::RANK, value: *value, target: *target }).fmt(state)
			}
			PerformAction::Terminate { target, value, .. } => {
				format!("Game ended: {target} {value}")
			}
		}
	}

	pub fn fmt_obj(&self, game: &Game, player_index: usize) -> String {
		let Game { state, deck_ids, .. }  = game;

		let action_type = match self {
			PerformAction::Play { target, .. } => {
				format!("play {}, order {}", state.log_oid(&deck_ids[*target]), target)
			}
			PerformAction::Discard { target, .. } => {
				format!("discard {}, order {}", state.log_oid(&deck_ids[*target]), target)
			}
			PerformAction::Colour { target, value, .. } => {
				(Clue { kind: ClueKind::COLOUR, value: *value, target: *target }).fmt(state)
			}
			PerformAction::Rank { target, value, .. } => {
				(Clue { kind: ClueKind::RANK, value: *value, target: *target }).fmt(state)
			}
			PerformAction::Terminate { target, value, .. } => {
				format!("Game ended: {target} {value}")
			}
		};

		format!("{} ({})", action_type, state.player_names[player_index])
	}

	pub fn json(&self, table_id: u32) -> Value {
		match self {
			PerformAction::Play { target } => 				json!({ "tableID": table_id, "type": 0, "target": target }),
			PerformAction::Discard { target } => 			json!({ "tableID": table_id, "type": 1, "target": target }),
			PerformAction::Colour { target, value } => 		json!({ "tableID": table_id, "type": 2, "target": target, "value": value }),
			PerformAction::Rank { target, value } => 		json!({ "tableID": table_id, "type": 3, "target": target, "value": value }),
			PerformAction::Terminate { target, value } => 	json!({ "tableID": table_id, "type": 4, "target": target, "value": value })
		}
	}
}
