use log::info;
use std::collections::HashSet;

use crate::basics::action::{Action, ClueAction};
use crate::basics::card::{CardStatus, IdOptions, Identifiable, Identity, MatchOptions};
use crate::basics::game::{frame::Frame, Game};
use crate::basics::util::visible_find;

pub fn check_fix(prev: &Game, game: &mut Game, action: &ClueAction) -> (Vec<usize>, Vec<usize>) {
	let ClueAction { clue, giver, list, target } = action;
	let Game { common: old_common, .. } = prev;
	let Game { common, state, .. } = game;

	let mut clue_resets: HashSet<usize> = HashSet::new();

	for order in &state.hands[*target] {
		let old_thought = &old_common.thoughts[*order];
		let thought = &common.thoughts[*order];

		// TODO: pink fix
		let clued_reset = !old_thought.inferred.is_empty() && thought.inferred.is_empty();

		if clued_reset {
			clue_resets.insert(*order);
			common.thoughts[*order].reset_inferences();
			game.meta[*order].status = CardStatus::None;
		}
	}

	// refresh links
	// cancel waiting connections, undo elims

	let clued_resets = list.iter().filter(|&order| clue_resets.contains(order)).cloned().collect::<Vec<_>>();

	if !clued_resets.is_empty() {
		info!("clued cards {:?} were newly reset!", clued_resets);
	}

	let duplicate_reveals = list.iter().filter(|&order| {
		let old_thought = &old_common.thoughts[*order];
		let thought = &common.thoughts[*order];
		if thought.possible.len() == old_thought.possible.len() {
			return false;
		}

		// No-info clue
		if state.deck[*order].clues.iter().filter(|&cl| cl == clue).count() > 1 {
			return false;
		}

		if let Some(id) = thought.identity(&Default::default()) {
			let frame = Frame::new(state, &game.meta);
			let copy = visible_find(state, common, id, MatchOptions { infer: true, ..Default::default() }, |player_index, o|
				player_index != *giver && frame.is_touched(o) && o != *order);

			if !copy.is_empty() {
				info!("duplicate {} revealed! copy of order {}", state.log_id(id), copy[0]);
				return true;
			}
		}
		false
	}).cloned().collect::<Vec<_>>();

	(clued_resets, duplicate_reveals)
}

pub fn connectable_simple(game: &Game, start: usize, target: usize, id: Option<&Identity>) -> bool {
	let Game { state, players, .. } = game;

	if let Some(id) = id {
		if state.is_playable(id) {
			return true;
		}
	}

	if start == target {
		return !players[target].thinks_playables(&game.frame(), target).is_empty();
	}

	let next_player_index = state.next_player_index(start);
	let playables = players[start].thinks_playables(&game.frame(), start);

	for order in playables {
		let play_id = players[start].thoughts[order].identity(&IdOptions { infer: true, ..Default::default() });

		// Simulate playing the card
		if let Some(play_id) = play_id {
			let new_game = game.clone();
			new_game.simulate_action(&Action::play(start, order, play_id.suit_index as i32, play_id.rank as i32));
			new_game.simulate_action(&Action::turn(state.turn_count, next_player_index as i32));

			if connectable_simple(&new_game, next_player_index, target, id) {
				return true;
			}
		}
	}
	connectable_simple(game, next_player_index, target, id)
}
