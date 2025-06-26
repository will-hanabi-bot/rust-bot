use crate::basics::{card::Identity, clue::{Clue, ClueKind}, game::Game, state::State, variant::colourable_suits};

use super::clue::BaseClue;
use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusAction {
	pub clues: usize,
	pub score: usize,
	pub max_score: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnAction {
	pub num: usize,
	pub current_player_index: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClueAction {
	pub giver: usize,
	pub target: usize,
	pub list: Vec<usize>,
	pub clue: BaseClue
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardAction {
	pub player_index: usize,
	pub order: usize,
	pub suit_index: i32,
	pub rank: i32,
	pub failed: bool
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrikeAction {
	pub num: usize,
	pub turn: usize,
	pub order: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOverAction {
	pub end_condition: usize,
	pub player_index: usize
}

#[derive(Debug, Clone, Deserialize)]
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
	GameOver(GameOverAction)
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

	pub fn fmt(&self, state: &State) -> String {
		match self {
			Action::Clue(ClueAction { giver, target, clue, .. }) => {
				let value = match clue.kind {
					ClueKind::COLOUR => colourable_suits(&state.variant)[clue.value].to_lowercase(),
					ClueKind::RANK => clue.value.to_string(),
				};
				format!("{} clues {} to {}", state.player_names[*giver], value, state.player_names[*target])
			}
			Action::Play(PlayAction { player_index, suit_index, rank, .. }) => {
				let id = if *suit_index == -1 || *rank == -1 {
					"xx".to_string()
				} else {
					(Identity { suit_index: *suit_index as usize, rank: *rank as usize }).fmt(&state.variant)
				};
				format!("{} plays {}", state.player_names[*player_index], id)
			}
			Action::Discard(DiscardAction { player_index, suit_index, rank, failed, .. }) => {
				let id = if *suit_index == -1 || *rank == -1 {
					"xx".to_string()
				} else {
					(Identity { suit_index: *suit_index as usize, rank: *rank as usize }).fmt(&state.variant)
				};
				format!("{} {} {}", state.player_names[*player_index], if *failed { "bombs" } else { "discards" }, id)
			}
			Action::Draw(DrawAction { player_index, suit_index, rank, .. }) => {
				let id = if *suit_index == -1 || *rank == -1 {
					"xx".to_string()
				} else {
					(Identity { suit_index: *suit_index as usize, rank: *rank as usize }).fmt(&state.variant)
				};
				format!("{} draws {}", state.player_names[*player_index], id)
			}
			Action::Turn(TurnAction { num, current_player_index }) => {
				format!("Turn {} ({})", num, state.player_names[*current_player_index as usize])
			}
			Action::Status(StatusAction { clues, score, max_score }) => {
				format!("Status! Clues: {}, Score: {}/{}", clues, score, max_score)
			}
			Action::GameOver(GameOverAction { .. }) => {
				"Game over!".to_string()
			}
			_ => { "".to_string() }
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformAction {
	Play { table_id: Option<u32>, target: usize },
	Discard { table_id: Option<u32>, target: usize },
	Colour { table_id: Option<u32>, target: usize, value: usize },
	Rank { table_id: Option<u32>, target: usize, value: usize },
	Terminate { table_id: Option<u32>, target: usize, value: usize }
}

impl<'de> Deserialize<'de> for PerformAction {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = Value::deserialize(deserializer)?;
		match s {
			Value::Object(ref map) => {
				let table_id = map.get("tableID").map(|v| v.to_string().parse().unwrap_or(0));
				let target = map.get("target").unwrap().to_string().parse().unwrap();

				match map.get("type").unwrap() {
					Value::Number(n) => match n.as_u64().unwrap() {
						0 => Ok(PerformAction::Play { table_id, target }),
						1 => Ok(PerformAction::Discard { table_id, target }),
						2 => Ok(PerformAction::Colour { table_id, target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						3 => Ok(PerformAction::Rank { table_id, target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						4 => Ok(PerformAction::Terminate { table_id, target, value: map.get("value").unwrap().to_string().parse().unwrap() }),
						_ => panic!("Invalid action type {:?}", s)
					},
					_ => panic!("Invalid action type {:?}", s)
				}
			}
			_ => Err(serde::de::Error::unknown_variant(&s.to_string(), &["number"])),
		}
	}
}

impl PerformAction {
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
				format!("Game ended: {} {}", target, value)
			}
		}
	}
}

impl Serialize for PerformAction {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match self {
			PerformAction::Play { table_id, target } => {
				let mut state = serializer.serialize_struct("PlayAction", 3)?;
				state.serialize_field("type", &0)?;
				state.serialize_field("tableID", table_id)?;
				state.serialize_field("target", target)?;
				state.end()
			}
			PerformAction::Discard { table_id, target } => {
				let mut state = serializer.serialize_struct("DiscardAction", 3)?;
				state.serialize_field("type", &1)?;
				state.serialize_field("tableID", table_id)?;
				state.serialize_field("target", target)?;
				state.end()
			}
			PerformAction::Colour { table_id, target, value } => {
				let mut state = serializer.serialize_struct("ColourAction", 4)?;
				state.serialize_field("type", &2)?;
				state.serialize_field("tableID", table_id)?;
				state.serialize_field("target", target)?;
				state.serialize_field("value", value)?;
				state.end()
			}
			PerformAction::Rank { table_id, target, value } => {
				let mut state = serializer.serialize_struct("RankAction", 4)?;
				state.serialize_field("type", &3)?;
				state.serialize_field("tableID", table_id)?;
				state.serialize_field("target", target)?;
				state.serialize_field("value", value)?;
				state.end()
			}
			PerformAction::Terminate { table_id, target, value } => {
				let mut state = serializer.serialize_struct("TerminateAction", 4)?;
				state.serialize_field("type", &4)?;
				state.serialize_field("tableID", table_id)?;
				state.serialize_field("target", target)?;
				state.serialize_field("value", value)?;
				state.end()
			}
		}
	}
}
