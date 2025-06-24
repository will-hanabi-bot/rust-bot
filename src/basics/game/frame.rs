use crate::basics::{card::{CardStatus, ConvData}, player::Player, state::State};

pub struct Frame<'a> {
	pub state: &'a State,
	pub meta: &'a [ConvData]
}

impl<'a> Frame<'a> {
	pub fn new(state: &'a State, meta: &'a [ConvData]) -> Self {
		Self { state, meta }
	}

	pub fn is_touched(&self, order: usize) -> bool {
		let Frame { state, meta, .. } = self;
		let ConvData { status, .. } = meta[order];

		state.deck[order].clued || status == CardStatus::Finessed || status == CardStatus::CalledToPlay
	}

	pub fn is_blind_playing(&self, order: usize) -> bool {
		let Frame { state, meta, .. } = self;
		let ConvData { status, .. } = meta[order];

		!state.deck[order].clued && (status == CardStatus::Finessed || status == CardStatus::CalledToPlay)
	}

	pub fn get_note(&self, common: &Player, order: usize) -> String {
		let thought = &common.thoughts[order];

		let note = if thought.inferred.is_empty() {
			"??".to_string()
		} else if thought.inferred.len() <= 6 {
			common.str_infs(self.state, order)
		} else {
			"...".to_string()
		};

		match self.meta[order].status {
			CardStatus::Finessed | CardStatus::CalledToPlay => {
				format!("[f] [{note}]")
			}
			CardStatus::ChopMoved => {
				format!("[cm] [{note}]")
			}
			CardStatus::CalledToDiscard => {
				"dc".to_string()
			}
			_ => note
		}
	}
}
