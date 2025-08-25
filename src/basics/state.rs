use std::hash::Hasher;
use std::sync::{Arc, LazyLock};

use crate::basics::identity_set::IdentitySet;
use crate::basics::variant::{all_ids, card_touched, Variant, PINKISH, RAINBOWISH};
use super::action::Action;
use super::clue::{BaseClue, Clue, ClueKind};
use super::card::{Card, Identifiable, Identity, Thought};

use ahash::AHasher;
use fraction::{ConstOne, Fraction};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct State {
	pub turn_count: usize,
	pub clue_tokens: Fraction,
	pub strikes: u8,
	pub hands: Vec<Vec<usize>>,
	pub deck: Vec<Card>,
	pub variant: Arc<Variant>,
	pub all_ids: IdentitySet,
	pub player_names: Vec<String>,
	pub num_players: usize,
	pub our_player_index: usize,
	/** The order of the next card to draw. */
	pub card_order: usize,
	pub cards_left: usize,
	pub play_stacks: Vec<usize>,
	pub discard_stacks: Vec<Vec<Vec<usize>>>,
	pub max_ranks: Vec<usize>,
	pub action_list: Arc<Vec<Vec<Action>>>,
	pub current_player_index: usize,
	pub endgame_turns: Option<usize>,
	card_count: Vec<usize>,
}

impl State {
	pub fn new(player_names: Vec<String>, our_player_index: usize, variant: Arc<Variant>) -> Self {
		let num_players = player_names.len();
		let num_suits = variant.suits.len();

		let mut card_count = Vec::new();
		let mut cards_left = 0;

		for suit_index in 0..num_suits {
			for rank in 1..=5 {
				let count = crate::basics::variant::card_count(&variant, Identity { suit_index, rank });

				cards_left += count;
				card_count.push(count);
			}
		}

		Self {
			turn_count: 0,
			clue_tokens: Fraction::new(8_u64, 1_u64),
			strikes: 0,
			hands: (0..num_players).map(|_| Vec::new()).collect(),
			deck: Vec::new(),
			all_ids: IdentitySet::from_iter(all_ids(&variant)),
			variant,
			player_names,
			num_players,
			our_player_index,
			card_order: 0,
			cards_left,
			card_count,
			play_stacks: vec![0; num_suits],
			discard_stacks: vec![vec![Vec::new(); 5]; num_suits],
			max_ranks: vec![5; num_suits],
			action_list: Arc::new(Vec::new()),
			current_player_index: 0,
			endgame_turns: None,
		}
	}

	pub fn hash(&self) -> u64 {
		let mut hasher = AHasher::default();

		for hand in &self.hands {
			hasher.write_usize(hand.len()); // keep structure info
			for &card in hand {
				hasher.write_usize(card);
			}
		}

		hasher.write_usize(self.deck.len());
		for card in &self.deck {
			hasher.write_usize(card.id().map(Identity::to_ord).unwrap_or(0));
		}

		hasher.write_u64(self.clue_tokens.numer().copied().unwrap());
		hasher.write_u64(self.clue_tokens.denom().copied().unwrap());

		match self.endgame_turns {
			Some(turns) => {
				hasher.write_u8(1);
				hasher.write_usize(turns);
			}
			None => {
				hasher.write_u8(0);
			}
		}

		hasher.finish()
	}

	#[inline]
	pub fn ended(&self) -> bool {
		self.strikes == 3 || self.score() == self.max_score() || match self.endgame_turns {
			Some(turns) => turns == 0,
			None => false,
		}
	}

	pub fn hand_size(&self) -> usize {
		[0, 0, 5, 5, 4, 4, 3][self.num_players]
	}

	#[inline]
	pub fn score(&self) -> usize {
		self.play_stacks.iter().sum()
	}

	#[inline]
	pub fn max_score(&self) -> usize {
		self.max_ranks.iter().sum()
	}

	pub fn rem_score(&self) -> usize {
		self.max_score() - self.score()
	}

	#[inline]
	pub fn pace(&self) -> i32 {
		self.score() as i32 + self.cards_left as i32 + self.num_players as i32 - self.max_score() as i32
	}

	pub fn in_endgame(&self) -> bool {
		self.pace() < self.num_players as i32 || self.score() >= self.max_score() - 5
	}

	pub fn last_player_index(&self, player_index: usize) -> usize {
		(player_index + self.num_players - 1) % self.num_players
	}

	pub fn next_player_index(&self, player_index: usize) -> usize {
		(player_index + 1) % self.num_players
	}

	/** Returns whether the identity is trash (played already or can never be played).  */
	pub fn is_basic_trash(&self, id: Identity) -> bool {
		id.rank <= self.play_stacks[id.suit_index] || id.rank > self.max_ranks[id.suit_index]
	}

	/** Returns how far the identity is from playable. 0 means that it is playable.*/
	pub fn playable_away(&self, id: Identity) -> i32 {
		id.rank as i32 - (self.play_stacks[id.suit_index] + 1) as i32
	}

	pub fn is_playable(&self, id: Identity) -> bool {
		self.playable_away(id) == 0
	}

	pub fn is_critical(&self, id: Identity) -> bool {
		!self.is_basic_trash(id) && self.discard_stacks[id.suit_index][id.rank - 1].len() == (self.card_count(id) - 1)
	}

	pub fn our_hand(&self) -> &Vec<usize> {
		&self.hands[self.our_player_index]
	}

	pub fn our_hand_mut(&mut self) -> &mut Vec<usize> {
		&mut self.hands[self.our_player_index]
	}

	/** Returns the number of cards matching an identity on the play+discard stacks.  */
	pub fn base_count(&self, id: Identity) -> usize {
		(if self.play_stacks[id.suit_index] >= id.rank { 1 } else { 0 }) +
		self.discard_stacks[id.suit_index][id.rank - 1].len()
	}

	pub fn all_valid_clues(&self, target: usize) -> Vec<Clue> {
		let mut clues = Vec::new();
		for rank in 1..=5 {
			if !self.clue_touched(&self.hands[target], &BaseClue { kind: ClueKind::RANK, value: rank }).is_empty() {
				clues.push(Clue { kind: ClueKind::RANK, value: rank, target });
			}
		}

		for suit_index in 0..self.variant.colourable_suits.as_ref().unwrap().len() {
			if !self.clue_touched(&self.hands[target], &BaseClue { kind: ClueKind::COLOUR, value: suit_index }).is_empty() {
				clues.push(Clue { kind: ClueKind::COLOUR, value: suit_index, target });
			}
		}
		clues
	}

	pub fn clue_touched(&self, orders: &[usize], clue: &BaseClue) -> Vec<usize> {
		orders.iter().filter_map(|&order| {
			card_touched(&self.deck[order], &self.variant, clue).then_some(order)
		}).collect()
	}

	pub fn has_consistent_inferences(&self, thought: &Thought) -> bool {
		if thought.possible.len() == 1 {
			return true;
		}

		match self.deck[thought.order].id() {
			None => true,
			Some(id) => thought.inferred.contains(id),
		}
	}

	pub fn regain_clue(&mut self) {
		self.clue_tokens += if self.variant.clue_starved.is_some_and(|c| c) { Fraction::new(1u64, 2u64) } else { Fraction::ONE };

		if self.clue_tokens > Fraction::from(8) {
			self.clue_tokens = Fraction::from(8);
		}
	}

	pub fn can_clue(&self) -> bool {
		self.clue_tokens >= Fraction::ONE
	}

	pub fn includes_variant(&self, regex: &LazyLock<Regex>) -> bool {
		self.variant.suits.iter().any(|suit| regex.is_match(suit)) ||
		(regex.as_str() == RAINBOWISH.as_str() && self.variant.rainbow_s.is_some_and(|s| s)) ||
		(regex.as_str() == PINKISH.as_str() && self.variant.pink_s.is_some_and(|s| s))
	}

	pub fn remaining_multiplicity(&self, ids: impl Iterator<Item = Identity>) -> usize {
		ids.map(|id| self.card_count(id) - self.base_count(id)).sum()
	}

	pub fn card_count(&self, id: Identity) -> usize {
		self.card_count[id.to_ord()]
	}

	pub fn holder_of(&self, order: usize) -> usize {
		self.hands.iter().position(|hand| hand.contains(&order)).unwrap_or_else(|| panic!("Tried to get holder of {order}, hands were {:?}!", self.hands))
	}

	pub fn expand_short(&self, short: &str) -> Identity {
		let suit_index = self.variant.short_forms.as_ref().unwrap().iter().position(|form| form == &short[0..1]).unwrap_or_else(|| panic!("Colour {short} doesn't exist in selected variant"));
		Identity { suit_index, rank: short[1..2].parse().unwrap_or_else(|_| panic!("Rank {short} doesn't exist in selected variant")) }
	}

	pub fn log_id(&self, id: Identity) -> String {
		format!("{}{}", self.variant.short_forms.as_ref().unwrap()[id.suit_index], id.rank)
	}

	pub fn log_oid(&self, id: &Option<Identity>) -> String {
		match id {
			Some(id) => self.log_id(*id),
			None => "xx".to_string(),
		}
	}

	pub fn log_iden<T>(&self, iden: &T) -> String where T: Identifiable {
		self.log_oid(&iden.id())
	}
}
