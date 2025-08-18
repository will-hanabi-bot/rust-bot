use crate::basics::action::{Action, ClueAction};
use crate::basics::card::{IdOptions, Identifiable, Identity, MatchOptions};
use crate::basics::game::{Game};

pub fn check_fix(prev: &Game, game: &Game, action: &ClueAction) -> (Vec<usize>, Vec<usize>) {
	let ClueAction { list, .. } = action;
	let Game { common, state, .. } = game;

	let mut clued_resets = Vec::new();
	let mut duplicate_reveals = Vec::new();

	for &order in list {
		let duplicated = prev.state.deck[order].clued && list.iter().any(|&o|
			o != order &&
			prev.state.deck[o].clued &&
			state.deck[order].is(&state.deck[o]) &&
			!prev.common.thoughts[order].matches(&prev.common.thoughts[o], &MatchOptions { infer: true, ..Default::default() })
		);

		if !prev.common.thoughts[order].reset && common.thoughts[order].reset {
			clued_resets.push(order);
		}
		else if duplicated {
			duplicate_reveals.push(order);
		}
	}

	(clued_resets, duplicate_reveals)
}

pub fn connectable_simple(game: &Game, start: usize, target: usize, id: Option<Identity>) -> Vec<usize> {
	let Game { state, players, .. } = game;

	if let Some(id) = id {
		if state.is_playable(id) {
			return vec![99];
		}
	}

	if start == target {
		return players[target].obvious_playables(&game.frame(), target);
	}

	let next_player_index = state.next_player_index(start);
	let playables = players[start].obvious_playables(&game.frame(), start);

	for order in playables {
		let play_id = players[start].thoughts[order].identity(&IdOptions { infer: true, ..Default::default() });

		// Simulate playing the card
		if let Some(play_id) = play_id {
			let mut new_game = game.simulate_action(&Action::play(start, order, play_id.suit_index as i32, play_id.rank as i32), None);
			new_game = new_game.simulate_action(&Action::turn(state.turn_count, next_player_index as i32), None);

			let play_connectables = connectable_simple(&new_game, next_player_index, target, id);

			if !play_connectables.is_empty() {
				return play_connectables;
			}
		}
	}
	connectable_simple(game, next_player_index, target, id)
}
