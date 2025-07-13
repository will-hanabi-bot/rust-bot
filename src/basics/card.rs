use std::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

use crate::basics::identity_set::IdentitySet;
use super::clue::CardClue;

#[derive(Debug, Clone, Default)]
pub struct IdOptions {
	pub infer: bool,
	pub symmetric: bool
}

#[derive(Debug, Clone, Default)]
pub struct MatchOptions {
	pub infer: bool,
	pub symmetric: bool,
	pub assume: bool,
}

pub trait Identifiable {
	fn identity(&self, options: &IdOptions) -> Option<Identity>;

	fn id(&self) -> Option<Identity> {
		self.identity(&Default::default())
	}

	fn matches<Other>(&self, other: &Other, options: &MatchOptions) -> bool where Other: Identifiable + ?Sized {
		let id_opts = IdOptions {
			infer: options.infer,
			symmetric: options.symmetric,
		};
		let a = self.identity(&id_opts);
		match a {
			None => options.assume,
			Some(a) => {
				let b = other.identity(&id_opts);
				match b {
					None => false,
					Some(b) => a.suit_index == b.suit_index && a.rank == b.rank
				}
			}
		}
	}

	fn is<Other>(&self, other: &Other) -> bool where Other: Identifiable + ?Sized {
		self.matches(other, &MatchOptions::default())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identity {
	#[serde(rename="suitIndex")]
	pub suit_index: usize,
	pub rank: usize
}

impl Identifiable for Identity {
	fn identity(&self, _options: &IdOptions) -> Option<Identity> {
		Some(*self)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
	None,
	Finessed,
	ChopMoved,
	CalledToPlay,
	CalledToDiscard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
	pub base: Option<Identity>,
	pub order: usize,
	pub drawn_index: usize,
	pub clued: bool,
	pub newly_clued: bool,
	pub clues: Vec<CardClue>,
}

impl Display for CardStatus {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			CardStatus::None => write!(f, "none"),
			CardStatus::Finessed => write!(f, "finessed"),
			CardStatus::ChopMoved => write!(f, "chop moved"),
			CardStatus::CalledToPlay => write!(f, "called to play"),
			CardStatus::CalledToDiscard => write!(f, "called to discard"),
		}
	}
}

impl Card {
	pub fn new(base: Option<Identity>, order: usize, drawn_index: usize) -> Self {
		Self {
			base,
			order,
			drawn_index,
			clued: false,
			newly_clued: false,
			clues: Vec::new(),
		}
	}
}

impl Identifiable for Card {
	fn identity(&self, _options: &IdOptions) -> Option<Identity> {
		self.base
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thought {
	pub order: usize,
	pub base: Option<Identity>,
	pub possible: IdentitySet,
	pub inferred: IdentitySet,
	pub info_lock: Option<IdentitySet>,
	pub reset: bool,
}

#[derive(Debug, Clone)]
pub struct ConvData {
	pub order: usize,
	pub focused: bool,
	pub urgent: bool,
	pub trash: bool,
	pub status: CardStatus,
	pub depends_on: Option<Vec<usize>>,
	pub reasoning: Vec<usize>,
}

impl ConvData {
	pub fn new(order: usize) -> Self {
		Self {
			order,
			focused: false,
			urgent: false,
			trash: false,
			depends_on: None,
			status: CardStatus::None,
			reasoning: Vec::new(),
		}
	}

	pub fn blind_playing(&self) -> bool {
		self.status == CardStatus::Finessed
	}

	pub fn cm(&self) -> bool {
		self.status == CardStatus::ChopMoved
	}
}

impl Thought {
	pub fn new(order: usize, base: Option<Identity>, poss: IdentitySet) -> Self {
		Self {
			order,
			base,
			possible: poss,
			inferred: poss,
			info_lock: None,
			reset: false,
		}
	}

	pub fn possibilities(&self) -> IdentitySet {
		if self.inferred.is_empty() { self.possible } else { self.inferred }
	}

	pub fn reset_inferences(&mut self) {
		self.reset = true;
		self.inferred = self.possible;
		if let Some(info_lock) = &self.info_lock {
			self.inferred.intersect(info_lock);
		}
	}
}

impl Identifiable for Thought {
	fn identity(&self, options: &IdOptions) -> Option<Identity> {
		if self.possible.len() == 1 {
			return Some(self.possible.iter().next().unwrap())
		}

		if !options.symmetric && self.base.is_some() {
			return self.base;
		}

		(options.infer && self.inferred.len() == 1).then(|| self.inferred.iter().next().unwrap())
	}
}
