
use crate::basics::card::MatchOptions;

use super::game::Game;
use super::card::{CardStatus, Identifiable};

pub struct ElimResult {
	pub new_touched: Vec<usize>,
	pub fill: Vec<usize>,
	pub elim: Vec<usize>
}

pub fn elim_result(prev: &Game, game: &Game, hand: &[usize], list: &[usize]) -> ElimResult {
	let mut new_touched = Vec::new();
	let mut fill = Vec::new();
	let mut elim = Vec::new();

	for &order in hand {
		let prev_thought = &prev.common.thoughts[order];
		let thought = &game.common.thoughts[order];
		let card = &game.state.deck[order];

		if card.clued && game.meta[order].status != CardStatus::CalledToDiscard && thought.possible.len() < prev_thought.possible.len() {
			if card.newly_clued && !prev.frame().is_blind_playing(order) && !game.common.order_kt(&game.frame(), order) {
				new_touched.push(order);
			}
			else if list.contains(&order) && game.state.has_consistent_inferences(thought) && game.meta[order].status != CardStatus::CalledToPlay {
				fill.push(order);
			}
			else if game.state.has_consistent_inferences(thought) {
				elim.push(order);
			}
		}
	}

	ElimResult { new_touched, fill, elim }
}

pub struct BadTouchResult {
	pub bad_touch: Vec<usize>,
	pub trash: Vec<usize>,
	pub avoidable_dupe: usize,
}

pub fn bad_touch_result(prev: &Game, game: &Game, giver: usize, target: usize) -> BadTouchResult {
	let Game { state, .. } = game;
	let player = &game.common;

	let dupe_scores = prev.players.iter().enumerate().map(|(i, player)| {
		if i == target {
			return 99;
		}

		let mut score = 0;

		for &order in &state.hands[target] {
			let card = &state.deck[order];

			// Not newly clued, trash id or we don't know: don't care about duplicating
			if !card.newly_clued || card.id().map(|id| state.is_basic_trash(id)).unwrap_or(true) {
				continue;
			}

			score += state.hands[i].iter().filter(|&&o| {
				let thought = &player.thoughts[o];
				state.deck[o].clued && thought.inferred.len() > 1 && thought.inferred.contains(card.id().unwrap())
			}).count();
		}
		score
	}).collect::<Vec<_>>();

	let min_dupe = dupe_scores.iter().min().unwrap();
	let avoidable_dupe = dupe_scores[giver] - min_dupe;

	let mut bad_touch = Vec::new();
	let mut trash = Vec::new();

	for &order in &state.hands[target] {
		let card = &state.deck[order];

		if !card.newly_clued {
			continue;
		}

		if player.order_kt(&game.frame(), order) {
			trash.push(order);
			continue;
		}

		if let Some(id) = card.id() {
			if state.is_basic_trash(id) {
				bad_touch.push(order);
			}
		}
	}

	// Previously-finessed cards can be reset (and no longer touched) after the clue, so double-check for "duplicates".
	for &order in &state.hands[target] {
		let card = &state.deck[order];

		if !card.newly_clued || bad_touch.contains(&order) || trash.contains(&order) {
			continue;
		}

		for (i, hand) in state.hands.iter().enumerate() {
			for &o in hand {
				let duplicated = (prev.frame().is_touched(o) || game.frame().is_touched(o)) &&
					game.me().thoughts[o].matches(card, &MatchOptions { infer: true, ..Default::default() }) &&
					(i != target || o < order);

				if duplicated {
					bad_touch.push(order);
				}
			}
		}
	}

	BadTouchResult { bad_touch, trash, avoidable_dupe }
}

pub struct PlayablesResult {
	pub blind_plays: Vec<usize>,
	pub playables: Vec<usize>,
}

pub fn playables_result(prev: &Game, game: &Game) -> PlayablesResult {
	let mut blind_plays = Vec::new();
	let mut playables = Vec::new();

	for &order in &game.me().hypo_plays {
		if prev.me().hypo_plays.contains(&order) {
			continue;
		}

		if game.frame().is_blind_playing(order) && !prev.frame().is_blind_playing(order) {
			blind_plays.push(order);
		}

		playables.push(order);
	}

	PlayablesResult { blind_plays, playables }
}

pub struct CMResult {
	pub cm: Vec<usize>,
}

pub fn cm_result(prev: &Game, game: &Game) -> CMResult {
	let Game { common, .. } = game;

	let mut cm = Vec::new();

	for &order in &common.hypo_plays {
		if game.meta[order].cm() && !prev.meta[order].cm() {
			cm.push(order);
		}
	}

	CMResult { cm }
}
