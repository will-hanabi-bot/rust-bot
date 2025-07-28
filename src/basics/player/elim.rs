use crate::basics::game::frame::Frame;
use crate::basics::identity_set::IdentitySet;
use crate::basics::variant::{all_ids};
use crate::basics::card::{IdOptions, Identifiable, Identity, Thought};
use crate::basics::state::State;
use super::{IdEntry, Player, Link};

use std::collections::{hash_map::Entry, HashMap, HashSet};
use itertools::Itertools;
use log::info;

#[derive(Debug, Clone)]
struct GTEntry { order: usize, cm: bool }

impl Player {
	fn update_map(&mut self, id: Identity, exclude: Vec<usize>) -> (bool, Vec<Identity>) {
		let mut changed = false;
		let mut recursive_ids = Vec::new();
		let mut cross_elim_removals = Vec::new();

		if let Some(candidates) = self.id_map.get_mut(&id) {
			candidates.retain(|&IdEntry { order, player_index }| {
				let no_elim = exclude.contains(&player_index) || self.certain_map.get(&id).is_some_and(|entry|
					entry.iter().any(|(o, unknown_to)| *o == order || unknown_to.contains(&player_index)));

				if no_elim {
					return true;
				}

				let thought = &mut self.thoughts[order];

				changed = true;
				thought.inferred.retain(|i| i != id);
				thought.possible.retain(|i| i != id);

				if thought.possible.is_empty() && !thought.reset {
					thought.reset_inferences();
				}
				// Card can be further eliminated
				else if thought.possible.len() == 1 {
					let recursive_id = thought.possible.iter().next().unwrap();
					match self.certain_map.entry(recursive_id) {
						Entry::Occupied(mut e) => { e.get_mut().insert(order, Vec::new()); },
						Entry::Vacant(e) => {
							let mut hmap = HashMap::new();
							hmap.insert(order, Vec::new());
							e.insert(hmap);
						}
					}
					recursive_ids.push(recursive_id);
					cross_elim_removals.push(order);
				}
				false
			});
			self.cross_elim_candidates.retain(|c| !cross_elim_removals.contains(&c.order));
		}
		(changed, recursive_ids)
	}

	/**
	 * The "typical" empathy operation. If there are enough known instances of an identity, it is removed from every card (including future cards).
	 * Returns true if at least one card was modified.
	 */
	fn basic_card_elim(&mut self, state: &State, ids: &IdentitySet) -> bool {
		let mut changed = false;
		let mut recursive_ids = IdentitySet::EMPTY;
		let mut eliminated = IdentitySet::EMPTY;

		for id in ids.iter() {
			let known_count = state.base_count(id) + self.certain_map.get(&id).map(|e| e.len()).unwrap_or(0);

			if known_count == state.card_count(id) {
				eliminated.insert(id);
				let (inner_changed, inner_recursive_ids) = self.update_map(id, Vec::new());
				changed = changed || inner_changed;
				recursive_ids.extend(&inner_recursive_ids);
			}
		}

		if !recursive_ids.is_empty() {
			self.basic_card_elim(state, &recursive_ids);
		}

		self.all_possible.retain(|i| !eliminated.contains(i));
		self.all_inferred.retain(|i| !eliminated.contains(i));
		changed
	}

	/**
	 * The "sudoku" emathy operation, involving 2 parts:
	 * Symmetric info - if Alice has [r5,g5] and Bob has [r5,g5], then everyone knows how r5 and g5 are distributed.
	 * Naked pairs - If Alice has 3 cards with [r4,g5], then everyone knows that both r4 and g5 cannot be elsewhere (will be eliminated in basic_elim).
	 * Returns true if at least one card was modified.
	 */
	fn perform_cross_elim(&mut self, state: &State, entries: &[IdEntry], ids: &IdentitySet) -> bool {
		let mut changed = false;
		let groups = entries.iter().into_group_map_by(|IdEntry { order, ..}| state.deck[*order].id());

		for (id, group) in groups {
			if let Some(id) = id {
				let certains = self.certain_map.get(&id).map(|c|
					c.iter().filter(|(order, _)| !group.iter().any(|e| e.order == **order)).count()
				).unwrap_or(0);

				if !self.id_map.contains_key(&id) || group.len() < state.remaining_multiplicity([id].into_iter()) - certains {
					continue;
				}

				let (inner_changed, _) = self.update_map(id, group.iter().map(|g| g.player_index).collect());
				changed = changed || inner_changed;
			}
		}

		// Now elim all the cards outside of this entry
		for id in ids.iter() {
			if !self.id_map.contains_key(&id) {
				continue;
			}

			let (inner_changed, _) = self.update_map(id, entries.iter().map(|e| e.player_index).collect());
			changed = changed || inner_changed;
		}

		self.basic_card_elim(state, ids) || changed
	}

	fn cross_card_elim(&mut self, state: &State, contained: &Vec<IdEntry>, acc_ids: &IdentitySet, certains: &Vec<usize>, next_index: usize) -> bool {
		if self.cross_elim_candidates.len() == 1 {
			return false;
		}

		let multiplicity = state.remaining_multiplicity(acc_ids.iter());

		// Impossible to reach multiplicity
		if multiplicity - certains.len() > contained.len() + (self.cross_elim_candidates.len() - next_index) {
			return false;
		}

		if contained.len() >= 2 && multiplicity - certains.len() == contained.len() {
			let inner_changed = self.perform_cross_elim(state, contained, acc_ids);
			if inner_changed {
				return true;
			}
		}

		if next_index >= self.cross_elim_candidates.len() {
			return false;
		}

		// Check all remaining subsets that contain the next item
		let item = &self.cross_elim_candidates[next_index];
		let new_acc_ids: IdentitySet = acc_ids.union(&self.thoughts[item.order].possible);

		let mut next_contained = contained.clone();
		next_contained.push(item.clone());

		let mut next_certains = certains.clone();

		for id in self.thoughts[item.order].possible.iter() {
			if acc_ids.contains(id) {
				continue;
			}

			if let Some(entries) = self.certain_map.get(&id) {
				for order in entries.keys() {
					if !certains.contains(order) {
						next_certains.push(*order);
					}
				}
			}
		}
		next_certains.retain(|o| !next_contained.iter().any(|e| e.order == *o));

		let included = self.cross_card_elim(state, &next_contained, &new_acc_ids, &next_certains, next_index + 1);
		if included {
			return true;
		}

		// Check all remaining subsets that skip the next item
		self.cross_card_elim(state, contained, acc_ids, certains, next_index + 1)
	}

	pub fn card_elim(&mut self, state: &State) {
		self.certain_map.clear();
		self.id_map.clear();
		self.cross_elim_candidates.clear();

		let actual_id_opts = IdOptions { symmetric: self.is_common, ..Default::default() };
		let symmetric_id_opts = IdOptions { symmetric: true, ..Default::default() };

		for player_index in 0..state.num_players {
			for &order in &state.hands[player_index] {
				let thought = &self.thoughts[order];
				let id = thought.identity(&actual_id_opts);

				let unknown_to = if thought.identity(&symmetric_id_opts).is_none() {
					vec![player_index]
				}
				else {
					Vec::new()
				};

				if let Some(id) = id {
					match self.certain_map.entry(id) {
						Entry::Occupied(mut e) => { e.get_mut().insert(order, unknown_to); },
						Entry::Vacant(e) => {
							let mut hmap = HashMap::new();
							hmap.insert(order, unknown_to);
							e.insert(hmap);
						}
					}
				}

				if thought.possible.len() > 1 && thought.possible.iter().any(|id| !state.is_basic_trash(id)) && state.remaining_multiplicity(thought.possible.iter()) <= 8 {
					self.cross_elim_candidates.push(IdEntry { order, player_index });
				}

				for id in thought.possible.iter() {
					let entry = IdEntry { order, player_index };
					match self.id_map.entry(id) {
						Entry::Occupied(mut e) => e.get_mut().push(entry),
						Entry::Vacant(e) => { e.insert(vec![entry]); }
					}
				}
			}
		}

		let all_ids = IdentitySet::from_iter(all_ids(&state.variant));
		self.basic_card_elim(state, &all_ids);
		while self.cross_card_elim(state, &Vec::new(), &IdentitySet::EMPTY, &Vec::new(), 0) {}
	}

	pub fn good_touch_elim(&mut self, frame: &Frame) {
		let Frame { state, meta } = frame;
		self.certain_map.clear();
		self.infer_map.clear();
		let mut elim_candidates = Vec::new();

		for i in 0..state.num_players {
			for &order in &state.hands[i] {
				// self.add_to_maps(frame, order, i);

				let thought = &self.thoughts[order];

				if meta[order].trash || thought.reset ||  thought.identity(&IdOptions { symmetric: true, ..Default::default() }).is_some() {
					continue;
				}

				if !thought.inferred.is_empty() && thought.possible.iter().any(|i| !state.is_basic_trash(i)) {
					if frame.is_touched(order) {
						elim_candidates.push(GTEntry { order, cm: false });
					}
					else if meta[order].cm() {
						elim_candidates.push(GTEntry { order, cm: self.is_common });
					}
				}
			}
		}

		let mut all_ids = IdentitySet::from_iter(all_ids(&state.variant));
		let trash_ids: IdentitySet = all_ids.filter(|i| state.is_basic_trash(i));
		all_ids.retain(|i| !trash_ids.contains(i));

		// Remove all trash identities
		for &GTEntry { order, cm, .. } in &elim_candidates {
			let thought = &mut self.thoughts[order];
			thought.inferred.retain(|i| !trash_ids.contains(i));

			if !cm && thought.inferred.is_empty() && !thought.reset {
				thought.reset_inferences();
			}
		}

		// self.basic_gt_elim(frame, &all_ids, &elim_candidates);
		// self.card_elim(state);
	}

	fn elim_link(&mut self, frame: &Frame, matches: &Vec<&usize>, focused_order: &usize, id: Identity, good_touch: bool) {
		let Frame { state, .. } = frame;
		info!("eliminating link with inference {} from focus! original {:?}, final {}", state.log_id(id), matches, focused_order);

		for &order in matches {
			let thought = &mut self.thoughts[*order];
			if order == focused_order {
				thought.inferred = IdentitySet::single(id);
			}
			else {
				thought.inferred.retain(|i| i != id);
			}

			if thought.inferred.is_empty() && !thought.reset {
				thought.reset_inferences();

				if good_touch {
					let mut inferred = thought.inferred;
					inferred.retain(|i| !self.is_trash(frame, i, 999));
					self.thoughts[*order].inferred = inferred;
				}
			}
		}
	}

	pub fn find_links(&mut self, frame: &Frame, good_touch: bool) {
		let Frame { state, meta } = frame;
		let mut linked_orders: HashSet<usize> = self.links.iter().flat_map(|link| match link {
			Link::Promised { orders, .. } | Link::Unpromised { orders, .. } => orders
		}).cloned().collect();

		let orders = if self.is_common { &state.hands.concat() } else { &state.hands[self.player_index] };
		let linkable_orders = orders.iter().filter(|o| {
			let thought = &self.thoughts[**o];
			thought.id().is_none() && (0..=3).contains(&thought.inferred.len()) && !thought.inferred.iter().all(|i| state.is_basic_trash(i))
		}).collect::<Vec<_>>();

		for &&order in &linkable_orders {
			let thought = &self.thoughts[order];
			let Thought { inferred, .. } = thought;

			if linked_orders.contains(&order) {
				continue;
			}

			// Find all cards with the same inferences
			let matches = linkable_orders.iter().filter(|&&&o| &self.thoughts[o].inferred == inferred).copied().collect::<Vec<_>>();
			let focused_matches = matches.iter().filter(|&&&o| meta[o].focused).collect::<Vec<_>>();

			if matches.len() == 1 {
				continue;
			}

			if focused_matches.len() == 1 && inferred.len() == 1 {
				self.elim_link(frame, &matches, focused_matches[0], inferred.iter().next().unwrap(), good_touch);
				continue;
			}

			// We have enough inferred cards to elim elsewhere
			if matches.len() > inferred.len() {
				info!("adding link {:?} inferences {} ({})", matches, inferred.iter().map(|i| state.log_id(i)).join(","), if self.is_common { "common" } else { &state.player_names[self.player_index] });
				for o in &matches {
					linked_orders.insert(**o);
				}
				self.links.push(Link::Unpromised { orders: matches.into_iter().cloned().collect(), ids: inferred.to_vec() });
			}
		}
	}

	pub fn refresh_links(&mut self, frame: &Frame, good_touch: bool) {
		let Frame { state, meta } = frame;
		let mut new_links = Vec::new();

		for link in self.links.clone() {
			match link {
				Link::Promised { orders, id, target } => {
					// At least 1 card matches, promise resolved
					if orders.iter().any(|&o| self.thoughts[o].is(&id)) {
						continue;
					}

					if !self.thoughts[target].possible.iter().any(|i| id.suit_index == i.suit_index) {
						continue;
					}

					let viable_orders = orders.iter().filter(|&o| self.thoughts[*o].possible.contains(id)).collect::<Vec<_>>();

					if viable_orders.is_empty() {
						info!("promised id {} not found among cards {:?}, rewind?", state.log_id(id), orders)
					}
					else if viable_orders.len() == 1 {
						self.thoughts[*viable_orders[0]].inferred = IdentitySet::single(id);
					}
					else {
						new_links.push(Link::Promised { orders: viable_orders.into_iter().cloned().collect(), id, target });
					}
				}
				Link::Unpromised { ref orders, ref ids } => {
					let revealed = orders.iter().filter(|&&o| {
						let thought = &self.thoughts[o];
						thought.id().is_some() || ids.iter().any(|i| !thought.possible.contains(*i))
					}).collect::<Vec<_>>();

					if !revealed.is_empty() {
						continue;
					}

					let focused_orders = orders.iter().filter(|&&o| meta[o].focused).collect::<Vec<_>>();

					if focused_orders.len() == 1 && ids.len() == 1 {
						self.elim_link(frame, &orders.iter().collect(), focused_orders[0], *ids.iter().next().unwrap(), good_touch);
					}

					if let Some(lost_inference) = ids.iter().find(|&i| orders.iter().any(|&o| !self.thoughts[o].inferred.contains(*i))) {
						info!("linked orders {:?} lost inference {}", orders, state.log_id(*lost_inference));
						continue;
					}
					new_links.push(link);
				}
			}
		}

		self.links = new_links;
		self.find_links(frame, good_touch);
	}
}
