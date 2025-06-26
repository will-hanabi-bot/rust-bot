use core::f32;
use colored::Colorize;
use log::{info, warn};

use crate::basics;
use crate::basics::card::{CardStatus, IdOptions, Identifiable, Identity};
use crate::basics::clue::ClueKind;
use crate::basics::clue_result::{bad_touch_result, elim_result, playables_result, BadTouchResult, ElimResult, PlayablesResult};
use crate::basics::game::{Convention, frame::Frame, Game, Interp};
use crate::basics::action::{Action, ClueAction, DiscardAction, PerformAction, PlayAction, TurnAction};
use crate::basics::player::WaitingConnection;
use crate::basics::util;

pub mod interpret_clue;

pub struct Reactor;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClueInterp {
	None, Mistake, Reactive, RefPlay, RefDiscard, Lock, Reveal, Reclue, Stall
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReactorInterp {
	Clue(ClueInterp),
}

impl Reactor {
	fn get_result(game: &Game, hypo: &Game, action: &ClueAction) -> f32 {
		let Game { state, common, .. } = game;
		let Game { state: hypo_state, common: hypo_common, .. } = hypo;
		let hypo_frame = hypo.frame();
		let ClueAction { giver, target, list, clue } = &action;

		let BadTouchResult { bad_touch, trash, .. } = bad_touch_result(game, hypo, *giver, *target);
		let ElimResult { new_touched, fill, elim } = elim_result(game, hypo, &hypo_state.hands[*target], list);
		let PlayablesResult { playables, .. } = playables_result(game, hypo);

		let revealed_trash = hypo_common.thinks_trash(&hypo_frame, *target).iter().filter(|&o|
			hypo_state.deck[*o].clued && !common.thinks_trash(&game.frame(), *target).contains(o)).count();

		let bad_playable = state.hands.concat().into_iter().find(|o| hypo.meta[*o].status == CardStatus::CalledToPlay && !hypo.me().hypo_plays.contains(o));

		if let Some(bad_playable) = bad_playable {
			warn!("clue {} results in {} looking playable!", clue.fmt(state, *target), state.deck[bad_playable].id().map(|&i| i.fmt(&state.variant)).unwrap_or(format!("order {}", bad_playable)));
			return -100.0;
		}

		if let Some(Interp::Reactor(ReactorInterp::Clue(last_move))) = &hypo.last_move {
			if (last_move == &ClueInterp::RefPlay || last_move == &ClueInterp::Reclue) && playables.is_empty() {
				warn!("clue {} looks like {:?} but gets no playables!", clue.fmt(state, *target), last_move);
				return -100.0;
			}

			if last_move == &ClueInterp::Reveal && playables.is_empty() && !trash.is_empty() && trash.iter().all(|o| !state.deck[*o].clued) {
				warn!("clue {} only reveals new trash but isn't a trash push!", clue.fmt(state, *target));
				return -100.0;
			}
		}

		let duped_playables = hypo.me().hypo_plays.iter().filter(|&&p|
			state.hands.concat().iter().any(|&o| o != p && game.frame().is_touched(o) && state.deck[o].is(&state.deck[p]))).count();

		let good_touch = if bad_touch.len() >= new_touched.len() { -(bad_touch.len() as f32) } else { [0.0, 0.25, 0.5, 0.6, 0.7, 0.75][new_touched.len() - bad_touch.len()] };

		let untouched_plays = playables.iter().filter(|&&p| !hypo_state.deck[p].clued).count();

		info!("good touch: {}, playables: [{}], duped: {}, trash: {}, fill: {}, elim: {}, bad_touch: {:?}",
			good_touch,
			playables.iter().map(|&o| state.deck[o].id().map(|&i| i.fmt(&state.variant)).unwrap_or("xx".to_string())).collect::<Vec<String>>().join(", "),
			duped_playables,
			trash.len(),
			fill.len(),
			elim.len(),
			bad_touch
		);

		let value: f32 = good_touch
			+ (playables.len() as f32 - 2.0*duped_playables as f32)
			+ 0.2 * untouched_plays as f32
			+ 0.1 * revealed_trash as f32
			+ 0.1 * fill.len() as f32
			+ 0.05 * elim.len() as f32
			+ 0.1 * bad_touch.len() as f32;

		value
	}

	fn advance_game(game: &Game, action: &Action) -> Game {
		match action {
			Action::Clue(clue) => game.simulate_clue(clue),
			_ => game.simulate_action(action)
		}
	}

	fn best_value(game: &Game, offset: usize, value: f32) -> f32 {
		let Game { state, common, .. } = game;
		let frame = game.frame();
		let player_index = (state.our_player_index + offset) % state.num_players;

		if player_index == state.our_player_index || state.endgame_turns.map(|t| t == 0).unwrap_or(false) {
			return value;
		}

		let mult = |x|
			x * (if offset == 1 || state.clue_tokens == 0 {
				if x < 0.0 { 1.25 } else { 0.25 }
			} else { 0.1 });

		let sieving_trash = || {
			if state.in_endgame() || state.max_score() - state.score() < state.variant.suits.len() {
				return false;
			}

			let chop = state.hands[player_index][0];
			let id = state.deck[chop].id().unwrap();

			state.is_basic_trash(id) || game.me().is_sieved(&frame, id, chop)
		};

		let playables = common.thinks_playables(&frame, player_index);
		if !playables.is_empty() {
			let play_actions = playables.iter().map(|&order| {
				let id = state.deck[order].id().unwrap();
				let Identity { suit_index, rank } = id;
				let action = Action::play(player_index, order, *suit_index as i32, *rank as i32);

				let diff = if state.is_playable(id) {
					if *rank == 5 { 1.75 } else { 1.5 }
				} else {
					-10.0
				} + if sieving_trash() { -10.0 } else { 0.0 };
				let new_value = value + mult(diff);

				info!("{} playing {} {}{}", state.player_names[player_index], id.fmt(&state.variant), mult(diff), if sieving_trash() { ", sieving trash!" } else { "" });
				Reactor::best_value(&Reactor::advance_game(game, &action), offset + 1, new_value)
			});
			return play_actions.fold(f32::MIN, |a, b| a.max(b));
		}

		if common.thinks_locked(&frame, player_index) || (offset == 1 && state.clue_tokens == 8) {
			if state.clue_tokens == 0 {
				warn!("forcing discard at 0 clues from locked hand!");
				return -15.0;
			}

			let mut next_game = game.simulate_clean();
			next_game.state.clue_tokens -= 1;

			let diff = if state.clue_tokens == 0 || sieving_trash() { -10.0 } else { 0.25 };
			let new_value = value + mult(diff);

			info!("{} forced clue {}", state.player_names[player_index], mult(diff));
			return Reactor::best_value(&next_game, offset + 1, new_value);
		}

		let trash = common.thinks_trash(&frame, player_index);
		let discard = trash.first().unwrap_or(&state.hands[player_index][0]);
		let id = state.deck[*discard].id().unwrap();
		let Identity { suit_index, rank } = id;
		let action = Action::discard(player_index, *discard, *suit_index as i32, *rank as i32, false);

		let dc_value = game.me().card_value(&frame, id, Some(*discard)) as f32;

		let diff = (if state.in_endgame() { 0.0 } else { 10.0 } as f32)
			.min(0.25 + if dc_value == 0.0 { 1.0 } else { - dc_value*0.5 })
				+ if *discard != state.hands[player_index][0] && sieving_trash() { -10.0 } else { 0.0 };
		let new_value = value + mult(diff);

		info!("{} discarding {} {}{}", state.player_names[player_index], id.fmt(&state.variant), mult(diff), if *discard != state.hands[player_index][0] && sieving_trash() { ", sieving trash!" } else { "" });
		Reactor::best_value(&Reactor::advance_game(game, &action), offset + 1, new_value)
	}

	fn predict_value(game: &Game, action: &Action) -> f32 {
		let Game { state, common, .. } = game;
		let hypo_game = Reactor::advance_game(game, action);

		let value = match action {
			Action::Clue(clue) => {
				if let Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::None))) = hypo_game.last_move {
					return -100.0;
				}

				let mult = if !game.me().thinks_playables(&game.frame(), state.our_player_index).is_empty() {
					if state.in_endgame() { 0.1 } else { 0.25 }
				} else {
					0.5
				};

				Reactor::get_result(game, &hypo_game, clue) * mult - 0.25
			},
			Action::Discard(DiscardAction { player_index, order, .. }) => {
				let mult = if state.in_endgame() { 0.2 } else { 1.0 };

				mult * if common.thinks_trash(&game.frame(), *player_index).contains(order) { 1.2 } else { 0.5 }
			},
			Action::Play(PlayAction { order, suit_index, rank, .. }) => {
				if *suit_index == -1 || *rank == -1 {
					1.5
				}
				else {
					let id = Identity { suit_index: *suit_index as usize, rank: *rank as usize };

					let duplicated = state.hands.concat().iter().any(|o|
						o != order && game.frame().is_touched(*o) &&
						state.deck[*o].is(&id) &&
						common.thoughts[*o].inferred.iter().any(|i| *i != id && !state.is_basic_trash(i)));

					if duplicated { if state.in_endgame() { 0.5 } else { 0.0 } } else { 1.5 }
				}
			},
			_ => -1.0
		};
		info!("starting value {}", value);

		let best = Reactor::best_value(&hypo_game, 1, value);
		info!("{}: {} ({:?})", action.fmt(state), best, hypo_game.last_move.unwrap());
		best
	}
}

impl Convention for Reactor {
	fn interpret_clue(&self, game: &mut Game, action: &ClueAction) {
		let prev = game.clone();
		basics::on_clue(game, action);
		basics::elim(game, true);

		let ClueAction { giver, target, .. } = &action;

		let interp = if prev.common.thinks_locked(&prev.frame(), *giver) || game.state.in_endgame() {
			Reactor::interpret_stable(&prev, game, action)
		}
		else {
			let mut reacter = None;

			for i in 1..game.state.num_players {
				let player_index = (giver + i) % game.state.num_players;

				// The clue may reveal a new playable, or the clue may fix a bad-touched card that looked playable previously
				let old_playables = prev.common.thinks_playables(&game.frame(), player_index);
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

			match reacter {
				None => {
					ClueInterp::None
				}
				Some(reacter) => {
					if &reacter == target {
						Reactor::interpret_stable(&prev, game, action)
					}
					else {
						Reactor::interpret_reactive(&prev, game, action, reacter)
					}
				}
			}
		};

		game.last_move = Some(Interp::Reactor(ReactorInterp::Clue(interp)));

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);
		basics::elim(game, true);
	}

	fn interpret_discard(&self, game: &mut Game, action: &DiscardAction) {
		let prev = game.clone();
		let DiscardAction { player_index, order, failed, .. } = action;

		basics::on_discard(game, action);

		if *failed {
			warn!("bombed! not reacting");
			return;
		}

		let Game { common, state, meta, .. } = game;
		if let Some(WaitingConnection { reacter, receiver, receiver_hand, clue, focus_slot, .. }) = common.waiting.first() {
			if player_index != reacter {
				warn!("Had unrelated waiting connection! {:?}", common.waiting[0]);
				return;
			}

			let react_slot = prev.state.hands[*reacter].iter().position(|o| o == order).unwrap() + 1;
			let mut target_slot = (focus_slot + 5 - react_slot) % 5;
			if target_slot == 0 {
				target_slot = 5;
			}

			let receive_order = receiver_hand[target_slot - 1];
			if !state.hands[*receiver].contains(&receive_order) {
				warn!("Receiver no longer holds target {}!", receive_order);
			}

			let receive_thought = &mut common.thoughts[receive_order];
			let receive_meta = &mut meta[receive_order];

			match clue.kind {
				ClueKind::COLOUR => {
					receive_meta.status = CardStatus::CalledToPlay;
					receive_thought.inferred.retain(|i| state.is_playable(i));
					receive_meta.focused = true;

					info!("reactive dc+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {} (order {})", state.player_names[*reacter], react_slot, state.player_names[*receiver], target_slot, focus_slot, state.hands[*receiver][target_slot - 1]);
				}
				ClueKind::RANK => {
					receive_meta.status = CardStatus::CalledToDiscard;
					receive_thought.inferred.retain(|i| state.is_basic_trash(i));
					receive_meta.trash = true;

					info!("reactive dc+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", state.player_names[*reacter], react_slot, state.player_names[*receiver], target_slot, focus_slot);
				}
			}
		}

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);
		basics::elim(game, true);
	}

	fn interpret_play(&self, game: &mut Game, action: &PlayAction) {
		let prev = game.clone();
		let PlayAction { player_index, order, .. } = action;

		basics::on_play(game, action);

		let Game { common, state, meta, .. } = game;
		if let Some(WaitingConnection { reacter, receiver, receiver_hand, clue, focus_slot, .. }) = common.waiting.first() {
			if player_index != reacter {
				warn!("Had unrelated waiting connection! {:?}", common.waiting[0]);
				return;
			}

			let react_slot = prev.state.hands[*reacter].iter().position(|o| o == order).unwrap() + 1;
			let mut target_slot = (focus_slot + 5 - react_slot) % 5;
			if target_slot == 0 {
				target_slot = 5;
			}

			let receive_order = receiver_hand[target_slot - 1];
			if !state.hands[*receiver].contains(&receive_order) {
				warn!("Receiver no longer holds target {}!", receive_order);
			}

			let receive_thought = &mut common.thoughts[receive_order];
			let receive_meta = &mut meta[receive_order];

			match clue.kind {
				ClueKind::RANK => {
					receive_meta.status = CardStatus::CalledToPlay;
					receive_thought.inferred.retain(|i| state.is_playable(i));
					receive_meta.focused = true;

					info!("reactive play+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", state.player_names[*reacter], react_slot, state.player_names[*receiver], target_slot, focus_slot);
				}
				ClueKind::COLOUR => {
					receive_meta.status = CardStatus::CalledToDiscard;
					receive_thought.inferred.retain(|i| state.is_basic_trash(i));
					receive_meta.trash = true;

					info!("reactive play+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", state.player_names[*reacter], react_slot, state.player_names[*receiver], target_slot, focus_slot);
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

		let mut playable_orders = me.thinks_playables(&frame, state.our_player_index);
		let trash_orders = me.thinks_trash(&frame, state.our_player_index);

		// Retain only signalled playables if there is at least 1 such
		if playable_orders.iter().any(|&o| me.order_kp(&frame, o)) {
			playable_orders.retain(|&o| me.order_kp(&frame, o));
		}

		info!("playables {:?}", playable_orders);
		info!("trash {:?}", trash_orders);

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
					Action::play(state.our_player_index, order, *suit_index as i32, *rank as i32)
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
						Action::discard(state.our_player_index, order, *suit_index as i32, *rank as i32, false)
					}
					None => {
						Action::discard(state.our_player_index, order, -1, -1, false)
					}
				})
			}).collect::<Vec<_>>()
		};
		let num_discards = all_discards.len();

		let mut all_actions = all_clues.into_iter().chain(all_plays).chain(all_discards).collect::<Vec<_>>();

		if !cant_discard && num_plays == 0 && num_discards == 0 && !me.thinks_locked(&frame, state.our_player_index) {
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

	fn update_turn(&self, game: &mut Game, action: &TurnAction) {
		let Game { common, state, .. } = game;
		let TurnAction { current_player_index, .. } = action;

		if *current_player_index != -1 {
			let last_player_index = (*current_player_index as usize + state.num_players - 1) % state.num_players;

			common.waiting.retain(|w| w.reacter != last_player_index);
		}
	}
}
