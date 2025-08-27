use log::{info, warn};

use crate::basics::action::Action;
use crate::basics::card::{CardStatus, ConvData};
use crate::basics::clue::ClueKind;
use crate::basics::game::Game;
use crate::basics::player::{Player, WaitingConnection};
use crate::basics::state::State;
use crate::reactor::{ClueInterp, Reactor};

impl Reactor {
	pub(super) fn calc_slot(focus_slot: usize, slot: usize) -> usize {
		let other = (focus_slot + 5 - slot) % 5;
		if other == 0 { 5 } else { other }
	}

	fn calc_target_slot(prev: &Game, game: &Game, order: usize, wc: &WaitingConnection) -> Option<(usize, usize)> {
		let Game { state, .. } = game;
		let &WaitingConnection { reacter, receiver, ref receiver_hand,  focus_slot, .. } = wc;

		let react_slot = prev.state.hands[reacter].iter().position(|&o| o == order).unwrap() + 1;
		let target_slot = Reactor::calc_slot(focus_slot, react_slot);

		if receiver_hand.get(target_slot - 1).is_none() {
			warn!("Receiver no longer has slot {target_slot}!");
			return None;
		}

		let receive_order = receiver_hand[target_slot - 1];
		if !state.hands[receiver].contains(&receive_order) {
			warn!("Receiver no longer holds target {receive_order}!");
			return None;
		}

		Some((react_slot, target_slot))
	}

	pub(super) fn elim_dc_dc(state: &State, common: &mut Player, meta: &mut [ConvData], reacter: usize, receiver_hand: &[usize], focus_slot: usize, target_slot: usize) {
		// Entire hand is unplayable
		Reactor::elim_play_play(state, common, meta, reacter, receiver_hand, focus_slot, receiver_hand.len() + 1);

		for (i, receive_order) in receiver_hand.iter().enumerate().take(target_slot - 1) {
			if meta[*receive_order].status == CardStatus::CalledToPlay || meta[*receive_order].status == CardStatus::CalledToDiscard {
				continue;
			}

			let react_slot = Reactor::calc_slot(focus_slot, i + 1);
			if let Some(react_order) = state.hands[reacter].get(react_slot - 1) {
				let react_thought = &common.thoughts[*react_order];

				if react_thought.possible.iter().all(|i| state.is_critical(i)) {
					continue;
				}
				else {
					common.thoughts[*receive_order].inferred.retain(|i| !state.is_basic_trash(i));
					info!("eliminated trash from slot {} {} - {}", i + 1, *receive_order, common.str_infs(state, *receive_order));
				}
			}
		}
	}

	pub(super) fn elim_play_dc(state: &State, common: &mut Player, meta: &mut [ConvData], reacter: usize, receiver_hand: &[usize], focus_slot: usize, target_slot: usize) {
		// Entire hand is unplayable
		Reactor::elim_play_play(state, common, meta, reacter, receiver_hand, focus_slot, receiver_hand.len() + 1);

		for (i, receive_order) in receiver_hand.iter().enumerate().take(target_slot - 1) {
			if meta[*receive_order].status == CardStatus::CalledToPlay || meta[*receive_order].status == CardStatus::CalledToDiscard {
				continue;
			}

			let react_slot = Reactor::calc_slot(focus_slot, i + 1);
			if let Some(react_order) = state.hands[reacter].get(react_slot - 1) {
				let react_thought = &common.thoughts[*react_order];
				let playable_reacts = react_thought.possible.iter().filter(|&i| state.is_playable(i)).collect::<Vec<_>>();

				if playable_reacts.is_empty() {
					continue;
				}
				else {
					common.thoughts[*receive_order].inferred.retain(|i| !state.is_basic_trash(i));
					info!("eliminated trash from slot {} {} - {}", i + 1, *receive_order, common.str_infs(state, *receive_order));
				}
			}
		}
	}

	pub(super) fn elim_dc_play(state: &State, common: &mut Player, meta: &mut [ConvData], reacter: usize, receiver_hand: &[usize], focus_slot: usize, target_slot: usize) {
		for (i, receive_order) in receiver_hand.iter().enumerate().take(target_slot - 1) {
			if meta[*receive_order].status == CardStatus::CalledToPlay || meta[*receive_order].status == CardStatus::CalledToDiscard {
				continue;
			}

			let react_slot = Reactor::calc_slot(focus_slot, i + 1);
			if let Some(react_order) = state.hands[reacter].get(react_slot - 1) {
				let react_thought = &common.thoughts[*react_order];

				if react_thought.possible.iter().all(|i| state.is_critical(i)) {
					continue;
				}
				else {
					common.thoughts[*receive_order].inferred.retain(|i| !state.is_playable(i));
					info!("eliminated playables from slot {} {} - {}", i + 1, *receive_order, common.str_infs(state, *receive_order));

					if common.thoughts[*receive_order].inferred.is_empty() {
						meta[*receive_order].trash = true;
					}
				}
			}
		}
	}

	pub(super) fn elim_play_play(state: &State, common: &mut Player, meta: &mut [ConvData], reacter: usize, receiver_hand: &[usize], focus_slot: usize, target_slot: usize) {
		for (i, receive_order) in receiver_hand.iter().enumerate().take(target_slot - 1) {
			if meta[*receive_order].status == CardStatus::CalledToPlay || meta[*receive_order].status == CardStatus::CalledToDiscard {
				continue;
			}

			let react_slot = Reactor::calc_slot(focus_slot, i + 1);
			if let Some(react_order) = state.hands[reacter].get(react_slot - 1) {
				let react_thought = &common.thoughts[*react_order];
				let playable_reacts = react_thought.possible.iter().filter(|&i| state.is_playable(i)).collect::<Vec<_>>();

				if playable_reacts.is_empty() {
					continue;
				}
				else if playable_reacts.len() == 1 {
					common.thoughts[*receive_order].inferred.retain(|i| !state.is_playable(i) || playable_reacts.contains(&i));
					info!("eliminated playables except {} from slot {} {} - {}", state.log_id(*playable_reacts.first().unwrap()), i + 1, *receive_order, common.str_infs(state, *receive_order));
				}
				else {
					common.thoughts[*receive_order].inferred.retain(|i| !state.is_playable(i));
					info!("eliminated playables from slot {} {} - {}", i + 1, *receive_order, common.str_infs(state, *receive_order));
				}

				if common.thoughts[*receive_order].inferred.is_empty() {
					meta[*receive_order].trash = true;
				}
			}
		}
	}

	fn target_idiscard(prev: &Game, game: &mut Game, wc: &WaitingConnection, target_slot: usize) {
		let Game { common, state, meta, .. } = game;
		let order = wc.receiver_hand[target_slot - 1];
		let meta = &mut meta[order];

		common.thoughts[order].old_inferred = Some(common.thoughts[order].inferred);
		common.thoughts[order].inferred.retain(|i| !prev.state.is_critical(i));
		meta.status = CardStatus::CalledToDiscard;
		meta.by = Some(wc.giver);

		if common.thoughts[order].inferred.is_empty() {
			meta.trash = true;
		}

		if meta.reasoning.last().is_none_or(|r| *r != state.turn_count) {
			meta.reasoning.push(state.turn_count);
		}
	}

	fn target_iplay(_prev: &Game, game: &mut Game, wc: &WaitingConnection, target_slot: usize) {
		let Game { common, state, meta, .. } = game;
		let order = wc.receiver_hand[target_slot - 1];

		if meta[order].status == CardStatus::ZeroClueChop {
			if let Some(new_zcs) = state.hands[wc.receiver].iter().find(|&&o| o < order && !state.deck[o].clued && meta[o].status == CardStatus::None) {
				info!("shifting zcs forward to {new_zcs}!");
				meta[*new_zcs].status = CardStatus::ZeroClueChop;
			}
			else {
				warn!("unable to shift zcs forward!");
			}
		}

		let meta = &mut game.meta[order];

		common.thoughts[order].old_inferred = Some(common.thoughts[order].inferred);
		common.thoughts[order].inferred.retain(|i| state.is_playable(i));
		meta.status = CardStatus::CalledToPlay;
		meta.by = Some(wc.giver);
		meta.focused = true;

		if meta.reasoning.last().is_none_or(|r| *r != state.turn_count) {
			meta.reasoning.push(state.turn_count);
		}
	}

	pub fn react_discard(prev: &Game, game: &mut Game, player_index: usize, order: usize, wc: &WaitingConnection) {
		let &WaitingConnection { reacter, receiver, clue, ref receiver_hand, focus_slot, inverted, turn, .. } = wc;

		if player_index != reacter {
			warn!("Had unrelated waiting connection! {:?}", game.common.waiting);
			return;
		}

		let known_trash = prev.common.thinks_trash(&prev.frame(), reacter);

		// We were waiting for a response inversion and they reacted unnaturally
		if inverted {
			if if known_trash.is_empty() { prev.state.hands[reacter][0] != order } else { !known_trash.contains(&order) } {
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
				return;
			}
		}

		if let Some((react_slot, target_slot)) = Reactor::calc_target_slot(prev, game, order, wc) {
			match clue.kind {
				ClueKind::COLOUR => {
					Reactor::target_iplay(prev, game, wc, target_slot);
					Reactor::elim_dc_play(&prev.state, &mut game.common, &mut game.meta, reacter, receiver_hand, focus_slot, target_slot);
				},
				ClueKind::RANK => {
					Reactor::target_idiscard(prev, game, wc, target_slot);
					Reactor::elim_dc_dc(&prev.state, &mut game.common, &mut game.meta, reacter, receiver_hand, focus_slot, target_slot);
				}
			}

			info!("reactive dc+{}, reacter {} (slot {}) receiver {} (slot {}), focus slot {} (order {})",
				if clue.kind == ClueKind::COLOUR { "play" } else { "dc" },
				game.state.player_names[reacter], react_slot, game.state.player_names[receiver], target_slot, focus_slot, game.state.hands[receiver][target_slot - 1]);
		}

	}

	pub fn react_play(prev: &Game, game: &mut Game, player_index: usize, order: usize, wc: &WaitingConnection) {
		let &WaitingConnection { reacter, receiver, clue, ref receiver_hand, focus_slot, inverted, turn, .. } = wc;

		if player_index != reacter {
			warn!("Had unrelated waiting connection! {:?}", game.common.waiting);
			return;
		}

		let known_playables = prev.common.obvious_playables(&prev.frame(), reacter);

		// We were waiting for a response inversion and they reacted unnaturally
		if inverted {
			if !known_playables.contains(&order) {
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
				return;
			}
		}

		if let Some((react_slot, target_slot)) = Reactor::calc_target_slot(prev, game, order, wc) {
			match clue.kind {
				ClueKind::RANK => {
					Reactor::target_iplay(prev, game, wc, target_slot);
					Reactor::elim_play_play(&prev.state, &mut game.common, &mut game.meta, reacter, receiver_hand, focus_slot, target_slot);
				},
				ClueKind::COLOUR => {
					Reactor::target_idiscard(prev, game, wc, target_slot);
					Reactor::elim_play_dc(&prev.state, &mut game.common, &mut game.meta, reacter, receiver_hand, focus_slot, target_slot);
				}
			}

			info!("reactive play+{}, reacter {} (slot {}) receiver {} (slot {}), focus slot {} (order {})",
				if clue.kind == ClueKind::COLOUR { "dc" } else { "play" },
				game.state.player_names[reacter], react_slot, game.state.player_names[receiver], target_slot, focus_slot, game.state.hands[receiver][target_slot - 1]);
		}
	}
}
