use core::f32;
use colored::Colorize;
use log::{info, warn};
use serde::Deserialize;

use crate::basics;
use crate::basics::card::{CardStatus, IdOptions, Identifiable, Identity};
use crate::basics::clue::ClueKind;
use crate::basics::endgame::{EndgameSolver};
use crate::basics::game::{Convention, frame::Frame, Game, Interp};
use crate::basics::action::{Action, ClueAction, DiscardAction, PerformAction, PlayAction, TurnAction};
use crate::basics::player::WaitingConnection;
use crate::basics::util;
use crate::fix::check_fix;

mod interpret_clue;
mod state_eval;

pub struct Reactor;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub enum ClueInterp {
	None, Mistake, Reactive, RefPlay, RefDiscard, Lock, Reveal, Fix, Reclue, Stall
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReactorInterp {
	Clue(ClueInterp),
}

impl Convention for Reactor {
	fn interpret_clue(&self, prev: &Game, game: &mut Game, action: &ClueAction) {
		let ClueAction { giver, target, .. } = &action;

		for &order in &game.state.hands[*giver] {
			let meta = &mut game.meta[order];
			if meta.urgent {
				warn!("removing status on {order}, didn't react");
				meta.status = CardStatus::None;
				meta.urgent = false;
				meta.trash = false;
				meta.focused = false;

				for &o in &game.state.hands.concat() {
					if game.meta[o].depends_on.map(|d| d == order).unwrap_or(false) {
						info!("removing associated dependency on {o}");
						game.meta[o].depends_on = None;
					}
				}
			}
		}

		// Force interpretation if rewinded
		let interp = if let Some(interp) = &game.next_interp {
			info!("forcing rewinded interp {interp:?}");
			if *interp == ClueInterp::Reactive {
				let reacter = game.state.next_player_index(*giver);
				Reactor::interpret_reactive(prev, game, action, reacter, false)
			}
			else {
				Reactor::interpret_stable(prev, game, action, false)
			}
		}
		else if prev.common.thinks_locked(&prev.frame(), *giver) || game.state.in_endgame() || (prev.state.clue_tokens == 8 && prev.state.turn_count != 1) {
			Reactor::interpret_stable(prev, game, action, true)
		}
		else {
			let mut reacter = None;

			for i in 1..game.state.num_players {
				let player_index = (giver + i) % game.state.num_players;

				// The clue may reveal a new playable, or the clue may fix a bad-touched card that looked playable previously
				let old_playables = prev.common.thinks_playables(&prev.frame(), player_index);
				let new_playables = game.common.thinks_playables(&game.frame(), player_index);
				let playables = old_playables.iter().filter(|o| new_playables.contains(o)).collect::<Vec<_>>();

				if playables.is_empty() {
					reacter = Some(player_index);
					info!("reacter is {}", game.state.player_names[player_index]);
					break;
				}
				else {
					info!("{} has playables {:?}, not reacter", game.state.player_names[player_index], playables);
				}
			}

			let (clued_resets, duplicate_reveals) = check_fix(prev, game, action);
			let allowable_fix = *target == game.state.next_player_index(*giver) && (!clued_resets.is_empty() || !duplicate_reveals.is_empty());

			match reacter {
				None => (allowable_fix).then_some(ClueInterp::Fix),
				Some(reacter) => {
					if &reacter == target {
						Reactor::interpret_stable(prev, game, action, false)
					}
					else {
						let prev_playables = prev.players[*target].thinks_playables(&prev.frame(), *target);

						// Urgent fix on previous playable
						if allowable_fix && clued_resets.iter().chain(duplicate_reveals.iter()).any(|o| prev_playables.contains(o)) {
							Some(ClueInterp::Fix)
						}
						else {
							Reactor::interpret_reactive(prev, game, action, reacter, false)
						}
					}
				}
			}
		};

		game.last_move = Some(Interp::Reactor(ReactorInterp::Clue(interp.unwrap_or(ClueInterp::None))));

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);
		basics::elim(game, true);
		game.next_interp = None;
	}

	fn interpret_discard(&self, prev: &Game, game: &mut Game, action: &DiscardAction) {
		let DiscardAction { player_index, order, failed, .. } = action;

		if *failed {
			warn!("bombed! not reacting");
			return;
		}

		if let Some(WaitingConnection { reacter, receiver, receiver_hand, clue, focus_slot, inverted, turn, .. }) = game.common.waiting.clone() {
			'wc_scope: {
				if *player_index != reacter {
					warn!("Had unrelated waiting connection! {:?}", game.common.waiting);
					break 'wc_scope;
				}

				let known_trash = prev.common.thinks_trash(&prev.frame(), reacter);

				// We were waiting for a response inversion and they reacted unnaturally
				if inverted {
					if if known_trash.is_empty() { prev.state.hands[reacter][0] != *order } else { !known_trash.contains(order) } {
						let rewind_turn = turn;
						match game.rewind(rewind_turn, Action::interp(ClueInterp::Reactive)) {
							Ok(new_game) => {
								*game = new_game;
								return;
							}
							Err(err) => warn!("Failed to rewind a response inversion! {err}")
						}
					}
					else {
						break 'wc_scope;
					}
				}

				let Game { common, state, meta, .. } = game;

				let react_slot = prev.state.hands[reacter].iter().position(|o| o == order).unwrap() + 1;
				let mut target_slot = (focus_slot + 5 - react_slot) % 5;
				if target_slot == 0 {
					target_slot = 5;
				}

				if receiver_hand.get(target_slot - 1).is_none() {
					warn!("Receiver no longer has slot {target_slot}!");
					return;
				}

				let receive_order = receiver_hand[target_slot - 1];
				if !state.hands[receiver].contains(&receive_order) {
					warn!("Receiver no longer holds target {receive_order}!");
					return;
				}

				let receive_thought = &mut common.thoughts[receive_order];
				let receive_meta = &mut meta[receive_order];

				match clue.kind {
					ClueKind::COLOUR => {
						receive_meta.status = CardStatus::CalledToPlay;
						receive_thought.inferred.retain(|i| state.is_playable(i));
						receive_meta.focused = true;

						info!("reactive dc+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {} (order {})",
							state.player_names[reacter], react_slot, state.player_names[receiver], target_slot, focus_slot, state.hands[receiver][target_slot - 1]);
					}
					ClueKind::RANK => {
						receive_meta.status = CardStatus::CalledToDiscard;
						receive_thought.inferred.retain(|i| state.is_basic_trash(i));
						receive_meta.trash = true;

						info!("reactive dc+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}",
							state.player_names[reacter], react_slot, state.player_names[receiver], target_slot, focus_slot);
					}
				}
			}
		}

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);
		basics::elim(game, true);
	}

	fn interpret_play(&self, prev: &Game, game: &mut Game, action: &PlayAction) {
		let PlayAction { player_index, order, .. } = action;

		if let Some(WaitingConnection { reacter, receiver, receiver_hand, clue, focus_slot, inverted, turn, .. }) = game.common.waiting.clone() {
			'wc_scope: {
				if *player_index != reacter {
					warn!("Had unrelated waiting connection! {:?}", game.common.waiting);
					break 'wc_scope;
				}

				let known_playables = prev.common.thinks_playables(&prev.frame(), reacter);

				// We were waiting for a response inversion and they reacted unnaturally
				if inverted {
					if !known_playables.contains(order) {
						let rewind_turn = turn;
						match game.rewind(rewind_turn, Action::interp(ClueInterp::Reactive)) {
							Ok(new_game) => {
								*game = new_game;
								return;
							}
							Err(err) => warn!("Failed to rewind a response inversion! {err}")
						}
					}
					else {
						break 'wc_scope;
					}
				}

				let Game { common, state, meta, .. } = game;

				let react_slot = prev.state.hands[reacter].iter().position(|o| o == order).unwrap() + 1;
				let mut target_slot = (focus_slot + 5 - react_slot) % 5;
				if target_slot == 0 {
					target_slot = 5;
				}

				if receiver_hand.get(target_slot - 1).is_none() {
					warn!("Receiver no longer has slot {target_slot}!");
					return;
				}

				let receive_order = receiver_hand[target_slot - 1];
				if !state.hands[receiver].contains(&receive_order) {
					warn!("Receiver no longer holds target {receive_order}!");
				}

				let receive_thought = &mut common.thoughts[receive_order];
				let receive_meta = &mut meta[receive_order];

				match clue.kind {
					ClueKind::RANK => {
						receive_meta.status = CardStatus::CalledToPlay;
						receive_thought.inferred.retain(|i| state.is_playable(i));
						receive_meta.focused = true;

						info!("reactive play+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", state.player_names[reacter], react_slot, state.player_names[receiver], target_slot, focus_slot);
					}
					ClueKind::COLOUR => {
						receive_meta.status = CardStatus::CalledToDiscard;
						receive_thought.inferred.retain(|i| state.is_basic_trash(i));
						receive_meta.trash = true;

						info!("reactive play+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", state.player_names[reacter], react_slot, state.player_names[receiver], target_slot, focus_slot);
					}
				}
			}
		}

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);
		basics::elim(game, true);
	}

	fn take_action(&self, game: &Game) -> PerformAction {
		let Game { state, meta, table_id, .. } = game;
		let frame = game.frame();
		let me = game.me();

		if let Some(urgent) = state.our_hand().iter().map(|&o| &meta[o]).find(|&t| t.urgent) {
			match urgent.status {
				CardStatus::CalledToPlay => {
					if !me.thoughts[urgent.order].possible.iter().all(|i| state.is_basic_trash(i)) {
						return PerformAction::Play { table_id: Some(*table_id), target: urgent.order }
					}
				}
				CardStatus::CalledToDiscard => {
					return PerformAction::Discard { table_id: Some(*table_id), target: urgent.order }
				}
				_ => {
					warn!("Unexpected urgent card status {:?}", urgent.status);
				}
			}
		}

		if state.in_endgame() && state.rem_score() <= state.variant.suits.len() + 1{
			info!("{}", "trying to solve endgame...".purple());

			let mut solver = EndgameSolver::new(true);
			let cloned_game = game.clone();
			let player_index = state.our_player_index;

			let result = solver.solve_game(&cloned_game, player_index);
			match result {
				Ok((perform, _)) => return perform,
				Err(err) => {
					info!("couldn't solve endgame: {err}");
				}
			}
		}

		let mut playable_orders = me.thinks_playables(&frame, state.our_player_index);
		let trash_orders = me.thinks_trash(&frame, state.our_player_index);

		// Retain only signalled playables if there is at least 1 such
		if playable_orders.iter().any(|&o| me.order_kp(&frame, o)) {
			playable_orders.retain(|&o| me.order_kp(&frame, o));
		}

		info!("playables {playable_orders:?}");
		info!("trash {trash_orders:?}");

		let all_clues = if state.clue_tokens == 0 { Vec::new() } else {
			(1..state.num_players).flat_map(|offset| {
				let target = (state.our_player_index + offset) % state.num_players;
				state.all_valid_clues(target)
			}).map(|clue| {
				let perform = util::clue_to_perform(&clue, *table_id);
				let action = util::perform_to_action(state, &perform, state.our_player_index, None);
				(perform, action)
			}).collect()
		};
		let num_clues = all_clues.len();

		let all_plays = playable_orders.iter().map(|&order| {
			(PerformAction::Play { table_id: Some(*table_id), target: order },
			match me.thoughts[order].identity(&IdOptions { infer: true, ..Default::default() }) {
				Some(Identity { suit_index, rank }) => {
					Action::play(state.our_player_index, order, suit_index as i32, rank as i32)
				}
				None => {
					Action::play(state.our_player_index, order, -1, -1)
				}
			})
		}).collect::<Vec<_>>();
		let num_plays = all_plays.len();

		let cant_discard = state.clue_tokens == 8 || (state.pace() == 0 && (num_clues > 0 || num_plays > 0));
		info!("can discard: {}", !cant_discard);

		let all_discards = if cant_discard { Vec::new() } else {
			trash_orders.iter().map(|&order| {
				(PerformAction::Discard { table_id: Some(*table_id), target: order },
				match me.thoughts[order].identity(&IdOptions { infer: true, ..Default::default() }) {
					Some(Identity { suit_index, rank }) => {
						Action::discard(state.our_player_index, order, suit_index as i32, rank as i32, false)
					}
					None => {
						Action::discard(state.our_player_index, order, -1, -1, false)
					}
				})
			}).collect::<Vec<_>>()
		};
		let num_discards = all_discards.len();

		let mut all_actions = all_clues.into_iter().chain(all_plays).chain(all_discards).collect::<Vec<_>>();

		if !cant_discard && (state.clue_tokens == 0 || num_plays == 0) && num_discards == 0 && !me.thinks_locked(&frame, state.our_player_index) {
			let chop = state.our_hand()[0];

			all_actions.push((
				PerformAction::Discard { table_id: Some(*table_id), target: chop },
				Action::discard(state.our_player_index, chop, -1, -1, false)
			));
		}

		if all_actions.is_empty() {
			return PerformAction::Discard { table_id: Some(*table_id), target: me.locked_discard(state, state.our_player_index) };
		}

		all_actions.iter().fold((f32::MIN, None), |(best_value, best), curr| {
			info!("{}", format!("===== Predicting value for ({}) =====", curr.1.fmt(state)).green());
			let value = Reactor::predict_value(game, &curr.1);
			if value > best_value {
				(value, Some(curr))
			} else {
				(best_value, best)
			}
		}).1.unwrap().0
	}

	fn update_turn(&self, _prev: &Game, game: &mut Game, action: &TurnAction) {
		let Game { common, state, .. } = game;
		let TurnAction { current_player_index, .. } = action;

		if *current_player_index != -1 {
			let last_player_index = (*current_player_index as usize + state.num_players - 1) % state.num_players;

			if let Some(wc) = &common.waiting && wc.reacter == last_player_index {
				common.waiting = None;
			}
		}
	}

	fn find_all_clues(&self, game: &Game, player_index: usize) -> Vec<PerformAction> {
		let Game { state, table_id, .. } = game;

		let level = log::max_level();
		log::set_max_level(log::LevelFilter::Off);

		let all_clues = (0..state.num_players)
			.filter(|&i| i != player_index)
			.flat_map(|i| state.all_valid_clues(i))
			.filter_map(|clue| {
				let base_clue = clue.to_base();
				let list = state.clue_touched(&state.hands[clue.target], &base_clue);

				let action = Action::Clue(ClueAction { giver: state.our_player_index, target: clue.target, list, clue: base_clue });
				let touched = state.clue_touched(&state.hands[clue.target], &base_clue);
				// Do not simulate clues that touch only previously-clued trash
				if touched.iter().all(|&o| state.deck[o].clued && state.is_basic_trash(state.deck[o].id().unwrap())) {
					return None;
				}
				info!("{}", format!("===== Predicting value for ({}) =====", action.fmt(state)).green());
				let value = Reactor::predict_value(game, &action);

				(value > -5.0).then_some(util::clue_to_perform(&clue, *table_id))
			})
			.collect();

		log::set_max_level(level);

		all_clues
	}

	fn find_all_discards(&self, game: &Game, player_index: usize) -> Vec<PerformAction> {
		let Game { common, state, table_id, .. } = game;
		let trash = common.thinks_trash(&game.frame(), player_index);

		if trash.is_empty() {
			vec![PerformAction::Discard { table_id: Some(*table_id), target: state.hands[player_index][0] }]
		}
		else {
			vec![trash.into_iter().map(|o| {
				PerformAction::Discard { table_id: Some(*table_id), target: o }
			}).next().unwrap()]
		}
	}
}
