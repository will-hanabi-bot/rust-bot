use crate::basics::card::ConvData;
use crate::basics::game::frame::Frame;

use self::clue::{BaseClue, CardClue};
use self::card::{Card, Identifiable, Identity, Thought};
use self::game::{Game};
use self::action::{ClueAction, DiscardAction, DrawAction, PlayAction};
use self::variant::{card_count, touch_possibilities};
use std::collections::HashSet;
use std::cmp::min;

pub mod action;
pub mod card;
pub mod clue;
pub mod clue_result;
pub mod game;
pub mod player;
pub mod state;
pub mod variant;
pub mod util;

pub fn on_clue(game: &mut Game, action: &ClueAction) {
	let Game { common, state, meta, .. } = game;
	let &ClueAction { target, clue, ref list, giver } = action;
	let BaseClue { kind, value } = clue;
	let new_possible: HashSet<Identity> = HashSet::from_iter(touch_possibilities(&clue, &state.variant));

	for &order in &state.hands[target] {
		let thought = &mut common.thoughts[order];
		let Thought { inferred, possible, .. } = thought;

		if list.contains(&order) {
			let card = &mut state.deck[order];

			if !card.clued {
				card.clued = true;
				card.newly_clued = true;
			}
			card.clues.push(CardClue { kind, value, giver, turn: state.turn_count });

			let new_inferred: HashSet<_> = inferred.intersection(&new_possible).cloned().collect();
			let new_reasoning = new_inferred.len() < inferred.len();

			thought.inferred = new_inferred;
			thought.possible = possible.intersection(&new_possible).cloned().collect();

			if new_reasoning {
				meta[order].reasoning.push(state.turn_count);
			}
		}
		else {
			thought.inferred = inferred.difference(&new_possible).cloned().collect();
			thought.possible = possible.difference(&new_possible).cloned().collect();
		}
	}

	if state.endgame_turns.is_some() {
		state.endgame_turns = state.endgame_turns.map(|turns| turns - 1);
	}

	state.clue_tokens -= 1;
}

pub fn on_discard(game: &mut Game, action: &DiscardAction) {
	let Game { common, state, .. } = game;
	let &DiscardAction { failed, order, player_index, suit_index, rank } = action;

	if suit_index != -1 && rank != -1 {
		let id = Identity { suit_index: suit_index as usize, rank: rank as usize };

		state.hands[player_index].retain(|&o| o != order);
		state.discard_stacks[id.suit_index][id.rank - 1] += 1;

		// Assign identity if not known
		if state.deck[order].id().is_none() {
			state.deck[order].base = Some(id)
		}

		// Discarded all copies of an identity
		if state.discard_stacks[id.suit_index][id.rank - 1] == card_count(&state.variant, &id) {
			state.max_ranks[id.suit_index] = min(state.max_ranks[id.suit_index], id.rank);
		}

		let thought = &mut common.thoughts[order];
		thought.possible = HashSet::from([id]);
		thought.inferred = HashSet::from([id]);
	}

	let Game { state, .. } = game;

	if let Some(endgame_turns) = state.endgame_turns {
		state.endgame_turns = Some(endgame_turns - 1);
	}

	if failed {
		state.strikes += 1;
	}
	else {
		state.clue_tokens = min(state.clue_tokens + 1, 8);
	}
}

pub fn on_draw(game: &mut Game, action: &DrawAction) {
	let Game { common, state, meta, players, .. } = game;
	let &DrawAction { order, player_index, suit_index, rank } = action;
	let id = (suit_index != -1).then_some(Identity { suit_index: suit_index as usize, rank: rank as usize });

	state.hands[player_index].insert(0, order);
	if state.deck.get(order).is_none() {
		state.deck.push(Card::new(id, order, state.turn_count));
	}
	state.card_order = order + 1;
	state.cards_left -= 1;

	if state.cards_left == 0 {
		state.endgame_turns = Some(state.num_players as u8);
	}

	for (i, player) in players.iter_mut().enumerate() {
		let id = if i != player_index { id } else { None };
		if player.thoughts.get(order).is_none() {
			player.thoughts.push(Thought::new(order, id, &player.all_possible));
		}
	}

	if common.thoughts.get(order).is_none() {
		common.thoughts.push(Thought::new(order, None, &common.all_possible));
	}

	if meta.get(order).is_none() {
		meta.push(ConvData::new(order));
	}
}

pub fn on_play(game: &mut Game, action: &PlayAction) {
	let Game { common, state,  .. } = game;
	let &PlayAction { order, player_index, suit_index, rank } = action;

	state.hands[player_index].retain(|&o| o != order);

	if suit_index != -1 && rank != -1 {
		let id = Identity { suit_index: suit_index as usize, rank: rank as usize };

		state.play_stacks[id.suit_index] = id.rank;

		// Assign identity if not known
		if state.deck[order].id().is_none() {
			state.deck[order].base = Some(id)
		}

		let thought = &mut common.thoughts[order];
		thought.base = Some(id);
		thought.possible = HashSet::from([id]);
		thought.inferred = HashSet::from([id]);
	}

	let Game { state, .. } = game;

	if let Some(endgame_turns) = state.endgame_turns {
		state.endgame_turns = Some(endgame_turns - 1);
	}

	if rank == 5 && state.clue_tokens < 8 {
		state.clue_tokens += 1;
	}
}

pub fn elim(game: &mut Game, good_touch: bool) {
	let Game { common, state, players, meta, .. } = game;
	let frame = Frame::new(state, meta);

	if good_touch {
		common.good_touch_elim(&frame);
	} else {
		common.card_elim(state);
	}
	common.refresh_links(&frame, good_touch);
	common.update_hypo_stacks(&frame, &[]);

	for player in players {
		for (i, thought) in player.thoughts.iter_mut().enumerate() {
			let Thought { possible, inferred, .. } = &common.thoughts[i];

			thought.possible.retain(|id| possible.contains(id));
			thought.inferred.retain(|id| inferred.contains(id));
		}

		if good_touch {
			player.good_touch_elim(&frame);
		} else {
			player.card_elim(state);
		}

		player.refresh_links(&frame, good_touch);
		player.update_hypo_stacks(&frame, &[]);
	}
}
