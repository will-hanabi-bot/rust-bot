use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::basics::state::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClueKind {
	COLOUR,
	RANK
}

impl<'de> Deserialize<'de> for ClueKind {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = Value::deserialize(deserializer)?;
		match s {
			Value::Number(n) => match n.as_u64() {
				Some(0) => Ok(ClueKind::COLOUR),
				Some(1) => Ok(ClueKind::RANK),
				_ => Err(serde::de::Error::unknown_variant(&n.to_string(), &["number"])),
			},
			_ => Err(serde::de::Error::unknown_variant(&s.to_string(), &["number"])),
		}
	}
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct BaseClue {
	#[serde(rename="type")]
	pub kind: ClueKind,
	pub value: usize
}

impl BaseClue {
	pub fn fmt(&self, state: &State, target: usize) -> String {
		let value = match self.kind {
			ClueKind::COLOUR => &state.variant.suits[self.value].to_lowercase(),
			ClueKind::RANK => &self.value.to_string(),
		};
		format!("({} to {})", value, state.player_names[target])
	}

	pub fn hash(&self) -> u64 {
		(if self.kind == ClueKind::COLOUR { 0 } else { 10 }) + self.value as u64
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CardClue {
	pub kind: ClueKind,
	pub value: usize,
	pub giver: usize,
	pub turn: usize
}

impl PartialEq<BaseClue> for CardClue {
	fn eq(&self, other: &BaseClue) -> bool {
		self.kind == other.kind && self.value == other.value
	}
}

impl PartialEq<CardClue> for BaseClue {
	fn eq(&self, other: &CardClue) -> bool {
		self.kind == other.kind && self.value == other.value
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Clue {
	pub kind: ClueKind,
	pub value: usize,
	pub target: usize
}

impl Clue {
	pub fn fmt(&self, state: &State) -> String {
		let value = match self.kind {
			ClueKind::COLOUR => &state.variant.suits[self.value].to_lowercase(),
			ClueKind::RANK => &self.value.to_string(),
		};
		format!("({} to {})", value, state.player_names[self.target])
	}

	pub fn to_base(&self) -> BaseClue {
		BaseClue {
			kind: self.kind,
			value: self.value,
		}
	}
}
