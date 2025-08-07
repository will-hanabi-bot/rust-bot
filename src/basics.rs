use crate::basics::card::{CardStatus, ConvData};
use crate::basics::game::frame::Frame;
use crate::basics::identity_set::IdentitySet;

use self::clue::{BaseClue, CardClue};
use self::card::{Card, Identity, Thought};
use self::game::{Game};
use self::action::{ClueAction, DiscardAction, DrawAction, PlayAction};
use self::variant::{touch_possibilities};
use std::cmp::min;

pub mod action;
pub mod card;
pub mod clue;
pub mod clue_result;
pub mod endgame;
pub mod game;
pub mod player;
pub mod state;
pub mod variant;
pub mod util;
pub mod identity_set;

pub fn on_clue(game: &mut Game, action: &ClueAction) {
	let Game { common, state, meta, deck_ids, .. } = game;
	let &ClueAction { target, clue, ref list, giver } = action;
	let BaseClue { kind, value } = clue;
	let new_possible: IdentitySet = IdentitySet::from_iter(touch_possibilities(&clue, &state.variant));

	for &order in &state.hands[target] {
		let thought = &mut common.thoughts[order];
		let Thought { inferred, possible, .. } = thought;

		if list.contains(&order) {
			let card = &mut state.deck[order];
			card.clued = true;
			card.clues.push(CardClue { kind, value, giver, turn: state.turn_count });

			let new_inferred = inferred.intersect(&new_possible);
			let new_reasoning = new_inferred.len() < inferred.len();

			thought.inferred = new_inferred;
			thought.possible = possible.intersect(&new_possible);

			// Write identity if fully known
			if thought.possible.len() == 1 {
				let id = thought.possible.iter().next().unwrap();
				card.base = Some(id);
				deck_ids[order] = Some(id);
			}

			if new_reasoning {
				meta[order].reasoning.push(state.turn_count);
			}
		}
		else {
			thought.inferred = inferred.difference(&new_possible);
			thought.possible = possible.difference(&new_possible);
		}
	}

	state.endgame_turns = state.endgame_turns.map(|turns| turns - 1);
	state.clue_tokens -= 1;
}

pub fn on_discard(game: &mut Game, action: &DiscardAction) {
	let Game { common, state, deck_ids, .. } = game;
	let &DiscardAction { failed, order, player_index, suit_index, rank } = action;

	if suit_index != -1 && rank != -1 {
		let id = Identity { suit_index: suit_index as usize, rank: rank as usize };

		state.hands[player_index].retain(|&o| o != order);
		state.discard_stacks[id.suit_index][id.rank - 1] += 1;

		// Assign identity
		state.deck[order].base = Some(id);
		deck_ids[order] = Some(id);

		// Discarded all copies of an identity
		if state.discard_stacks[id.suit_index][id.rank - 1] == state.card_count(id) {
			state.max_ranks[id.suit_index] = min(state.max_ranks[id.suit_index], id.rank - 1);
		}

		let thought = &mut common.thoughts[order];
		thought.possible = IdentitySet::single(id);
		thought.inferred = IdentitySet::single(id);
	}

	let Game { state, .. } = game;

	state.endgame_turns = state.endgame_turns.map(|turns| turns - 1);

	if failed {
		state.strikes += 1;
	}
	else {
		state.clue_tokens = min(state.clue_tokens + 1, 8);
	}
}

pub fn on_draw(game: &mut Game, action: &DrawAction) {
	let Game { common, state, meta, players, deck_ids, .. } = game;
	let &DrawAction { order, player_index, suit_index, rank } = action;

	let id = if suit_index != -1 {
		if let Some(Some(deck_id)) = deck_ids.get(order) {
			assert_eq!(*deck_id, Identity { suit_index: suit_index as usize, rank: rank as usize });
		}
		Some(Identity { suit_index: suit_index as usize, rank: rank as usize })
	}
	else {
		None
	};

	if deck_ids.len() == order {
		deck_ids.push(id);
	}
	else if deck_ids.len() > order {
		deck_ids[order] = id;
	}
	else {
		panic!("Only have {} deck ids, but drew card with order {}!", deck_ids.len(), order);
	}

	assert_eq!(state.deck.len(), order);
	assert_eq!(state.deck.len(), state.card_order);

	state.hands[player_index].insert(0, order);
	state.deck.push(Card::new(id, order, state.turn_count));
	state.card_order = order + 1;
	state.cards_left -= 1;

	if state.cards_left == 0 {
		state.endgame_turns = Some(state.num_players);
	}

	for (i, player) in players.iter_mut().enumerate() {
		let id = if i != player_index { id } else { None };
		if player.thoughts.get(order).is_none() {
			player.thoughts.push(Thought::new(order, id, player.all_possible));
		}
	}

	if common.thoughts.get(order).is_none() {
		common.thoughts.push(Thought::new(order, None, common.all_possible));
	}

	if meta.get(order).is_none() {
		meta.push(ConvData::new(order));
	}
}

pub fn on_play(game: &mut Game, action: &PlayAction) {
	let Game { common, state, deck_ids,  .. } = game;
	let &PlayAction { order, player_index, suit_index, rank } = action;

	state.hands[player_index].retain(|&o| o != order);

	if suit_index != -1 && rank != -1 {
		let id = Identity { suit_index: suit_index as usize, rank: rank as usize };

		state.play_stacks[id.suit_index] = id.rank;

		// Assign identity
		state.deck[order].base = Some(id);
		deck_ids[order] = Some(id);

		let thought = &mut common.thoughts[order];
		thought.base = Some(id);
		thought.possible = IdentitySet::single(id);
		thought.inferred = IdentitySet::single(id);
	}

	let Game { state, .. } = game;

	state.endgame_turns = state.endgame_turns.map(|turns| turns - 1);

	if rank == 5 && state.clue_tokens < 8 {
		state.clue_tokens += 1;
	}
}

pub fn elim(game: &mut Game, good_touch: bool) {
	let Game { common, state, players, meta, .. } = game;

	for &order in &state.hands.concat() {
		if common.thoughts[order].inferred.is_empty() && !common.thoughts[order].reset {
			common.thoughts[order].reset_inferences();
			meta[order].status = CardStatus::None;
		}
	}

	let frame = Frame::new(state, meta);

	let mut resets = common.card_elim(state);
	if good_touch {
		resets.extend(common.good_touch_elim(&frame));
	}

	common.refresh_links(&frame, good_touch);
	common.update_hypo_stacks(&frame, &[]);

	for player in players {
		for (i, thought) in player.thoughts.iter_mut().enumerate() {
			let Thought { possible, inferred, info_lock, reset, .. } = &common.thoughts[i];

			thought.possible = *possible;
			thought.inferred = *inferred;
			thought.info_lock = *info_lock;
			thought.reset = *reset;
		}

		player.card_elim(state);
		if good_touch {
			player.good_touch_elim(&frame);
		}

		player.refresh_links(&frame, good_touch);
		player.update_hypo_stacks(&frame, &[]);
	}

	for order in resets {
		if meta[order].status == CardStatus::CalledToPlay {
			meta[order].status = CardStatus::None;
		}
	}
}
