use crate::basics::card::ConvData;
use crate::basics::clue::BaseClue;
use crate::basics::game::{frame::Frame};
use crate::basics::util::visible_find;

use super::card::{CardStatus, IdOptions, Identifiable, Identity, MatchOptions, Thought};
use super::state::State;
use super::variant::{card_count};
use std::collections::{HashMap, HashSet};
use itertools::Itertools;
use log::{warn};

mod elim;

#[derive(Debug, Clone)]
pub enum Link {
	Promised { orders: Vec<usize>, id: Identity, target: usize },
	Unpromised { orders: Vec<usize>, ids: Vec<Identity> }
}

#[derive(Debug, Clone, PartialEq)]
struct MatchEntry { order: usize, unknown_to: Vec<usize> }

#[derive(Debug, Clone)]
struct IdEntry { order: usize, player_index: usize }

#[derive(Debug, Clone)]
struct GTEntry { order: usize, player_index: usize, cm: bool }

#[derive(Debug, Clone)]
pub struct WaitingConnection {
	pub giver: usize,
	pub reacter: usize,
	pub receiver: usize,
	pub receiver_hand: Vec<usize>,
	pub clue: BaseClue,
	pub focus_slot: usize,
}

#[derive(Debug, Clone)]
pub struct Player {
	pub player_index: usize,
	pub is_common: bool,
	pub thoughts: Vec<Thought>,
	pub all_possible: HashSet<Identity>,
	pub all_inferred: HashSet<Identity>,

	pub hypo_stacks: Vec<usize>,
	pub links: Vec<Link>,

	pub unknown_plays: HashSet<usize>,
	pub hypo_plays: HashSet<usize>,

	pub waiting: Vec<WaitingConnection>,

	certain_map: HashMap<Identity, HashMap<usize, Vec<usize>>>,
	infer_map: HashMap<Identity, Vec<MatchEntry>>,
	id_map: HashMap<Identity, Vec<IdEntry>>,
	cross_elim_candidates: Vec<IdEntry>,
}

impl Player {
	pub fn new(player_index: Option<usize>, all_possible: HashSet<Identity>, hypo_stacks: Vec<usize>) -> Self {
		Self {
			player_index: player_index.unwrap_or(99),
			is_common: player_index.is_none(),
			thoughts: Vec::new(),
			all_possible: all_possible.clone(),
			all_inferred: all_possible.clone(),
			hypo_stacks,
			links: Vec::new(),
			unknown_plays: HashSet::new(),
			hypo_plays: HashSet::new(),
			waiting: Vec::new(),

			certain_map: HashMap::new(),
			infer_map: HashMap::new(),
			id_map: HashMap::new(),
			cross_elim_candidates: Vec::new(),
		}
	}

	pub fn str_infs(&self, state: &State, order: usize) -> String {
		self.thoughts[order].inferred.iter().sorted_by_key(|&i| i.suit_index * 10 + i.rank).map(|id| state.log_id(id)).join(",")
	}

	pub fn str_poss(&self, state: &State, order: usize) -> String {
		self.thoughts[order].possible.iter().sorted_by_key(|&i| i.suit_index * 10 + i.rank).map(|id| state.log_id(id)).join(",")
	}

	pub fn refer(&self, frame: &Frame, hand: &[usize], order: usize, left: bool) -> usize {
		let offset: i32 = if left { -1 } else { 1 };
		let index = hand.iter().position(|&o| o == order).unwrap();

		let mut target_index = (index as i32 + offset + hand.len() as i32) as usize % hand.len();

		while frame.is_touched(hand[target_index]) && target_index != index {
			target_index = (target_index as i32 + offset + hand.len() as i32) as usize % hand.len();
		}

		hand[target_index]
	}

	/** Returns whether the identity has already been sieved in someone's hand, excluding the given order. */
	pub fn is_sieved(&self, frame: &Frame, id: &Identity, order: usize) -> bool {
		let Frame { state, meta } = frame;
		for player_index in 0..state.num_players {
			let loaded = self.thinks_loaded(frame, player_index);

			for (i, o) in state.hands[player_index].iter().enumerate() {
				if *o != order && self.thoughts[*o].matches(id, &MatchOptions { infer: true, ..Default::default() }) {
					if loaded {
						if meta[*o].status != CardStatus::CalledToDiscard  {
							return true;
						}
					}
					else if i != 0 {
						return true;
					}
				}
			}
		}

		self.links.iter().any(|l| match l {
			Link::Promised { orders, id: promise, .. } => {
				!orders.contains(&order) && promise == id
			}
			Link::Unpromised { orders, ids } => {
				!orders.contains(&order) && ids.contains(id)
			}
		})
	}

	/** Returns whether the identity has already been touched in someone's hand, excluding the given order. */
	pub fn is_saved(&self, frame: &Frame, id: &Identity, order: usize) -> bool {
		let Frame { state, .. } = frame;

		state.hands.concat().iter().any(|&o|
			o != order &&
			self.thoughts[o].matches(id, &MatchOptions { infer: true, ..Default::default() }) &&
			frame.is_touched(o) &&
			// Not sharing a link
			!self.links.iter().any(|l| match l {
				Link::Promised { orders, id: promise, .. } => {
					orders.contains(&order) && orders.contains(&o) && promise == id
				}
				Link::Unpromised { orders, ids } => {
					orders.contains(&order) && orders.contains(&o) && ids.contains(id)
				}
			})
		)
	}

	/** Returns whether the identity is trash (either basic trash or already saved). */
	pub fn is_trash(&self, frame: &Frame, id: &Identity, order: usize) -> bool {
		frame.state.is_basic_trash(id) || self.is_saved(frame, id, order)
	}

	/** Returns whether the order is trash (either basic trash or already saved). */
	pub fn order_trash(&self, frame: &Frame, order: usize) -> bool {
		let ConvData { status, trash, depends_on, .. } = &frame.meta[order];

		if self.thoughts[order].possible.iter().all(|id| self.is_trash(frame, id, order)) {
			return true;
		}

		if *trash || *status == CardStatus::CalledToDiscard {
			if let Some(depends) = depends_on {
				if depends.iter().any(|d| frame.state.hands.concat().contains(d)) {
					warn!("{} depends on {:?}!", order, depends);
				}
			}
			else {
				return true;
			}
		}

		self.thoughts[order].possibilities().iter().all(|id| self.is_trash(frame, id, order))
	}

	/** Returns whether the order is globally known trash (either basic trash or already saved). */
	pub fn order_kt(&self, frame: &Frame, order: usize) -> bool {
		if frame.meta[order].trash {
			return true;
		}

		self.thoughts[order].possible.iter().all(|id| self.is_trash(frame, id, order))
	}

	pub fn order_kp(&self, frame: &Frame, order: usize) -> bool {
		if frame.meta[order].status == CardStatus::CalledToPlay && self.thoughts[order].possible.iter().any(|id| frame.state.is_playable(id)) {
			return true;
		}
		self.thoughts[order].possible.iter().all(|id| frame.state.is_playable(id))
	}

	pub fn thinks_locked(&self, frame: &Frame, player_index: usize) -> bool {
		!self.thinks_loaded(frame, player_index) && frame.state.hands[player_index].iter().all(|&order|
			frame.state.deck[order].clued ||
			frame.meta[order].status == CardStatus::Finessed ||
			frame.meta[order].status == CardStatus::CalledToPlay ||
			frame.meta[order].status == CardStatus::ChopMoved
		)
	}

	pub fn thinks_playables(&self, frame: &Frame, player_index: usize) -> Vec<usize> {
		let Frame { state, meta } = frame;
		let linked_orders = self.linked_orders(state);

		state.hands[player_index].iter().filter_map(|&order| {
			if let Some(depends) = &meta[order].depends_on {
				if depends.iter().any(|d| state.hands.concat().contains(d)) {
					warn!("{} depends on {:?}!", order, depends);
					return None;
				}
			}

			let thought = &self.thoughts[order];
			let unsafe_linked = linked_orders.contains(&order) && (
				state.strikes == 2 ||
				state.endgame_turns.is_some()
			);

			if unsafe_linked {
				// warn!("{} is unsafe linked {:?}", order, self.links);
				return None;
			}

			thought.possibilities().iter().all(|id| state.is_playable(id)).then_some(order)
		}).collect()
	}

	pub fn thinks_trash(&self, frame: &Frame, player_index: usize) -> Vec<usize> {
		frame.state.hands[player_index].iter().filter(|&order| self.order_trash(frame, *order)).copied().collect()
	}

	pub fn thinks_loaded(&self, frame: &Frame, player_index: usize) -> bool {
		!self.thinks_playables(frame, player_index).is_empty() || !self.thinks_trash(frame, player_index).is_empty()
	}

	pub fn save2(&self, state: &State, id: &Identity) -> bool {
		let Identity { suit_index, rank } = id;

		*rank == 2 &&
		state.play_stacks[*suit_index] < 2 &&
		visible_find(state, self, id, MatchOptions { infer: true, ..Default::default() }, |_, _| true).len() == 1
	}

	pub fn card_value(&self, frame: &Frame, id: &Identity, order: Option<usize>) -> usize {
		let Identity { suit_index, rank } = id;

		if self.is_trash(frame, id, order.unwrap_or(99)) ||
			visible_find(frame.state, self, id, MatchOptions { infer: true, ..Default::default() }, |_, _| true).len() > 1 {
			0
		}
		else if frame.state.is_critical(id) {
			5
		}
		else if self.save2(frame.state, id) {
			4
		}
		else if *rank < self.hypo_stacks[*suit_index] {
			0
		}
		else {
			5 - (*rank - self.hypo_stacks[*suit_index])
		}
	}

	pub fn locked_discard(&self, state: &State, player_index: usize) -> usize {
		let crit_percents = state.hands[player_index].iter().map(|&o| {
			let poss = self.thoughts[o].possibilities();
			let percent = poss.iter().filter(|&p| state.is_critical(p)).count() / poss.len();
			(o, percent)
		}).sorted_by_key(|&(_, percent)| percent).collect::<Vec<_>>();

		let least_crits = crit_percents.iter().filter(|&(_, percent)| *percent == crit_percents[0].1);

		least_crits.max_by_key(|&(order, percent)| {
			self.thoughts[*order].possibilities().iter().map(|&p| {
				let crit_distance = if *percent == 1 { p.rank as i32 * 5 } else { 0 } + p.rank as i32 - self.hypo_stacks[p.suit_index] as i32;
				if crit_distance < 0 { 5 } else { crit_distance as usize }
			}).sum::<usize>()
		}).unwrap().0
	}

	pub fn unknown_ids(&self, state: &State, id: &Identity) -> usize {
		let visible_count: usize = state.hands.iter().map(|hand|
			hand.iter().filter(|&&o| self.thoughts[o].is(id)).count()).sum();
		card_count(&state.variant, id) - state.base_count(id) - visible_count
	}

	pub fn linked_orders(&self, state: &State) -> HashSet<usize> {
		let mut orders = HashSet::new();
		for link in &self.links {
			match link {
				Link::Promised { orders: link_orders, id, .. } => {
					if link_orders.len() > self.unknown_ids(state, id) {
						orders.extend(link_orders);
					}
				},
				Link::Unpromised { orders: link_orders, ids } => {
					if link_orders.len() > ids.iter().map(|id| self.unknown_ids(state, id)).sum() {
						orders.extend(link_orders);
					}
				}
			}
		}
		orders
	}

	pub fn update_hypo_stacks(&mut self, frame: &Frame, ignore: &[usize]) {
		let Frame { state, .. } = frame;
		let mut hypo_stacks = state.play_stacks.clone();
		let mut unknown_plays: HashSet<usize> = HashSet::new();
		let mut played: HashSet<usize> = HashSet::new();
		let mut unplayable: HashSet<usize> = HashSet::new();

		let mut found_playable = true;
		let mut good_touch_elim: HashSet<Identity> = HashSet::new();
		let linked_orders = self.linked_orders(state);

		while found_playable {
			found_playable = false;

			for player_index in 0..state.num_players {
				for &order in &state.hands[player_index] {
					if ignore.contains(&order) || linked_orders.contains(&order) || played.contains(&order) || unplayable.contains(&order) {
						continue;
					}

					let thought = &self.thoughts[order];
					let id = thought.identity(&IdOptions { infer: true, symmetric: self.player_index == player_index });
					let actual_id: Option<&Identity> = state.deck[order].id();

					if !frame.is_touched(order) || actual_id.map(|i| good_touch_elim.contains(i)).unwrap_or(false) {
						continue;
					}

					let delayed_playable = |ids: Vec<&Identity>| {
						let mut remaining = ids.iter().filter(|id| !good_touch_elim.contains(id)).peekable();
						remaining.peek().is_some() && remaining.all(|id| hypo_stacks[id.suit_index] + 1 == id.rank)
					};

					let playable = state.has_consistent_inferences(thought) &&
						(delayed_playable(thought.possible.iter().collect()) ||
						delayed_playable(thought.inferred.iter().collect()) ||
						(frame.is_blind_playing(order) && actual_id.is_some() && delayed_playable(vec![actual_id.unwrap()])));

					if !playable {
						continue;
					}

					match id {
						None => {
							unknown_plays.insert(order);
							played.insert(order);
							found_playable = true;

							for link in &self.links {
								if let Link::Promised { id, .. } = link {
									if id.rank != hypo_stacks[id.suit_index] + 1 {
										warn!("tried to add linked {} ({}) onto hypo stacks, but they were at {hypo_stacks:?} {:?}", state.log_id(id), order, played);
										unplayable.insert(order);
									}
									else {
										hypo_stacks[id.suit_index] = id.rank;
										good_touch_elim.insert(*id);
									}
								}
							}
						},
						Some(id) => {
							if id.rank != hypo_stacks[id.suit_index] + 1 {
								warn!("tried to add {} ({}) onto hypo stacks, but they were at {hypo_stacks:?} {:?}", state.log_id(id), order, played);
								unplayable.insert(order);
							}
							else {
								found_playable = true;
								hypo_stacks[id.suit_index] = id.rank;
								good_touch_elim.insert(*id);
								played.insert(order);
							}
						}
					}
				}
			}
		}
		self.hypo_stacks = hypo_stacks;
		self.unknown_plays = unknown_plays;
		self.hypo_plays = played;
	}
}
