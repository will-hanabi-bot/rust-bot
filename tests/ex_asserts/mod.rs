use rust_bot::basics::game::Game;

use crate::util::Player;

pub fn has_inferences(game: &Game, player_index: Option<Player>, target: Player, slot: usize, inferences: &[&str]) {
	let Game { common, state, players, .. } = game;
	let order = state.hands[target as usize].get(slot - 1).unwrap_or_else(|| panic!("Slot {slot} doesn't exist"));
	let player = player_index.map(|i| &players[i as usize]).unwrap_or(common);
	let thought = &player.thoughts[*order];

	assert!(thought.inferred.len() == inferences.len() && inferences.iter().all(|&i| thought.inferred.contains(state.expand_short(i))),
		"Differing inferences. Expected {}, got {}", inferences.join(","), player.str_infs(state, *order));
}

pub fn has_possible(game: &Game, player_index: Option<Player>, target: Player, slot: usize, possible: &[&str]) {
	let Game { common, state, players, .. } = game;
	let order = state.hands[target as usize].get(slot - 1).unwrap_or_else(|| panic!("Slot {slot} doesn't exist"));
	let player = player_index.map(|i| &players[i as usize]).unwrap_or(common);
	let thought = &player.thoughts[*order];

	assert!(thought.possible.len() == possible.len() && possible.iter().all(|&i| thought.possible.contains(state.expand_short(i))),
		"Differing possibilities. Expected {}, got {}", possible.join(","), player.str_poss(state, *order));
}
