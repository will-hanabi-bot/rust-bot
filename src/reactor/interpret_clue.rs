use itertools::Itertools;
use log::{info, warn};
use std::mem;
use std::sync::Arc;

use crate::basics;
use crate::basics::action::{Action, ClueAction};
use crate::basics::card::{CardStatus, Identifiable, Identity, IdOptions};
use crate::basics::clue::{Clue, ClueKind};
use crate::basics::game::{frame::Frame, Game};
use crate::basics::identity_set::IdentitySet;
use crate::basics::player::WaitingConnection;
use crate::basics::state::State;
use crate::basics::util::players_upto;
use crate::basics::variant::{touch_possibilities, BROWNISH, PINKISH, RAINBOWISH};
use crate::fix::{check_fix, connectable_simple};
use crate::reactor::{ClueInterp, Reactor};

impl Reactor {
	fn reactive_focus(state: &State, receiver: usize, action: &ClueAction) -> usize {
		let ClueAction { list, clue, .. } = &action;
		let (focus_index, _) = state.hands[receiver].iter().enumerate()
			.filter(|&(_, o)| list.contains(o))
			.max_by_key(|&(_, o)| if *o == state.hands[receiver][0] { 0 } else { *o })
			.unwrap();

		match clue.kind {
			ClueKind::COLOUR => if state.includes_variant(&RAINBOWISH) { clue.value + 1 } else { focus_index + 1 },
			ClueKind::RANK => if state.includes_variant(&PINKISH) { clue.value } else { focus_index + 1 }
		}
	}

	pub(super) fn interpret_stable(prev: &Game, game: &mut Game, action: &ClueAction, stall: bool) -> Option<ClueInterp> {
		let ClueAction { giver, target, .. } = &action;

		let interp = Reactor::try_stable(prev, game, action, stall);
		let bob = game.state.next_player_index(*giver);

		// Check for response inversion
		if *target != bob && *target != game.state.our_player_index && Reactor::bad_stable(prev, game, action, interp.as_ref().unwrap_or(&ClueInterp::Mistake), stall) {
			// Overwrite game with prev
			*game = prev.clone();
			let Game { state, .. } = game;
			let action_list = Arc::make_mut(&mut state.action_list);
			if action_list.len() <= state.turn_count {
				action_list.resize(state.turn_count + 1, Vec::new());
			}
			action_list[state.turn_count].push(Action::Clue(action.clone()));
			basics::on_clue(game, action);
			basics::elim(game, true);
			Reactor::interpret_reactive(prev, game, action, bob, true)
		}
		else {
			interp
		}
	}

	fn try_stable(prev: &Game, game: &mut Game, action: &ClueAction, stall: bool) -> Option<ClueInterp> {
		info!("interpreting stable clue!");
		let ClueAction { target, list, clue, giver } = &action;
		let (clued_resets, duplicate_reveals) = check_fix(prev, game, action);
		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();

		if clue.kind == ClueKind::RANK && !newly_touched.is_empty() {
			let mut focus = newly_touched.iter().max().unwrap();

			// Trash promise
			if (0..game.state.variant.suits.len()).all(|suit_index| game.state.is_basic_trash(Identity { suit_index, rank: clue.value })) {
				game.common.thoughts[*focus].inferred.retain(|i| game.state.is_basic_trash(i));
				game.meta[*focus].trash = true;
			}
			// Playable promise
			else if (0..game.state.variant.suits.len()).all(|suit_index| {
				let id = Identity { suit_index, rank: clue.value };
				game.state.is_basic_trash(id) || game.state.is_playable(id)
			}) {
				// Move focus to lock card if touched in a pinkish variant
				if let Some(lock_order) = game.state.hands[*target].iter().filter(|&&o| !prev.state.deck[o].clued).min() {
					if game.state.includes_variant(&PINKISH) && list.contains(lock_order) {
						focus = lock_order;
					}
				}

				let unnecessary_focus = game.common.thoughts[*focus].possible.iter().all(|i|
					game.state.is_basic_trash(i) || game.state.hands.concat().iter().any(|o| game.common.thoughts[*o].is(&i)));

				if unnecessary_focus {
					info!("unnecessary focus!");
				} else {
					game.common.thoughts[*focus].inferred.retain(|i| game.state.is_playable(i) && i.rank == clue.value);
					game.common.thoughts[*focus].info_lock = Some(game.common.thoughts[*focus].inferred);
					game.meta[*focus].focused = true;
				}
			}
		}

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);

		// Potential response inversion: don't allow response inversion if there's already a waiting connection
		if game.common.waiting.is_none() && game.state.next_player_index(*giver) != *target {
			let receiver = *target;

			let focus_slot = Reactor::reactive_focus(&game.state, receiver, action);

			game.common.waiting = Some(WaitingConnection {
				giver: *giver,
				reacter: game.state.next_player_index(*giver),
				receiver,
				receiver_hand: game.state.hands[receiver].clone(),
				clue: *clue,
				focus_slot,
				inverted: true,
				turn: game.state.turn_count
			});
		}

		if !clued_resets.is_empty() || !duplicate_reveals.is_empty() {
			info!("fix clue!");
			return Some(ClueInterp::Fix);
		}

		let Game { state, common, .. } = &game;
		let frame = game.frame();
		let prev_playables = prev.common.obvious_playables(&prev.frame(), *target).into_iter().chain(connectable_simple(prev, state.next_player_index(*giver), *target, None)).collect::<Vec<_>>();
		let playables = common.obvious_playables(&frame, *target).into_iter().chain(connectable_simple(game, state.next_player_index(*giver), *target, None)).collect::<Vec<_>>();

		info!("playables {playables:?}, prev_playables {prev_playables:?}");

		// Fill-in or hard burn is legal only in a stalling situation
		if newly_touched.is_empty() {
			if !playables.is_empty() && !prev_playables.is_empty() {
				info!("revealed a safe action!");
				return Some(ClueInterp::Reveal);
			}
			if stall {
				info!("stalling with fill-in/hard-burn!");
				return Some(ClueInterp::Stall);
			}
			warn!("looked like fill-in/hard burn outside of a stalling situation!");
			return None;
		}

		let colour_reveal = clue.kind == ClueKind::COLOUR && {
			let prev_playables = prev.common.obvious_playables(&prev.frame(), *target);
			let curr_playables = common.obvious_playables(&frame, *target);

			// A colour clue that reveals a new playable in a previously touched card
			curr_playables.iter().any(|o| !prev_playables.contains(o) && prev.state.deck[*o].clued)
		};

		let trash_push = !colour_reveal && common.order_kt(&frame, *newly_touched.iter().max().unwrap());
		if trash_push {
			// Brownish TCM if there is at least 1 useful unplayable brown and clue didn't touch chop
			if state.includes_variant(&BROWNISH) && clue.kind == ClueKind::RANK &&
				state.variant.suits.iter().enumerate().any(|(suit_index, suit)| BROWNISH.is_match(suit) && state.play_stacks[suit_index] + 1 < state.max_ranks[suit_index]) &&
				!newly_touched.contains(&state.hands[*target][0]) {
					info!("brown direct discard!");
			}
			else {
				info!("trash push!");
				return Reactor::ref_play(prev, game, action);
			}
		}

		let reveal = !playables.is_empty() && (clue.kind == ClueKind::RANK || colour_reveal);

		if reveal {
			info!("revealed a safe action!");
			return Some(ClueInterp::Reveal);
		}

		if clue.kind == ClueKind::COLOUR {
			info!("colour clue!");
			Reactor::ref_play(prev, game, action)
		}
		else {
			info!("rank clue!");
			Reactor::ref_discard(prev, game, action, stall)
		}
	}

	/**
	 * Returns true if there exists a non-bad touching ref play clue or a ref dc clue on trash to the clue target instead.
	 */
	fn alternative_clue(game: &Game, clue_target: usize) -> Option<Clue>{
		let Game { common, state,  .. } = game;

		if game.no_recurse {
			return None;
		}

		for clue in state.all_valid_clues(clue_target) {
			let base_clue = clue.to_base();
			let list = state.clue_touched(&state.hands[clue_target], &base_clue);

			let hand = &state.hands[clue_target];
			let newly_touched = list.iter().filter(|&&o| !state.deck[o].clued).copied().collect::<Vec<_>>();

			if newly_touched.is_empty() {
				continue;
			}

			let valid_clue = match clue.kind {
				ClueKind::COLOUR => {
					let play_target = newly_touched.iter().map(|&o| common.refer(&game.frame(), hand, o, true)).max().unwrap();

					state.is_playable(state.deck[play_target].id().unwrap()) &&
					(newly_touched.iter().all(|&o| !state.is_basic_trash(state.deck[o].id().unwrap())) ||
						newly_touched.iter().all(|&o| common.thoughts[o].possible.intersect(&IdentitySet::from_iter(touch_possibilities(&base_clue, &state.variant))).iter().all(|i| state.is_basic_trash(i))))
				}
				ClueKind::RANK => {
					if let Some(lock_order) = hand.iter().filter(|&&o| !state.deck[o].clued).min() && list.contains(lock_order) {
						continue;
					}

					let focus = newly_touched.iter().max().unwrap();
					let focus_pos = hand.iter().position(|o| o == focus).unwrap();
					let target_index = hand.iter().enumerate().position(|(i, &o)| i > focus_pos && !state.deck[o].clued).unwrap();

					state.is_basic_trash(state.deck[hand[target_index]].id().unwrap())
				}
			};

			if valid_clue {
				return Some(clue);
			}
		}
		None
	}

	pub(super) fn bad_stable(prev: &Game, game: &Game, action: &ClueAction, interp: &ClueInterp, stall: bool) -> bool {
		let Game { common, state, meta, .. } = game;
		let ClueAction { target, .. } = action;

		if *interp == ClueInterp::Illegal {
			return false;
		}

		if *interp == ClueInterp::Mistake {
			return true;
		}

		let bad_playable = state.hands[*target].iter().find(|&&o|
			meta[o].status == CardStatus::CalledToPlay && prev.meta[o].status != CardStatus::CalledToPlay && !game.state.has_consistent_inferences(&common.thoughts[o]));

		if let Some(bad) = bad_playable {
			warn!("bad playable on {bad} {}!", state.log_iden(&state.deck[*bad]));
			return true;
		}

		let bad_discard = state.hands[*target].iter().find(|&&o|
			meta[o].status == CardStatus::CalledToDiscard && prev.meta[o].status != CardStatus::CalledToDiscard &&
			(state.is_critical(state.deck[o].id().unwrap()) ||
				(stall && !state.is_basic_trash(state.deck[o].id().unwrap()) && Reactor::alternative_clue(game, *target).is_some()))
		);

		if let Some(bad) = bad_discard {
			warn!("bad discard on {bad}!");
			return true;
		}

		// Check for bad lock
		if *interp == ClueInterp::Lock {
			if let Some(alt_clue) = Reactor::alternative_clue(game, *target) {
				warn!("alternative clue {} was available!", alt_clue.fmt(state));
				return true;
			}
		}

		if !stall {
			return false;
		}

		// Check for bad stall
		if *interp == ClueInterp::Stall {
			if let Some(alt_clue) = Reactor::alternative_clue(game, *target) {
				warn!("alternative clue {} was available!", alt_clue.fmt(state));
				return true;
			}
		}

		false
	}

	pub(super) fn interpret_reactive(prev: &Game, game: &mut Game, action: &ClueAction, reacter: usize, inverted: bool) -> Option<ClueInterp> {
		let Game { common, state, .. } = game;
		let ClueAction { giver, target: receiver, clue, .. } = action;

		info!("interpreting reactive clue!");
		info!("reacter: {:?} ({}), receiver: {:?} ({})", state.hands[reacter], state.player_names[reacter], state.hands[*receiver], state.player_names[*receiver]);

		let focus_slot = Reactor::reactive_focus(state, *receiver, action);

		common.waiting = Some(WaitingConnection {
			giver: *giver,
			reacter,
			receiver: *receiver,
			receiver_hand: state.hands[*receiver].clone(),
			clue: *clue,
			focus_slot,
			inverted: false,
			turn: state.turn_count
		});

		if *receiver == state.our_player_index {
			return Some(ClueInterp::Reactive);
		}

		let possible_conns = Reactor::delayed_plays(game, *giver, *receiver);

		let old_playables = prev.common.obvious_playables(&prev.frame(), *receiver);
		let new_playables = game.common.obvious_playables(&game.frame(), *receiver);
		let known_plays = old_playables.iter().filter(|o| new_playables.contains(o)).collect::<Vec<_>>();

		let Game { common, state, meta, .. } = game;

		match clue.kind {
			ClueKind::COLOUR => {
				let play_targets = state.hands[*receiver].iter().enumerate()
					.filter(|&(_, o)| meta[*o].status != CardStatus::CalledToDiscard && !known_plays.contains(&o) && state.is_playable(state.deck[*o].id().unwrap()))
					.sorted_by_key(|&(i, o)|
						// Unclued dupe, with a clued dupe
						if !prev.state.deck[*o].clued && state.hands[*receiver].iter().any(|o2| o2 < o && prev.state.deck[*o2].clued && state.deck[*o].is(&state.deck[*o2])) {
							99
						} else { i }
					);

				// Try targeting all play targets
				for (index, _) in play_targets {
					let target_slot = index + 1;
					let react_slot = Reactor::calc_slot(focus_slot, target_slot);

					if state.hands[reacter].get(react_slot - 1).is_none() {
						warn!("Reacter doesn't have slot {react_slot}!");
						continue;
					}

					let react_order = state.hands[reacter][react_slot - 1];
					let prev_trash = prev.common.thinks_trash(&prev.frame(), reacter);
					if prev_trash.contains(&react_order) || (inverted && prev_trash.is_empty() && react_slot == 1) {
						warn!("attempted dc+play would result in reacter naturally discarding {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					if common.thoughts[react_order].possible.iter().all(|i| state.is_critical(i)) {
						warn!("attempted dc+play would result in reacter discarding known critical {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					common.thoughts[react_order].old_inferred = Some(common.thoughts[react_order].inferred);
					Reactor::target_discard(game, action, react_order, true);

					info!("reactive dc+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
					return Some(ClueInterp::Reactive);
				}

				// Didn't work, so target trash
				let prev_kt = prev.common.thinks_trash(&prev.frame(), *receiver);

				let mut targets = state.hands[*receiver].iter().enumerate().filter(|&(_, o)|
					!prev_kt.contains(o) &&
					(state.is_basic_trash(state.deck[*o].id().unwrap()) ||
						state.hands[*receiver].iter().any(|o2| o2 != o && state.deck[*o].is(&state.deck[*o2])))		// duped in the same hand
				)
				.sorted_by_key(|(_, o)|
					if prev.state.deck[**o].clued {
						0
					} else if state.hands[*receiver].iter().any(|o2| o2 < o && prev.state.deck[*o2].clued && state.deck[**o].is(&state.deck[*o2])) {
						-1		// Unclued dupe, with a clued dupe
					} else {
						1
					}
				)
				.collect::<Vec<_>>();

				// Add sacrifice discard targets
				if targets.is_empty() {
					targets.extend(state.hands[*receiver].iter().enumerate().filter(|&(_, o)|
						!prev_kt.contains(o) && !state.is_critical(state.deck[*o].id().unwrap())
					).sorted_by_key(|(_, o)|
						-common.playable_away(state.deck[**o].id().unwrap())
					));

					if targets.is_empty() {
						warn!("reactive clue but receiver had no playable, trash or sacrifice targets!");
						return None;
					}
				}

				for (index, target) in targets {
					if state.next_player_index(*giver) != reacter && meta[*target].status == CardStatus::CalledToPlay {
						warn!("can't target previously-playable trash with a reverse reactive clue!");
						continue;
					}

					let target_slot = index + 1;
					let react_slot = Reactor::calc_slot(focus_slot, target_slot);

					if state.hands[reacter].get(react_slot - 1).is_none() {
						warn!("reacter doesn't have slot {react_slot}!");
						continue;
					}

					let react_order = state.hands[reacter][react_slot - 1];
					let prev_plays = prev.common.obvious_playables(&prev.frame(), reacter);
					if prev_plays.contains(&react_order) {
						warn!("attempted play+dc would result in reacter naturally playing {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					if !common.thoughts[react_order].possible.iter().any(|i| state.is_playable(i) || possible_conns.iter().any(|p| p.1 == i)) {
						warn!("reaction would involve playing unplayable {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					game.common.thoughts[react_order].old_inferred = Some(game.common.thoughts[react_order].inferred);
					Reactor::target_play(game, action, react_order, true, false)?;
					info!("reactive play+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
					return Some(ClueInterp::Reactive);
				}
				None
			}
			ClueKind::RANK => {
				let play_targets = state.hands[*receiver].iter().enumerate().filter(|&(_, o)|
					meta[*o].status != CardStatus::CalledToDiscard && !known_plays.contains(&o) && state.is_playable(state.deck[*o].id().unwrap())
				).sorted_by_key(|(i, o)| {
					// Do not target an unclued copy when there is a clued copy
					let unclued_dupe = !prev.state.deck[**o].clued && state.hands[*receiver].iter().any(|o2| &o2 != o && prev.state.deck[*o2].clued && state.deck[**o].is(&state.deck[*o2]));
					if unclued_dupe { 99 } else { *i }
				});

				for (index, target) in play_targets {
					let target_slot = index + 1;
					let react_slot = Reactor::calc_slot(focus_slot, target_slot);

					if state.hands[reacter].get(react_slot - 1).is_none() {
						warn!("reacter doesn't have slot {react_slot}!");
						continue;
					}

					let react_order = state.hands[reacter][react_slot - 1];
					let receive_order = *target;

					let prev_plays = prev.common.obvious_playables(&prev.frame(), reacter);
					if prev_plays.contains(&react_order) {
						warn!("attempted play+play would result in reacter naturally playing {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					if !common.thoughts[react_order].possible.iter().any(|i| state.is_playable(i) || possible_conns.iter().any(|p| p.1 == i)) {
						warn!("reaction would involve playing unplayable {} {react_order}!", state.log_iden(&state.deck[react_order]));
						continue;
					}

					Reactor::target_play(game, action, react_order, true, false)?;
					game.common.thoughts[react_order].inferred.retain(|i| i != game.state.deck[receive_order].id().unwrap());

					info!("reactive play+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
					return Some(ClueInterp::Reactive);
				}

				let finesse_targets = state.hands[*receiver].iter().enumerate().filter(|(_, o)|
					state.playable_away(state.deck[**o].id().unwrap()) == 1
				).collect::<Vec<_>>();

				if finesse_targets.is_empty() {
					warn!("reactive clue but receiver had no playable targets!");
					return None;
				}

				for react_slot in [1, 5, 4, 3, 2] {
					let target_slot = Reactor::calc_slot(focus_slot, react_slot);

					if state.hands[reacter].get(react_slot - 1).is_none() {
						continue;
					}

					if let Some((_,finesse_target)) = finesse_targets.iter().find(|(i, _)| i + 1 == target_slot) {
						let react_order = state.hands[reacter][react_slot - 1];
						let receive_order = **finesse_target;

						let prev_plays = prev.common.obvious_playables(&prev.frame(), reacter);
						if prev_plays.contains(&react_order) {
							warn!("attempted finesse would result in reacter naturally playing {} {react_order}!", state.log_iden(&state.deck[react_order]));
							return None;
						}

						if !common.thoughts[react_order].possible.iter().any(|i| state.is_playable(i) || possible_conns.iter().any(|p| p.1 == i)) {
							warn!("reaction would involve playing unplayable {} {react_order}!", state.log_iden(&state.deck[react_order]));
							continue;
						}

						common.thoughts[react_order].old_inferred = Some(common.thoughts[react_order].inferred);
						Reactor::target_play(game, action, react_order, true, false)?;
						game.common.thoughts[react_order].inferred = IdentitySet::single(game.state.deck[receive_order].id().unwrap().prev());

						info!("reactive finesse, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
						return Some(ClueInterp::Reactive);
					}
				}
				None
			}
		}
	}

	fn delayed_plays(game: &Game, giver: usize, receiver: usize) -> Vec<(usize, Identity)> {
		let Game { common, state, meta, .. } = game;

		let mut possible_conns = Vec::new();

		for player_index in players_upto(state.num_players, state.next_player_index(giver), receiver) {
			let mut playables = common.obvious_playables(&game.frame(), player_index);

			// If they have an urgent discard, they can't play a connecting card. If they have an urgent playable, they can only play that card.
			if let Some(urgent) = state.hands[player_index].iter().find(|&&o| meta[o].urgent) {
				if meta[*urgent].trash {
					continue;
				} else {
					playables = vec![*urgent];
				}
			}

			for &o in &playables {
				// Only consider playing the leftmost of similarly-possible cards
				if playables.iter().any(|&p| p > o && common.thoughts[p].possible == common.thoughts[o].possible) {
					continue;
				}

				if let Some(id) = common.thoughts[o].identity(&IdOptions { infer: true, ..Default::default() }) {
					possible_conns.push((o, Identity { suit_index: id.suit_index, rank: id.rank + 1 }));
				}
				else {
					possible_conns.extend(common.thoughts[o].inferred.iter().map(|i| (o, Identity { suit_index: i.suit_index, rank: i.rank + 1 })));
				}
			}
		}
		possible_conns
	}

	fn ref_play(prev: &Game, game: &mut Game, action: &ClueAction) -> Option<ClueInterp> {
		let Game { common, state, .. } = game;
		let ClueAction { target: receiver, list, .. } = &action;
		let hand = &state.hands[*receiver];
		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();

		let target = newly_touched.iter().map(|&o| common.refer(&prev.frame(), hand, o, true)).max().unwrap();

		if game.frame().is_blind_playing(target) {
			warn!("targeting an already known playable!");
			return None;
		}

		if game.meta[target].status == CardStatus::CalledToDiscard {
			warn!("targeting a card called to discard!");
			return None;
		}

		Reactor::target_play(game, action, target, false, true)
	}

	fn target_play(game: &mut Game, action: &ClueAction, target: usize, urgent: bool, stable: bool) -> Option<ClueInterp> {
		let ClueAction { giver, .. } = action;
		let holder = game.state.holder_of(target).unwrap();
		let possible_conns = Reactor::delayed_plays(game, *giver, holder);


		let Game { common, state, .. } = game;
		let new_inferred = common.thoughts[target].inferred.filter(|i| state.is_playable(i) || possible_conns.iter().any(|p| p.1 == i));

		if let Some(id) = game.state.deck[target].id() && let Some((conn_order, _)) = possible_conns.iter().find(|c| c.1.is(&id)) {
			game.common.thoughts[*conn_order].old_inferred = Some(game.common.thoughts[*conn_order].inferred);
			game.common.thoughts[*conn_order].inferred = IdentitySet::single(id.prev());

			let meta = &mut game.meta[*conn_order];
			meta.urgent = true;
			meta.status = CardStatus::CalledToPlay;
			if meta.reasoning.last().map(|r| *r != game.state.turn_count).unwrap_or(true) {
				meta.reasoning.push(game.state.turn_count);
			}

			info!("updating connecting {} as {} to be urgent", *conn_order, game.state.log_id(id.prev()));
		}

		let reset = new_inferred.is_empty();
		game.common.thoughts[target].old_inferred = Some(game.common.thoughts[target].inferred);
		game.common.thoughts[target].inferred = new_inferred;

		if game.meta[target].reasoning.last().map(|r| *r != game.state.turn_count).unwrap_or(true) {
			game.meta[target].reasoning.push(game.state.turn_count);
		}

		if reset || !game.state.has_consistent_inferences(&game.common.thoughts[target]) {
			game.common.thoughts[target].reset_inferences();
			warn!("target {target} was reset!");

			// If the target is fully known trash, this is an acceptable stall.
			let interp = if stable {
				if game.common.order_kt(&game.frame(), target) {
					Some(ClueInterp::Stall)
				}
				else if *giver == game.state.our_player_index {
					Some(ClueInterp::Illegal)
				} else {
					None
				}
			} else { None };
			return interp;
		}

		game.common.thoughts[target].info_lock = Some(new_inferred);

		let Game { common, state, .. } = game;
		let meta = &mut game.meta[target];

		meta.status = CardStatus::CalledToPlay;
		meta.focused = true;
		if urgent {
			meta.urgent = true;
		}

		info!("targeting play {} ({}), infs {}{}", target, state.player_names[holder], common.str_infs(state, target), if urgent { ", urgent" } else { "" });
		Some(ClueInterp::RefPlay)
	}

	fn target_discard(game: &mut Game, action: &ClueAction, target: usize, urgent: bool) {
		let ClueAction { target: clue_target, .. } = action;

		let mut inferred = mem::take(&mut game.common.thoughts[target].inferred);
		inferred.retain(|i| !game.state.is_critical(i));
		game.common.thoughts[target].inferred = inferred;

		let Game { common, state, .. } = game;
		let meta = &mut game.meta[target];

		meta.status = CardStatus::CalledToDiscard;
		meta.trash = true;
		if urgent {
			meta.urgent = true;
		}
		if meta.reasoning.last().map(|r| *r != state.turn_count).unwrap_or(true) {
			meta.reasoning.push(state.turn_count);
		}

		info!("targeting discard {} ({}), infs {}{}", target, state.player_names[*clue_target], common.str_infs(state, target), if urgent { ", urgent" } else { "" });
	}

	fn ref_discard(prev: &Game, game: &mut Game, action: &ClueAction, stall: bool) -> Option<ClueInterp> {
		let Game { state, .. } = game;
		let ClueAction { target: receiver, list, clue, giver } = &action;
		let hand = &state.hands[*receiver];
		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();

		if let Some(lock_order) = hand.iter().filter(|&&o| !prev.state.deck[o].clued).min() && list.contains(lock_order) {
			// In a stalling situation, cluing Cathy's lock card is a stall.
			if stall && state.next_player_index(*receiver) == *giver {
				info!("stall to Cathy's lock card!");
				return Some(ClueInterp::Stall);
			}
			info!("locked!");

			// Lock pink promise
			if clue.kind == ClueKind::RANK && state.includes_variant(&PINKISH) {
				game.common.thoughts[*lock_order].inferred.retain(|i| i.rank == clue.value);
				game.meta[*lock_order].focused = true;
			}

			for &order in hand {
				let meta = &mut game.meta[order];

				if !state.deck[order].clued && meta.status == CardStatus::None {
					meta.status = CardStatus::ChopMoved;
					if meta.reasoning.last().map(|r| *r != state.turn_count).unwrap_or(true) {
						meta.reasoning.push(state.turn_count);
					}
				}
			}
			return Some(ClueInterp::Lock);
		}

		let focus = newly_touched.iter().max().unwrap();
		let focus_pos = hand.iter().position(|o| o == focus).unwrap();
		let target_index = hand.iter().enumerate().position(|(i, &o)| i > focus_pos && !state.deck[o].clued).unwrap();
		info!("ref discard on {}'s slot {}", state.player_names[*receiver], target_index + 1);

		let meta = &mut game.meta[hand[target_index]];
		meta.status = CardStatus::CalledToDiscard;
		meta.trash = true;
		if meta.reasoning.last().map(|r| *r != state.turn_count).unwrap_or(true) {
			meta.reasoning.push(state.turn_count);
		}
		Some(ClueInterp::RefDiscard)
	}
}
