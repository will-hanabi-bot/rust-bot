use super::action::{Action, PerformAction};
use crate::basics::action::GameOverAction;
use crate::basics::clue::{BaseClue, Clue, ClueKind};
use crate::basics::variant::card_touched;

use super::state::State;
use super::player::Player;
use super::card::{Identifiable, Identity, MatchOptions};

pub fn visible_find<F>(state: &State, player: &Player, id: &Identity, options: MatchOptions, cond: F) -> Vec<usize> where F: Fn(usize, usize) -> bool {
	let mut orders = Vec::new();

	for (player_index, hand) in state.hands.iter().enumerate() {
		for &order in hand {
			let thought = &player.thoughts[order];
			let mut per_options = options.clone();

			if player_index == player.player_index {
				per_options.symmetric = true;
			}

			if thought.matches(id, &per_options) && cond(player_index, order) {
				orders.push(order);
			}
		}
	}
	orders
}

pub fn clue_to_perform(clue: &Clue, table_id: u32) -> PerformAction {
	let Clue { kind, value, target } = clue;
	match kind {
		ClueKind::COLOUR => PerformAction::Colour { table_id: Some(table_id), value: *value, target: *target },
		ClueKind::RANK => PerformAction::Rank { table_id: Some(table_id), value: *value, target: *target },
	}
}

pub fn perform_to_action(state: &State, action: &PerformAction, player_index: usize, deck: Option<&[Identity]>) -> Action {
	let clue_touched = |orders: &[usize], clue: &BaseClue|
		orders.iter().filter_map(|&order| {
			match state.deck[order].id().or_else(|| deck.and_then(|d| d[order].id())) {
				Some(id) => card_touched(id, &state.variant, clue).then_some(order),
				None => None
			}
		}).collect::<Vec<_>>();

	match action {
		PerformAction::Play { target, .. } => {
			match state.deck[*target].id().or_else(|| deck.and_then(|d| d[*target].id())) {
				Some(id) =>
					if state.is_playable(id) {
						Action::play(player_index, *target, id.suit_index as i32, id.rank as i32)
					}
					else {
						Action::discard(player_index, *target, id.suit_index as i32, id.rank as i32, true)
					}
				None => Action::discard(player_index, *target, -1, -1, true)
			}
		},
		PerformAction::Discard { target, .. } => {
			match state.deck[*target].id().or_else(|| deck.and_then(|d| d[*target].id())) {
				Some(id) => Action::discard(player_index, *target, id.suit_index as i32, id.rank as i32, false),
				None => Action::discard(player_index, *target, -1, -1, false)
			}
		},
		PerformAction::Colour { target, value, .. } => {
			let clue = BaseClue { kind: ClueKind::COLOUR, value: *value };
			let list = clue_touched(&state.hands[*target], &clue);
			Action::clue(player_index, *target, clue, list)
		},
		PerformAction::Rank { target, value, .. } => {
			let clue = BaseClue { kind: ClueKind::RANK, value: *value };
			let list = clue_touched(&state.hands[*target], &clue);
			Action::clue(player_index, *target, clue, list)
		},
		PerformAction::Terminate { target, value, .. } => {
			Action::GameOver(GameOverAction { end_condition: *value, player_index: *target })
		},
	}
}

/** Returns all player indices between the start (exclusive) and end (inclusive) in play order. */
pub fn players_between(num_players: usize, start: usize, end: usize) -> Vec<usize> {
	let gap = (end + num_players - start) % num_players;

	if gap == 0 {
		Vec::new()
	}
	else {
		(1..gap).map(|inc| (start + inc) % num_players).collect()
	}
}
