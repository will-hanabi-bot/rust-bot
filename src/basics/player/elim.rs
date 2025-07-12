use crate::basics::game::frame::Frame;
use crate::basics::variant::{all_ids, card_count};
use crate::basics::card::{IdOptions, Identifiable, Identity, Thought};
use crate::basics::state::State;
use super::{IdEntry, GTEntry, MatchEntry, Player, Link};

use std::collections::{hash_map::Entry, HashSet};
use itertools::Itertools;
use log::{info, warn};

impl Player {
	fn update_map(&mut self, id: &Identity, exclude: Vec<usize>) -> (bool, Vec<Identity>) {
		let mut changed = false;
		let mut recursive_ids = Vec::new();
		let mut cross_elim_removals = Vec::new();

		if let Some(candidates) = self.id_map.get_mut(id) {
			candidates.retain(|&IdEntry { order, player_index }| {
				let no_elim = exclude.contains(&player_index) || self.certain_map.get(id).is_some_and(|entry|
					entry.iter().any(|x| x.order == order || x.unknown_to.contains(&player_index)));

				if no_elim {
					return true;
				}

				let thought = &mut self.thoughts[order];

				changed = true;
				thought.inferred.retain(|&i| i != *id);
				thought.possible.retain(|&i| i != *id);

				if thought.possible.is_empty() && !thought.reset {
					thought.reset_inferences();
				}
				// Card can be further eliminated
				else if thought.possible.len() == 1 {
					let recursive_id = thought.possible.iter().next().unwrap();
					let entry = MatchEntry { order, unknown_to: Vec::new() };
					match self.certain_map.entry(*recursive_id) {
						Entry::Occupied(mut e) => e.get_mut().push(entry),
						Entry::Vacant(e) => { e.insert(vec![entry]); }
					}
					recursive_ids.push(*recursive_id);
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
	fn basic_card_elim(&mut self, state: &State, ids: &HashSet<Identity>) -> bool {
		let mut changed = false;
		let mut recursive_ids = HashSet::new();
		let mut eliminated: HashSet<Identity> = HashSet::new();

		for id in ids {
			let known_count = state.base_count(id) + self.certain_map.get(id).map(|e| e.len()).unwrap_or(0);

			if known_count == card_count(&state.variant, id) {
				eliminated.insert(*id);
				let (inner_changed, inner_recursive_ids) = self.update_map(id, Vec::new());
				changed = changed || inner_changed;
				recursive_ids.extend(inner_recursive_ids);
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
	fn perform_cross_elim(&mut self, state: &State, entries: &[IdEntry], ids: &HashSet<Identity>) -> bool {
		let mut changed = false;
		let groups = entries.iter().into_group_map_by(|IdEntry { order, ..}| state.deck[*order].id());

		for (id, group) in groups {
			if let Some(id) = id {
				let certains = self.certain_map.get(id).map(|c|
					c.iter().filter(|MatchEntry { order, .. }| !group.iter().any(|e| e.order == *order)).count()
				).unwrap_or(0);

				if !self.id_map.contains_key(id) || group.len() < state.remaining_multiplicity([*id].iter()) - certains {
					continue;
				}

				let (inner_changed, _) = self.update_map(id, group.iter().map(|g| g.player_index).collect());
				changed = changed || inner_changed;
			}
		}

		// Now elim all the cards outside of this entry
		for id in ids {
			if !self.id_map.contains_key(id) {
				continue;
			}

			let (inner_changed, _) = self.update_map(id, entries.iter().map(|e| e.player_index).collect());
			changed = changed || inner_changed;
		}

		self.basic_card_elim(state, ids) || changed
	}

	fn cross_card_elim(&mut self, state: &State, contained: &Vec<IdEntry>, acc_ids: &HashSet<Identity>, certains: &HashSet<usize>, next_index: usize) -> bool {
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
		let new_acc_ids: HashSet<Identity> = acc_ids.union(&self.thoughts[item.order].possible).cloned().collect();

		let mut next_contained = contained.clone();
		next_contained.push(item.clone());

		let new_certains: HashSet<usize> = self.thoughts[item.order].possible.difference(acc_ids).flat_map(|&id|
			self.certain_map.get(&id).map(|c| c.iter().map(|e| e.order).collect::<Vec<usize>>()).unwrap_or_default()).collect();

		let mut next_certains = certains.union(&new_certains).cloned().collect::<HashSet<usize>>();
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
					let entry = MatchEntry { order, unknown_to };
					match self.certain_map.entry(*id) {
						Entry::Occupied(mut e) => e.get_mut().push(entry),
						Entry::Vacant(e) => { e.insert(vec![entry]); }
					}
				}

				if (1..=10).contains(&thought.possible.len()) && thought.possible.iter().any(|id| !state.is_basic_trash(id)) {
					self.cross_elim_candidates.push(IdEntry { order, player_index });
				}

				for id in &thought.possible {
					let entry = IdEntry { order, player_index };
					match self.id_map.entry(*id) {
						Entry::Occupied(mut e) => e.get_mut().push(entry),
						Entry::Vacant(e) => { e.insert(vec![entry]); }
					}
				}
			}
		}

		let all_ids: HashSet<Identity> = HashSet::from_iter(all_ids(&state.variant));
		self.basic_card_elim(state, &all_ids);
		while self.cross_card_elim(state, &Vec::new(), &HashSet::new(), &HashSet::new(), 0) {}
	}

	fn add_to_maps(&mut self, frame: &Frame, order: usize, player_index: usize) {
		let Frame { state, meta } = frame;
		let thought = &self.thoughts[order];

		if !frame.is_touched(order) {
			return;
		}

		let opts = IdOptions { infer: true, symmetric: self.is_common || self.player_index == player_index };
		if let Some(id) = thought.identity(&opts) {
			let entry = MatchEntry { order, unknown_to: vec![] };
			if thought.is(id) || meta[order].focused {
				match self.certain_map.entry(*id) {
					Entry::Occupied(mut e) => e.get_mut().push(entry.clone()),
					Entry::Vacant(e) => { e.insert(vec![entry.clone()]); }
				}
			}
			match self.infer_map.entry(*id) {
				Entry::Occupied(mut e) => e.get_mut().push(entry),
				Entry::Vacant(e) => { e.insert(vec![entry]); }
			}

			if let Some(matches) = self.infer_map.get(id) {
				if let Some(hard_matches) = self.certain_map.get_mut(id) {
					if state.base_count(id) + matches.len() <= card_count(&state.variant, id) {
						return;
					}

					// Players holding the identity
					let holders: HashSet<usize> = matches.iter().chain(hard_matches.iter())
						.filter(|&m| state.deck[m.order].is(id))
						.map(|m|state.holder_of(m.order).unwrap())
						.collect();

					hard_matches.retain(|m| holders.iter().any(|&h| state.hands[h].contains(&m.order)));
				}
			}
		}
	}

	fn basic_gt_elim(&mut self, frame: &Frame, all_ids: &HashSet<Identity>, elim_candidates: &Vec<GTEntry>) -> (bool, Vec<Identity>) {
		let Frame { state, meta } = frame;
		let mut changed = false;
		let mut curr_ids: Vec<Identity> = all_ids.iter().cloned().collect();

		for i in 0..curr_ids.len() {
			let id = curr_ids[i];

			if let Some(soft_matches) = self.infer_map.get(&id) {
				let matches = self.certain_map.get(&id).unwrap_or(soft_matches);
				let maybe_bad_touch = !matches.iter().any(|m| meta[m.order].focused);

				let bad_elim = matches.iter().all(|m| {
					let visible_count = state.hands.concat().iter().filter(|&&o| state.deck[o].is(&id) && o != m.order).count();
					state.deck[m.order].id().map(|&i| i != id).unwrap_or(false) || state.base_count(&id) + visible_count == card_count(&state.variant, &id)
				});

				if bad_elim {
					continue;
				}

				for &GTEntry { order, player_index, cm } in elim_candidates {
					let thought = &mut self.thoughts[order];

					if matches.iter().any(|m| m.order == order) || thought.inferred.is_empty() || !thought.inferred.contains(&id) {
						continue;
					}

					let asymmetric_gt = !state.is_critical(&id) && (
						// Every match was clued by this player
						matches.iter().all(|m| {
							match state.deck[m.order].clues.first() {
								Some(clue) => {
									let original_turn = state.deck[order].clues.first().map(|cl| cl.turn).unwrap_or(0);
									clue.giver == player_index && clue.turn > original_turn
								},
								None => false,
							}
						} ||
						// This player was clued by every match
						match state.deck[order].clues.first(){
							Some(clue) => matches.iter().all(|m| state.holder_of(m.order).unwrap() == clue.giver),
							None => false
						})
					);

					if asymmetric_gt {
						continue;
					}

					if frame.is_blind_playing(order) && maybe_bad_touch {
						warn!("tried to gt elim {} from finessed card (order {})! could be bad touched", state.log_id(&id), order);
					}

					thought.inferred.retain(|i| i != &id);
					changed = true;

					if !cm {
						if thought.inferred.is_empty() && !thought.reset {
							thought.reset_inferences();
						}
						else if thought.inferred.len() == 1 {
							if let Some(i) = thought.inferred.iter().next() {
								if !state.is_basic_trash(i) {
									curr_ids.push(*i);
								}
							}
						}
					}
				}
			}
		}

		for &GTEntry { order, player_index, .. } in elim_candidates {
			self.add_to_maps(frame, order, player_index);
		}

		(changed, curr_ids)
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
						elim_candidates.push(GTEntry { order, player_index: i, cm: false });
					}
					else if meta[order].cm() {
						elim_candidates.push(GTEntry { order, player_index: i, cm: self.is_common });
					}
				}
			}
		}

		let mut all_ids: HashSet<Identity> = HashSet::from_iter(all_ids(&state.variant));
		let trash_ids: HashSet<Identity> = all_ids.iter().filter(|i| state.is_basic_trash(i)).cloned().collect();
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
		info!("eliminating link with inference {} from focus! original {:?}, final {}", state.log_id(&id), matches, focused_order);

		for &order in matches {
			let thought = &mut self.thoughts[*order];
			if order == focused_order {
				thought.inferred = HashSet::from([id]);
			}
			else {
				thought.inferred.retain(|i| i != &id);
			}

			if thought.inferred.is_empty() && !thought.reset {
				thought.reset_inferences();

				if good_touch {
					let mut inferred = thought.inferred.clone();
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
				self.elim_link(frame, &matches, focused_matches[0], *inferred.iter().next().unwrap(), good_touch);
				continue;
			}

			// We have enough inferred cards to elim elsewhere
			if matches.len() > inferred.len() {
				info!("adding link {:?} inferences {} ({})", matches, inferred.iter().map(|i| state.log_id(i)).join(","), if self.is_common { "common" } else { &state.player_names[self.player_index] });
				for o in &matches {
					linked_orders.insert(**o);
				}
				self.links.push(Link::Unpromised { orders: matches.into_iter().cloned().collect(), ids: inferred.iter().cloned().collect() });
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

					let viable_orders = orders.iter().filter(|&o| self.thoughts[*o].possible.contains(&id)).collect::<Vec<_>>();

					if viable_orders.is_empty() {
						info!("promised id {} not found among cards {:?}, rewind?", state.log_id(&id), orders)
					}
					else if viable_orders.len() == 1 {
						self.thoughts[*viable_orders[0]].inferred = HashSet::from([id]);
					}
					else {
						new_links.push(Link::Promised { orders: viable_orders.into_iter().cloned().collect(), id, target });
					}
				}
				Link::Unpromised { ref orders, ref ids } => {
					let revealed = orders.iter().filter(|&&o| {
						let thought = &self.thoughts[o];
						thought.id().is_some() || ids.iter().any(|i| !thought.possible.contains(i))
					}).collect::<Vec<_>>();

					if !revealed.is_empty() {
						continue;
					}

					let focused_orders = orders.iter().filter(|&&o| meta[o].focused).collect::<Vec<_>>();

					if focused_orders.len() == 1 && ids.len() == 1 {
						self.elim_link(frame, &orders.iter().collect(), focused_orders[0], *ids.iter().next().unwrap(), good_touch);
					}

					if let Some(lost_inference) = ids.iter().find(|&i| orders.iter().any(|&o| !self.thoughts[o].inferred.contains(i))) {
						info!("linked orders {:?} lost inference {}", orders, state.log_id(lost_inference));
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
