use log::{info, warn};

use crate::basics::game::SimOpts;
use crate::reactor::{ClueInterp, Reactor, ReactorInterp};
use crate::basics::action::{Action, DiscardAction, PlayAction};
use crate::basics::card::{CardStatus, Identifiable, Identity};
use crate::basics::{action::ClueAction, game::{Game, Interp}};
use crate::basics::clue_result::{bad_touch_result, elim_result, playables_result, BadTouchResult, ElimResult, PlayablesResult};

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
			warn!("clue {} results in {} looking playable!", clue.fmt(state, *target), state.deck[bad_playable].id().map(|i| state.log_id(i)).unwrap_or(format!("order {bad_playable}")));
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

		// Previously-unclued playables whose copies are already touched
		let duped_playables = hypo.me().hypo_plays.iter().filter(|&&p|
			!state.deck[p].clued &&
			state.hands.concat().iter().any(|&o| o != p && game.frame().is_touched(o) && state.deck[o].is(&state.deck[p]))
		).count();

		let good_touch = if bad_touch.len() >= new_touched.len() { -(bad_touch.len() as f32) } else { [0.0, 0.25, 0.5, 0.6, 0.7, 0.75][new_touched.len() - bad_touch.len()] };

		let untouched_plays = playables.iter().filter(|&&p| !hypo_state.deck[p].clued).count();

		info!("good touch: {}, playables: [{}], duped: {}, trash: {}, fill: {}, elim: {}, bad_touch: {:?}",
			good_touch,
			playables.iter().map(|&o| state.log_iden(&state.deck[o])).collect::<Vec<String>>().join(", "),
			duped_playables,
			trash.len(),
			fill.len(),
			elim.len(),
			bad_touch
		);

		let mut value: f32 = good_touch
			+ (playables.len() as f32 - 2.0*duped_playables as f32)
			+ 0.2 * untouched_plays as f32
			+ if state.in_endgame() { 0.01 } else { 0.1 } * revealed_trash as f32
			+ if state.in_endgame() { 0.2 } else { 0.1 } * fill.len() as f32
			+ if state.in_endgame() { 0.1 } else { 0.05 } * elim.len() as f32
			+ 0.1 * bad_touch.len() as f32;

		if let Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Fix))) = hypo.last_move {
			value += 1.0;
		}

		value
	}

	fn advance_game(game: &Game, action: &Action) -> Game {
		match action {
			Action::Clue(clue) => game.simulate_clue(clue, SimOpts { log: true, ..SimOpts::default() }),
			_ => game.simulate_action(action)
		}
	}

	fn best_value(prev: &Game, game: &Game, offset: usize, value: f32) -> f32 {
		let Game { state, common, meta, .. } = game;
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
			if state.in_endgame() || state.rem_score() < 2*state.variant.suits.len() || prev.players[player_index].thinks_loaded(&prev.frame(), player_index) {
				return false;
			}

			let chop = state.hands[player_index][0];
			let id = state.deck[chop].id().unwrap();

			// Trash or same-hand dupe
			state.is_basic_trash(id) || state.hands[player_index].iter().any(|&o| o != chop && state.deck[o].is(&id))
		};

		let trash = game.players[player_index].thinks_trash(&frame, player_index);
		let urgent_dc = trash.iter().find(|o| meta[**o].urgent);

		let all_playables = game.players[player_index].thinks_playables(&frame, player_index);
		if urgent_dc.is_none() && !all_playables.is_empty() {
			let urgent_play = all_playables.iter().find(|o| meta[**o].urgent);

			let playables = match urgent_play {
				Some(order) => vec![*order],
				None => all_playables.iter().filter(|&&o|
						// Only consider playing the leftmost of similarly-possible cards
						!all_playables.iter().any(|&p| p > o && common.thoughts[p].possible == common.thoughts[o].possible)
					).copied().collect::<Vec<_>>(),
			};

			let play_actions = playables.iter().map(|&order| {
				let (id, action) = match state.deck[order].id() {
					None => {
						let action = Action::play(player_index, order, -1, -1);
						(None, action)
					}
					Some(id) => {
						let Identity { suit_index, rank } = id;
						let action = Action::play(player_index, order, suit_index as i32, rank as i32);
						(Some(id), action)
					}
				};

				let diff = if id.is_some_and(|i| state.is_playable(i)) {
					if id.unwrap().rank == 5 { 1.75 } else { 1.5 }
				} else {
					-10.0
				} + if sieving_trash() { -10.0 } else { 0.0 };
				let new_value = value + mult(diff);

				info!("{} playing {} {}{}", state.player_names[player_index], state.log_oid(&id), mult(diff), if sieving_trash() { ", sieving trash!" } else { "" });
				Reactor::best_value(prev, &Reactor::advance_game(game, &action), offset + 1, new_value)
			});
			return play_actions.fold(f32::MIN, |a, b| a.max(b));
		}

		if game.players[player_index].thinks_locked(&frame, player_index) || (offset == 1 && state.clue_tokens == 8) {
			if state.clue_tokens == 0 {
				warn!("forcing discard at 0 clues from locked hand!");
				return -15.0;
			}

			let mut next_game = game.simulate_clean();
			next_game.state.clue_tokens -= 1;

			let diff = if state.clue_tokens == 0 || sieving_trash() { -10.0 } else { 0.25 };
			let new_value = value + mult(diff);

			info!("{} forced clue {}", state.player_names[player_index], mult(diff));
			return Reactor::best_value(prev, &next_game, offset + 1, new_value);
		}

		let trash = game.players[player_index].thinks_trash(&frame, player_index);
		let discard = urgent_dc.unwrap_or_else(|| trash.first().unwrap_or(&state.hands[player_index][0]));

		let (id_str, action, dc_value) = match state.deck[*discard].id() {
			None => {
				let action = Action::discard(player_index, *discard, -1, -1, false);
				("xx".to_owned(), action, 0.0)
			}
			Some(id) => {
				let Identity { suit_index, rank } = id;
				let action = Action::discard(player_index, *discard, suit_index as i32, rank as i32, false);

				let dc_value = game.me().card_value(&frame, id, Some(*discard)) as f32;
				(state.log_id(id), action, dc_value)
			}
		};

		let diff = (if state.in_endgame() { 0.0 } else { 10.0 } as f32)
			.min(0.25 + if dc_value == 0.0 { 1.0 } else { - dc_value*0.5 })
				+ if *discard != state.hands[player_index][0] && sieving_trash() { -10.0 } else { 0.0 };
		let new_value = value + mult(diff);

		info!("{} discarding {} {}{}", state.player_names[player_index], id_str, mult(diff), if *discard != state.hands[player_index][0] && sieving_trash() { ", sieving trash!" } else { "" });
		Reactor::best_value(prev, &Reactor::advance_game(game, &action), offset + 1, new_value)
	}

	pub(super) fn predict_value(game: &Game, action: &Action) -> f32 {
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
				let useful_count = state.our_hand().iter().filter(|&&o|
					state.deck[o].clued && game.me().thoughts[o].inferred.iter().all(|i| !state.is_basic_trash(i))).count();

				let mult = if state.in_endgame() {
					0.2 * (1_i32 - useful_count as i32) as f32 - (state.num_players as i32 - state.pace()) as f32 * 0.1
				} else if !game.me().thinks_playables(&game.frame(), state.our_player_index).is_empty() {
					if state.in_endgame() { 0.1 } else { 0.25 }
				} else {
					1.0
				};

				mult * if common.thinks_trash(&game.frame(), *player_index).contains(order) {
					(if state.clue_tokens <= 2 { 1.2 } else if state.clue_tokens <= 4 { 1.0 } else { 0.8 }) *
					(if state.rem_score() <= state.variant.suits.len() { 0.1 } else if state.rem_score() <= 2*state.variant.suits.len() { 0.5 } else { 1.0 })
				} else { 0.5 }
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
						common.thoughts[*o].inferred.iter().any(|i| i != id && !state.is_basic_trash(i)));

					if duplicated { if state.in_endgame() { 0.5 } else { 0.0 } } else { 1.5 }
				}
			},
			_ => -1.0
		};
		info!("starting value {value}");

		let best = Reactor::best_value(game, &hypo_game, 1, value);
		info!("{}: {} ({:?})", action.fmt(state), best, hypo_game.last_move.unwrap());
		best
	}
}
