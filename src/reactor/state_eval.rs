use colored::Colorize;
use fraction::{ConstZero, Fraction};
use log::{info, warn};

use crate::basics::game::SimOpts;
use crate::basics::state::State;
use crate::basics::variant::all_ids;
use crate::reactor::{ClueInterp, Reactor, ReactorInterp};
use crate::basics::action::{Action, ClueAction, PlayAction};
use crate::basics::card::{CardStatus, Identifiable, Identity};
use crate::basics::game::{Game, Interp};
use crate::basics::clue_result::{bad_touch_result, elim_result, playables_result, BadTouchResult, ElimResult, PlayablesResult};

impl Reactor {
	pub fn get_result(game: &Game, hypo: &Game, action: &ClueAction) -> f32 {
		let Game { state, common, meta, .. } = game;
		let Game { state: hypo_state, common: hypo_common, .. } = hypo;
		let hypo_frame = hypo.frame();
		let ClueAction { giver, target, list, clue } = &action;

		let BadTouchResult { bad_touch, trash, .. } = bad_touch_result(game, hypo, *giver, *target);
		let ElimResult { new_touched, fill, elim } = elim_result(game, hypo, &hypo_state.hands[*target], list);
		let PlayablesResult { playables, .. } = playables_result(game, hypo);

		let revealed_trash = hypo_common.thinks_trash(&hypo_frame, *target).iter().filter(|&o|
			hypo_state.deck[*o].clued && !common.thinks_trash(&game.frame(), *target).contains(o)).count();

		let new_playables = state.hands.concat().iter().filter(|&o| meta[*o].status != CardStatus::CalledToPlay &&
			hypo.meta[*o].status == CardStatus::CalledToPlay).copied().collect::<Vec<_>>();

		let bad_playable = new_playables.iter().find(|&o|
			!(hypo.me().hypo_plays.contains(o) || (state.in_endgame() && state.deck[*o].id().is_some_and(|i| state.is_playable(i))))
		).copied();

		if let Some(bad_playable) = bad_playable {
			warn!("clue {} results in {} {} looking playable!", clue.fmt(state, *target), state.log_iden(&state.deck[bad_playable]), bad_playable);
			return -100.0;
		}

		if hypo.state.clue_tokens == Fraction::ZERO &&
			let Some(bad_zcs) = state.hands.concat().iter().find(|&&o| hypo.meta[o].status == CardStatus::ZeroClueChop && state.deck[o].id().is_some_and(|i| state.is_critical(i))) {
			warn!("clue {} results in bad zcs {} {bad_zcs}!", clue.fmt(state, *target), state.log_iden(&state.deck[*bad_zcs]));
			return -100.0;
		}

		if let Some(Interp::Reactor(ReactorInterp::Clue(last_move))) = &hypo.last_move {
			if (last_move == &ClueInterp::RefPlay || last_move == &ClueInterp::Reclue) && playables.is_empty() && !state.in_endgame() {
				warn!("clue {} looks like {:?} but gets no playables!", clue.fmt(state, *target), last_move);
				return -100.0;
			}

			if last_move == &ClueInterp::Reveal && playables.is_empty() && !trash.is_empty() && trash.iter().all(|o| !state.deck[*o].clued) {
				warn!("clue {} only reveals new trash but isn't a trash push!", clue.fmt(state, *target));
				return -100.0;
			}

			if last_move != &ClueInterp::Reactive && !bad_touch.is_empty() && new_touched.iter().all(|o| bad_touch.contains(o)) && playables.is_empty() {
				warn!("clue {} only bad touches and gets no playables! {:?}", clue.fmt(state, *target), common.hypo_plays);
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

		info!("good touch: {}, playables: [{}], duped: {}, trash: {}, fill: {}, elim: {}, bad_touch: {:?}, {:?}",
			good_touch,
			playables.iter().map(|&o| state.log_iden(&state.deck[o])).collect::<Vec<String>>().join(", "),
			duped_playables,
			trash.len(),
			fill.len(),
			elim.len(),
			bad_touch,
			hypo.last_move
		);

		let mut value: f32 = good_touch
			+ (playables.len() as f32 - 2.0*duped_playables as f32)
			+ 0.2 * untouched_plays as f32
			+ if state.in_endgame() { 0.01 } else { 0.1 } * revealed_trash as f32
			+ if state.in_endgame() { 0.2 } else { 0.1 } * fill.len() as f32
			+ if state.in_endgame() { 0.1 } else { 0.05 } * elim.len() as f32
			+ 0.1 * bad_touch.len() as f32;

		match hypo.last_move {
			Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Mistake))) => value -= 10.0,
			Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Fix))) => value += 1.0,
			Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Reactive))) => value += 1.0,
			_ => ()
		}

		value
	}

	fn advance_game(game: &Game, action: &Action) -> Game {
		match action {
			Action::Clue(clue) => game.simulate_clue(clue, SimOpts { log: true, ..SimOpts::default() }),
			_ => game.simulate_action(action, None)
		}
	}

	fn advance(game: &Game, offset: usize) -> f32 {
		let Game { state, common, meta, .. } = game;
		let frame = game.frame();
		let player_index = (state.our_player_index + offset) % state.num_players;

		if player_index == state.our_player_index || state.endgame_turns.is_some_and(|t| t == 0) {
			return Reactor::eval_game(game);
		}

		let trash = game.players[player_index].thinks_trash(&frame, player_index);
		let urgent_dc = trash.iter().find(|o| meta[**o].urgent);

		let all_playables = game.players[player_index].obvious_playables(&frame, player_index);
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
						let action = if state.is_playable(id) {
							Action::play(player_index, order, id.suit_index as i32, id.rank as i32)
						} else {
							warn!("not playable! {}", state.log_id(id));
							Action::discard(player_index, order, id.suit_index as i32, id.rank as i32, true)
						};
						(Some(id), action)
					}
				};

				info!("{} playing {}", state.player_names[player_index], state.log_oid(&id));
				Reactor::advance(&Reactor::advance_game(game, &action), offset + 1)
			});
			return play_actions.fold(f32::MIN, |a, b| a.max(b));
		}

		if game.players[player_index].thinks_locked(&frame, player_index) {
			return if !state.can_clue() {
				let locked_dc = game.players[player_index].locked_discard(state, player_index);
				let id = state.deck[locked_dc].id().unwrap();
				let action = Action::discard(player_index, locked_dc, id.suit_index as i32, id.rank as i32, false);
				info!("locked discard!");
				Reactor::advance(&Reactor::advance_game(game, &action), offset + 1)
			}
			else {
				let mut next_game = game.simulate_clean();
				next_game.state.clue_tokens -= 1;

				Reactor::advance(&next_game, offset + 1)
			}
		}

		if state.clue_tokens == Fraction::from(8) {
			let mut next_game = game.simulate_clean();
			next_game.state.clue_tokens -= 1;
			info!("forced clue at 8 clues!");

			return Reactor::advance(&next_game, offset + 1);
		}

		let bob = state.next_player_index(player_index);

		if !state.hands[player_index].iter().any(|&o| meta[o].urgent) && offset == 1 && !common.thinks_loaded(&frame, bob) && let Some(chop) = Reactor::chop(game, bob) {
			let id = state.deck[*chop].id().unwrap();

			// Assume Alice will clue Bob
			if state.can_clue() && (state.is_critical(id) || state.is_playable(id)) {
				let mut next_game = game.simulate_clean();
				next_game.state.clue_tokens -= 1;
				info!("forcing {} to clue bob!", state.player_names[player_index]);
				return Reactor::eval_game(&next_game);
			}
		}

		let trash = game.players[player_index].thinks_trash(&frame, player_index);

		match urgent_dc.or_else(|| trash.first()) {
			None => {
				if let Some(chop) = Reactor::chop(game, player_index) {
					let id = state.deck[*chop].id().unwrap();
					let action = Action::discard(player_index, *chop, id.suit_index as i32, id.rank as i32, false);
					let dc_game = Reactor::advance_game(game, &action);

					if state.clue_tokens > Fraction::from(2) {
						let mut clue_game = game.simulate_clean();
						clue_game.state.clue_tokens -= 1;

						let clue_prob = if offset == 1 {
							if common.thinks_loaded(&frame, bob) {
								0.2
							} else if let Some(chop) = Reactor::chop(game, bob) {
								if state.is_basic_trash(state.deck[*chop].id().unwrap()) { 0.2 } else { 0.7 }
							} else {
								0.5
							}
						} else {
							0.8
						};

						info!("{} discarding {} but might clue {}", state.player_names[player_index], state.log_id(id), clue_prob);
						clue_prob * Reactor::advance(&clue_game, offset + 1) + (1.0 - clue_prob) * Reactor::advance(&dc_game, offset + 1)
					}
					else {
						info!("{} discarding {}", state.player_names[player_index], state.log_id(id));
						Reactor::advance(&dc_game, offset + 1)
					}
				}
				else {
					panic!("Player {} not locked but no chop!", state.player_names[player_index]);
				}
			},
			Some(order) => {
				let id = state.deck[*order].id().unwrap();
				let Identity { suit_index, rank } = id;
				let action = Action::discard(player_index, *order, suit_index as i32, rank as i32, false);

				info!("{} discarding {}", state.player_names[player_index], state.log_id(id));
				Reactor::advance(&Reactor::advance_game(game, &action), offset + 1)
			}
		}
	}

	pub fn eval_action(game: &Game, action: &Action) -> f32 {
		info!("{}", format!("===== Predicting value for {} =====", action.fmt(&game.state)).green());

		let Game { state, .. } = game;
		let hypo_game = Reactor::advance_game(game, action);

		let value = match action {
			Action::Clue(clue) => {
				if matches!(hypo_game.last_move, Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Mistake))) | Some(Interp::Reactor(ReactorInterp::Clue(ClueInterp::Illegal)))) {
					return -100.0;
				}

				let mult = if !game.me().obvious_playables(&game.frame(), state.our_player_index).is_empty() {
					if state.in_endgame() { 0.1 } else { 0.25 }
				} else {
					0.5
				};

				Reactor::get_result(game, &hypo_game, clue) * mult - 0.5
			},
			Action::Play(PlayAction { suit_index, rank, .. }) => {
				if *suit_index == -1 || *rank == -1 {
					1.5
				}
				else {
					0.0
				}
			},
			_ => 0.0
		};

		info!("starting value {value}");

		let best = value + Reactor::advance(&hypo_game, 1);
		info!("{}: {} ({:?})", action.fmt(state), best, hypo_game.last_move.unwrap());
		best
	}

	fn eval_state(state: &State) -> f32 {
		// The first 2 * (# suits) pts are worth 2.
		let mut score_val = std::cmp::min(state.score(), 2 * state.variant.suits.len()) as f32;
		score_val += state.score() as f32;

		let clues: f32 = state.clue_tokens.try_into().unwrap();

		let clue_val = if state.clue_tokens == Fraction::ZERO {
			-0.5
		} else if !state.can_clue() {
			-0.25
		} else if clues > 6.0 {
			3.0 + (clues - 6.0) * 0.25
		} else {
			clues / 2.0
		};

		let score_loss = state.variant.suits.len() * 5 - state.max_score();
		let dc_crit_val = -((8 * score_loss) as f32);

		let strikes_val = if state.strikes == 1 {
			-1.5
		} else if state.strikes == 2 {
			-3.5
		} else if state.strikes == 3 {
			-100.0
		} else {
			0.0
		};

		info!("state eval: score {score_val}, clues {clue_val}, dc crit {dc_crit_val}, strikes {strikes_val}");

		score_val + clue_val + dc_crit_val + strikes_val
	}

	fn eval_game(game: &Game) -> f32 {
		let mut value = 0.0;
		let Game { state, meta, .. } = game;

		if game.state.score() == game.state.max_score() {
			return 100.0;
		}

		value += Reactor::eval_state(state);

		let mut future_val = 0.0;

		for &order in &state.hands.concat() {
			if meta[order].status == CardStatus::CalledToPlay {
				future_val += match state.deck[order].id() {
					None => 0.4,
					Some(id) => if state.is_basic_trash(id) {
							-1.5
						} else if id.rank == 5 {
							0.8
						} else {
							0.4
						}
				};
			}
			else if meta[order].status == CardStatus::CalledToDiscard {
				let by = meta[order].by.unwrap_or_else(|| panic!("order {order} doesn't have a by!"));

				match state.deck[order].id() {
					None => {
						// Trust others to discard trash
						if by != state.our_player_index {
							continue;
						}
						future_val += 0.5;
					},
					Some(id) => if state.is_basic_trash(id) {
						future_val += 1.0;
					} else if game.me().is_sieved(&game.frame(), state.deck[order].id().unwrap(), order) {
						future_val += 0.5;
					} else if state.is_critical(id) {
						future_val -= (5.0 - state.playable_away(id) as f32) * 10.0;
					} else if by != state.our_player_index {
						continue;
					}else {
						future_val -= (5.0 - state.playable_away(id) as f32) * 0.5;
					}
				}
			}
		}

		value += future_val;

		let mut bdr_val = 0.0;

		for id in all_ids(&state.variant) {
			if state.is_basic_trash(id) || id.rank == 5 {
				continue;
			}

			let discarded = &state.discard_stacks[id.suit_index][id.rank - 1];

			if discarded.is_empty() {
				continue;
			}

			// Trust others to discard stuff duplicated in our hand
			let duplicated = state.hands.concat().iter().any(|&o| state.deck[o].is(&id)) ||
				(discarded.iter().all(|&o| meta[o].by.is_some_and(|by| by != state.our_player_index)) && state.our_hand().iter().any(|&o| game.me().thoughts[o].possible.contains(id)));

			if duplicated {
				bdr_val -= 0.1;
			} else if id.rank == 1 {
				bdr_val -= (discarded.len() * discarded.len()) as f32;
			} else if id.rank == 2 {
				bdr_val -= 3.0;
			} else if id.rank == 3 {
				bdr_val -= 1.5;
			} else {
				bdr_val -= 1.0;
			}
		}

		bdr_val *= 2.5;

		value += bdr_val;

		// let mut locked_val = 0.0;

		// for i in 0..state.num_players {
		// 	if game.players[i].thinks_locked(&game.frame(), i) {
		// 		locked_val -= if state.clue_tokens < 2 { 2.0 } else { 1.0 };
		// 	}
		// }

		// value += locked_val;

		info!("future: {future_val}, bdr: {bdr_val}");
		value
	}
}
