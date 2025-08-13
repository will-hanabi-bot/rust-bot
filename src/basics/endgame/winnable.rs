use std::time::Instant;

#[allow(unused_imports)]
use colored::Colorize;
use fraction::ConstOne;
use itertools::Itertools;

#[allow(unused_imports)]
use log::info;

use crate::basics::action::PerformAction;
use crate::basics::card::{Card, Identifiable, Identity};
use crate::basics::game::Game;
use crate::basics::util::players_upto;
use crate::basics::{state::State};
use super::{EndgameSolver, WinnableResult, UNWINNABLE, RemainingMap, remove_remaining};

type Frac = fraction::Fraction;

#[derive(Debug, Clone)]
pub enum SimpleResult {
	AlwaysWinnable,
	WinnableWithDraws(Vec<Identity>),
	Unwinnable
}

impl EndgameSolver {
	fn find_must_plays(state: &State, hand: &[usize]) -> Vec<Identity> {
		let id_groups = hand.iter().into_group_map_by(|&&o| state.deck[o].base);

		id_groups.iter().filter_map(|(id, orders)| {
			match id {
				None => None,
				Some(id) => {
					if state.is_basic_trash(*id) {
						return None;
					}

					(state.card_count(*id) - state.base_count(*id) == orders.len()).then_some(id)
				}
			}
		}).copied().collect()
	}

	pub(super) fn unwinnable_state(state: &State, player_turn: usize) -> bool {
		if state.ended() || state.pace() < 0 {
			return true;
		}

		let void_players = (0..state.num_players).filter(|&i|
			state.hands[i].iter().all(|o| {
				let card = &state.deck[*o];
				match card.id() {
					None => true,
					Some(id) => state.is_basic_trash(id)
				}
			})
		).collect::<Vec<_>>();

		// println!("void players: {:?}, endgame_turns: {:?}, current turn: {}", void_players, state.endgame_turns, state.player_names[player_turn]);

		let must_plays = state.hands.iter().map(|hand| EndgameSolver::find_must_plays(state, hand)).collect::<Vec<_>>();
		let must_start_endgame = must_plays.iter().positions(|plays| plays.len() > 1).collect::<Vec<_>>();

		if let Some(endgame_turns) = state.endgame_turns {
			let possible_players = (0..endgame_turns).filter(|&i|
				!void_players.contains(&((player_turn + i) % state.num_players))
			).count();

			if possible_players + state.score() < state.max_score() {
				// println!("even if everyone ({}) plays, can't reach max ({}/{})", possible_players, state.score(), state.max_score());
				return true;
			}

			for i in 0..endgame_turns {
				let player_index = (player_turn + i) % state.num_players;

				if must_plays[player_index].len() > 1 {
					// println!("final round has started, {} still needs to play {:?}", state.player_names[player_index], must_plays[player_index]);
					return true;
				}
			}
		}

		if state.cards_left == 1 {
			// At least 2 people need to play 2 cards
			if must_start_endgame.len() > 1 {
				// println!("{:?} need to start endgame, only 1 card left", must_start_endgame.iter().map(|i| state.player_names[*i].to_owned()).collect::<Vec<_>>());
				return true;
			}

			if must_start_endgame.len() == 1 {
				let target = must_start_endgame[0];

				if player_turn != target && players_upto(state.num_players, player_turn, target).len() > state.clue_tokens {
					// println!("{} needs to start endgame, not enough clues to reach their turn", state.player_names[target]);
					return true;
				}
			}
		}
		else if void_players.len() as i32 > state.pace() {
			// println!("too many void players: {}, pace {}", void_players.len(), state.pace());
			return true;
		}
		false
	}

	/** Returns whether an endgame in the final round is winnable just by everyone playing what they know. */
	pub(super) fn trivially_winnable(game: &Game, player_turn: usize) -> WinnableResult {
		let Game { state, .. } = game;

		// if state.score() == state.max_score() {
		// 	return (true, None);
		// }

		match state.endgame_turns {
			None => UNWINNABLE,
			Some(endgame_turns) => {
				if state.rem_score() > endgame_turns {
					return UNWINNABLE;
				}

				let mut play_stacks = state.play_stacks.clone();
				let mut action: PerformAction = PerformAction::Discard { target: state.hands[player_turn][0] };

				for i in 0..endgame_turns {
					let player_index = (player_turn + i) % state.num_players;
					let playables = game.players[player_index].thinks_playables(&game.frame(), player_index);

					if playables.is_empty() {
						continue;
					}

					match state.deck[playables[0]].id() {
						None => continue,
						Some(id) => {
							if i == 0 {
								action = PerformAction::Play { target: playables[0] };
							}
							play_stacks[id.suit_index] = id.rank;
						}
					}
				}

				if play_stacks.iter().sum::<usize>() == state.max_score() {
					Ok((vec![action], Frac::ONE))
				} else {
					Err("")
				}
			}
		}
	}

	pub(super) fn clueless_winnable(&mut self, state: &State, player_turn: usize, deadline: &Instant) -> Option<PerformAction> {
		if state.score() == state.max_score() {
			return Some(PerformAction::Play { target: 99 });
		}

		let hash = state.hash();
		if self.clueless_cache.contains_key(&hash) {
			return self.clueless_cache[&hash];
		}

		if Instant::now() > *deadline {
			return None;
		}

		if EndgameSolver::unwinnable_state(state, player_turn) {
			self.clueless_cache.insert(hash, None);
			return None;
		}

		let mut discardable = None;

		for &order in &state.hands[player_turn] {
			let card = &state.deck[order];
			if let Some(id) = card.id() {
				if state.is_playable(id) {
					let action = PerformAction::Play { target: order };
					let new_state = EndgameSolver::advance_state(state, &action, player_turn, None);

					if self.clueless_winnable(&new_state, state.next_player_index(player_turn), deadline).is_some() {
						return Some(action);
					}
				}
			}
			else if discardable.is_none() {
				discardable = Some(order);
			}
		}

		if state.clue_tokens > 0 {
			let action = PerformAction::Rank { target: 0, value: 0 };
			let new_state = EndgameSolver::advance_state(state, &action, player_turn, None);

			if self.clueless_winnable(&new_state, state.next_player_index(player_turn), deadline).is_some() {
				return Some(action);
			}
		}

		if let Some(order) = discardable {
			let action = PerformAction::Discard { target: order };
			let new_state = EndgameSolver::advance_state(state, &action, player_turn, None);

			if self.clueless_winnable(&new_state, state.next_player_index(player_turn), deadline).is_some() {
				return Some(action);
			}
		}
		None
	}

	pub(super) fn winnable_simpler(&mut self, state: &State, player_turn: usize, remaining: &RemainingMap, deadline: &Instant) -> bool {
		if state.score() == state.max_score() {
			return true;
		}

		let hash = state.hash();
		if self.simpler_cache.contains_key(&hash) {
			return self.simpler_cache[&hash];
		}

		if EndgameSolver::unwinnable_state(state, player_turn) {
			// info!("{}", "unwinnable state".yellow());
			self.simpler_cache.insert(hash, false);
			return false;
		}

		let mut possible_actions = Vec::new();
		let mut discardable = false;

		for &order in &state.hands[player_turn] {
			let card = &state.deck[order];
			if let Some(id) = card.id() {
				if state.is_playable(id) {
					possible_actions.push(PerformAction::Play { target: order });
				}
				else if state.is_basic_trash(id) && !discardable {
					possible_actions.push(PerformAction::Discard { target: order });
					discardable = true;
				}
			}
			else if !discardable {
				possible_actions.push(PerformAction::Discard { target: order });
			}
		}

		if state.clue_tokens > 0 {
			possible_actions.push(PerformAction::Rank { target: 0, value: 0 });
		}

		possible_actions.sort_by_key(|perform| {
			match perform {
				PerformAction::Play { .. } => 0,
				PerformAction::Colour { .. } | PerformAction::Rank { .. } => 1,
				PerformAction::Discard { .. } => 2,
				_ => -1
			}
		});

		// info!("possible winnable simpler actions for {}: {:?}", state.player_names[player_turn], possible_actions);

		let winnable = possible_actions.iter().any(|action| match self.winnable_if(state, player_turn, action, remaining, deadline) {
			SimpleResult::AlwaysWinnable => true,
			SimpleResult::WinnableWithDraws(_) => true,
			SimpleResult::Unwinnable => false
		});
		self.simpler_cache.insert(hash, winnable);
		winnable
	}

	pub(super) fn winnable_if(&mut self, state: &State, player_turn: usize, action: &PerformAction, remaining: &RemainingMap, deadline: &Instant) -> SimpleResult {
		let hash = format!("{},{},{:?},{:?}", state.hash(), player_turn, action, remaining.iter().sorted_by_key(|(id, _)| id.suit_index * 10 + id.rank));

		if self.if_cache.contains_key(&hash) {
			return self.if_cache[&hash].clone();
		}

		if Instant::now() > *deadline {
			return SimpleResult::Unwinnable;
		}

		// info!("{}", format!("checking if {} is winning {} {}", action.fmt_s(state, player_turn), state.turn_count, self.simpler_cache.len()).green());
		if state.cards_left == 0 || action.is_clue() {
			let new_state = EndgameSolver::advance_state(state, action, player_turn, None);
			let winnable = self.winnable_simpler(&new_state, state.next_player_index(player_turn), remaining, deadline);

			let res = if winnable { SimpleResult::AlwaysWinnable } else { SimpleResult:: Unwinnable };
			self.if_cache.insert(hash, res.clone());
			return res;
		}

		let mut winnable_draws = Vec::new();

		for id in remaining.keys() {
			let draw = Card::new(Some(*id), state.card_order + 1, state.turn_count);
			let new_state = EndgameSolver::advance_state(state, action, player_turn, Some(draw));
			let new_remaining = remove_remaining(remaining, *id);

			let winnable = self.winnable_simpler(&new_state, state.next_player_index(player_turn), &new_remaining, deadline);
			if winnable {
				winnable_draws.push(*id);
			}
		}

		let res = if winnable_draws.is_empty() {
			SimpleResult::Unwinnable
		} else {
			SimpleResult::WinnableWithDraws(winnable_draws)
		};
		self.if_cache.insert(hash, res.clone());
		res
	}

	fn advance_state(state: &State, action: &PerformAction, player_index: usize, draw: Option<Card>) -> State {
		let mut new_state = state.clone();
		new_state.turn_count += 1;

		let remove_and_draw_new = |player_index: usize, order: usize| {
			let new_card_order = state.card_order;
			new_state.hands[player_index].retain(|&o| o != order);
			new_state.hands[player_index].insert(0, state.card_order);

			match state.endgame_turns {
				Some(endgame_turns) => new_state.endgame_turns = Some(endgame_turns - 1),
				None => {
					new_state.card_order += 1;
					new_state.cards_left -= 1;

					if new_state.cards_left == 0 {
						new_state.endgame_turns = Some(state.num_players);
					}
				}
			}

			if state.deck.get(new_card_order).and_then(|card| card.base).is_none() {
				let new_card = draw.unwrap_or_else(|| Card::new(None, new_card_order, state.turn_count));
				match new_state.deck.get_mut(new_card_order) {
					Some(card) => *card = new_card,
					None => new_state.deck.push(new_card)
				}
			}
		};

		match action {
			PerformAction::Play { target, .. } => {
				match &state.deck[*target].id() {
					None => new_state.strikes += 1,
					Some(id) => {
						if state.is_playable(*id) {
							new_state.play_stacks[id.suit_index] = id.rank;

							if id.rank == 5 {
								new_state.clue_tokens = std::cmp::min(state.clue_tokens + 1, 8);
							}
						}
						else {
							new_state.strikes += 1;
							new_state.discard_stacks[id.suit_index][id.rank - 1] += 1;
						}
					}
				}
				remove_and_draw_new(player_index, *target);
			}
			PerformAction::Discard { target, .. } => {
				if let Some(id) = state.deck[*target].id() {
					new_state.discard_stacks[id.suit_index][id.rank - 1] += 1;
				}

				new_state.clue_tokens = std::cmp::min(state.clue_tokens + 1, 8);
				remove_and_draw_new(player_index, *target);
			}
			PerformAction::Colour { .. } | PerformAction::Rank {.. } => {
				new_state.clue_tokens -= 1;
				new_state.endgame_turns = new_state.endgame_turns.map(|turns| turns - 1);
			}
			_ => {}
		}

		new_state
	}
}
