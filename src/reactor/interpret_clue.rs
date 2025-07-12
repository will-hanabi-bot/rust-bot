use log::{info, warn};
use std::mem;

use crate::basics::card::{CardStatus, Identifiable, Identity};
use crate::basics::clue::ClueKind;
use crate::basics::game::frame::Frame;
use crate::basics::game::Game;
use crate::basics::player::WaitingConnection;
use crate::basics::variant::{BROWNISH, PINKISH, RAINBOWISH};
use crate::fix::check_fix;
use crate::reactor::{ClueInterp, Reactor};
use crate::basics::action::{ClueAction};

impl Reactor {
	pub(super) fn interpret_fix(prev: &Game, game: &mut Game, action: &ClueAction) -> ClueInterp {
		info!("interpreting clue when both players are loaded!");
		let ClueAction { giver, target, .. } = &action;
		let Game { state, .. } = game;

		if state.next_player_index(*giver) != *target {
			info!("target is not the next player!");
			return ClueInterp::None;
		}

		let (clued_resets, duplicate_reveals) = check_fix(prev, game, action);
		let prev_playables = prev.players[*target].thinks_playables(&prev.frame(), *target);
		if clued_resets.iter().chain(duplicate_reveals.iter()).any(|o| prev_playables.contains(o)) {
			info!("fix clue!");
			return ClueInterp::Reveal;
		}
		info!("not an urgent fix clue, not interpreting");
		ClueInterp::None
	}

	pub(super) fn interpret_stable(prev: &Game, game: &mut Game, action: &ClueAction) -> ClueInterp {
		info!("interpreting stable clue!");
		let ClueAction { target, list, clue, .. } = &action;
		let (clued_resets, duplicate_reveals) = check_fix(prev, game, action);

		if clue.kind == ClueKind::RANK && game.state.includes_variant(&PINKISH) {
			let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();
			let mut focus = newly_touched.iter().max().unwrap();

			// Trash pink promise
			if (0..game.state.variant.suits.len()).all(|suit_index| game.state.is_basic_trash(&Identity { suit_index, rank: clue.value })) {
				game.common.thoughts[*focus].inferred.retain(|i| game.state.is_basic_trash(i));
				game.meta[*focus].trash = true;
			}
			// Playable pink promise
			else if (0..game.state.variant.suits.len()).all(|suit_index| {
				let id = Identity { suit_index, rank: clue.value };
				game.state.is_basic_trash(&id) || game.state.is_playable(&id)
			}) {
				// Move focus to lock card if touched
				if let Some(lock_order) = game.state.hands[*target].iter().filter(|&&o| !prev.state.deck[o].clued).min() {
					if list.contains(lock_order) {
						focus = lock_order;
					}
				}

				game.common.thoughts[*focus].inferred.retain(|i| game.state.is_playable(i) && i.rank == clue.value);
				game.meta[*focus].focused = true;
			}
		}

		let frame = Frame::new(&game.state, &game.meta);
		game.common.good_touch_elim(&frame);
		game.common.refresh_links(&frame, true);

		if !clued_resets.is_empty() || !duplicate_reveals.is_empty() {
			info!("fix clue!");
			return ClueInterp::Reveal;
		}

		let Game { common: prev_common, .. } = prev;
		let Game { state, common, .. } = &game;
		let frame = game.frame();

		if !state.in_endgame() && !prev_common.thinks_playables(&prev.frame(), *target).is_empty() {
			warn!("target was already loaded with a playable!");
			return ClueInterp::None;
		}

		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();
		if newly_touched.is_empty() {
			return ClueInterp::Reveal;
		}

		let trash_push = common.order_kt(&frame, *newly_touched.iter().max().unwrap());
		if trash_push {
			// Brownish TCM if there is at least 1 useful unplayable brown and clue didn't touch chop
			if state.includes_variant(&BROWNISH) && clue.kind == ClueKind::RANK &&
				state.variant.suits.iter().enumerate().any(|(suit_index, suit)| BROWNISH.is_match(suit) && state.play_stacks[suit_index] + 1 < state.max_ranks[suit_index]) &&
				!newly_touched.contains(&state.hands[*target][0]) {
					info!("brown direct discard!");
			}
			else {
				info!("trash push!");
				return Reactor::ref_play(prev, game, action);
			}
		}

		let loaded = common.thinks_loaded(&frame, *target);
		let reveal = loaded && (clue.kind == ClueKind::RANK || {
			let prev_playables = prev_common.thinks_playables(&prev.frame(), *target);
			let curr_playables = common.thinks_playables(&frame, *target);

			// A colour clue that reveals a new playable in a previously touched card
			curr_playables.iter().any(|o| !prev_playables.contains(o) && prev.state.deck[*o].clued)
		});

		if reveal {
			info!("revealed a safe action!");
			return ClueInterp::Reveal;
		}

		if clue.kind == ClueKind::COLOUR {
			info!("colour clue!");
			Reactor::ref_play(prev, game, action)
		}
		else {
			info!("rank clue!");
			Reactor::ref_discard(prev, game, action)
		}
	}

	pub(super) fn interpret_reactive(prev: &Game, game: &mut Game, action: &ClueAction, reacter: usize) -> ClueInterp {
		let Game { common: prev_common, .. } = prev;
		let Game { common, state, meta, .. } = game;
		let ClueAction { giver, target: receiver, list, clue } = action;

		info!("interpreting reactive clue!");

		let (focus_index, focus) = state.hands[*receiver].iter().enumerate()
			.filter(|&(_, o)| list.contains(o))
			.max_by_key(|&(_, o)| {
				if *o == state.hands[*receiver][0] { 0 } else { *o }
			}).unwrap();

		if *receiver == state.our_player_index {
			let focus_index = state.hands[*receiver].iter().position(|o| o == focus).unwrap();
			let focus_slot = match clue.kind {
				ClueKind::COLOUR => if state.includes_variant(&RAINBOWISH) { clue.value + 1 } else { focus_index + 1 },
				ClueKind::RANK => if state.includes_variant(&PINKISH) { clue.value } else { focus_index + 1 }
			};

			common.waiting.push(WaitingConnection {
				giver: *giver,
				reacter,
				receiver: *receiver,
				receiver_hand: state.hands[*receiver].clone(),
				clue: *clue,
				focus_slot
			});
			return ClueInterp::Reactive;
		}

		match clue.kind {
			ClueKind::COLOUR => {
				let known_plays = prev_common.thinks_playables(&prev.frame(), *receiver);
				let target = state.hands[*receiver].iter().enumerate().filter(|&(_, o)| !known_plays.contains(o) && state.is_playable(state.deck[*o].id().unwrap())).min();

				match target {
					None => {
						let prev_known_trash = prev_common.thinks_trash(&prev.frame(), *receiver);
						let known_trash = common.thinks_trash(&Frame::new(state, meta), *receiver);
						let all_trash = state.hands[*receiver].iter().enumerate().filter(|&(_, o)|
							!prev_known_trash.contains(o) &&
							!known_trash.contains(o) &&
							state.is_basic_trash(state.deck[*o].id().unwrap())
						).collect::<Vec<_>>();

						match all_trash.iter().find(|&(_, o)| state.deck[**o].clued).or_else(|| all_trash.first()) {
							None => {
								warn!("Reactive clue but receiver had no playable or trash targets!");
								ClueInterp::None
							}
							Some((index, target)) => {
								let target_slot = index + 1;
								let focus_slot = if state.includes_variant(&RAINBOWISH) { clue.value + 1 } else { focus_index + 1 };
								let mut react_slot = (focus_slot + 5 - target_slot) % 5;
								if react_slot == 0 {
									react_slot = 5;
								}

								let react_order = state.hands[reacter][react_slot - 1];
								let receive_order = **target;

								Reactor::target_play(game, action, react_order, true);
								Reactor::target_discard(game, action, receive_order, true);
								game.meta[receive_order].depends_on = Some(vec![react_order]);

								info!("reactive play+dc, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
								ClueInterp::Reactive
							}
						}
					}
					Some((index, target)) => {
						let target_slot = index + 1;
						let focus_slot = if state.includes_variant(&RAINBOWISH) { clue.value + 1 } else { focus_index + 1 };
						let mut react_slot = (focus_slot + 5 - target_slot) % 5;
						if react_slot == 0 {
							react_slot = 5;
						}

						let react_order = state.hands[reacter][react_slot - 1];
						let receive_order = *target;

						Reactor::target_discard(game, action, react_order, true);
						Reactor::target_play(game, action, receive_order, false);
						game.meta[receive_order].depends_on = Some(vec![react_order]);

						info!("reactive dc+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
						ClueInterp::Reactive
					}
				}
			}
			ClueKind::RANK => {
				let known_plays = prev_common.thinks_playables(&prev.frame(), *receiver);

				match state.hands[*receiver].iter().enumerate().filter(|&(_, o)| !known_plays.contains(o) && state.is_playable(state.deck[*o].id().unwrap())).min() {
					None => {
						warn!("Reactive clue but receiver had no playable targets!");
						ClueInterp::None
					}
					Some((index, target)) => {
						let target_slot = index + 1;
						let focus_slot = if state.includes_variant(&PINKISH) { clue.value } else { focus_index + 1 };
						let mut react_slot = (focus_slot + 5 - target_slot) % 5;
						if react_slot == 0 {
							react_slot = 5;
						}

						let react_order = state.hands[reacter][react_slot - 1];
						let receive_order = *target;

						Reactor::target_play(game, action, react_order, true);
						game.common.thoughts[react_order].inferred.retain(|i| i != game.state.deck[receive_order].id().unwrap());
						Reactor::target_play(game, action, receive_order, false);
						game.meta[receive_order].depends_on = Some(vec![react_order]);

						info!("reactive play+play, reacter {} (slot {}) receiver {} (slot {}), focus slot {}", game.state.player_names[reacter], react_slot, game.state.player_names[*receiver], target_slot, focus_slot);
						ClueInterp::Reactive
					}
				}
			}
		}
	}

	fn ref_play(prev: &Game, game: &mut Game, action: &ClueAction) -> ClueInterp {
		let Game { common, state, .. } = game;
		let ClueAction { target: receiver, list, .. } = &action;
		let hand = &state.hands[*receiver];
		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();

		let target = newly_touched.iter().map(|&o| common.refer(&prev.frame(), hand, o, true)).max().unwrap();

		if game.frame().is_blind_playing(target) {
			warn!("targeting an already known playable!");
			return ClueInterp::None;
		}

		if game.meta[target].status == CardStatus::CalledToDiscard {
			warn!("targeting a card called to discard!");
			return ClueInterp::None;
		}

		Reactor::target_play(game, action, target, false);
		ClueInterp::RefPlay
	}

	fn target_play(game: &mut Game, action: &ClueAction, target: usize, urgent: bool) {
		let ClueAction { giver, .. } = action;

		let mut inferred = mem::take(&mut game.common.thoughts[target].inferred);
		inferred.retain(|i| game.state.is_playable(i) && !game.players[*giver].is_trash(&game.frame(), i, target));
		let reset = inferred.is_empty();
		game.common.thoughts[target].inferred = inferred.clone();
		game.common.thoughts[target].info_lock = Some(inferred);

		if reset {
			game.common.thoughts[target].reset_inferences();
		}

		let Game { common, state, .. } = game;
		let meta = &mut game.meta[target];

		meta.status = CardStatus::CalledToPlay;
		meta.focused = true;
		if urgent {
			meta.urgent = true;
		}

		info!("targeting play {}, infs {}", target, common.str_infs(state, target));
	}

	fn target_discard(game: &mut Game, _action: &ClueAction, target: usize, urgent: bool) {
		let mut inferred = mem::take(&mut game.common.thoughts[target].inferred);
		inferred.retain(|i| !game.state.is_critical(i));
		game.common.thoughts[target].inferred = inferred;

		let Game { common, state, .. } = game;
		let meta = &mut game.meta[target];

		meta.status = CardStatus::CalledToDiscard;
		meta.trash = true;
		if urgent {
			meta.urgent = true;
		}

		info!("targeting discard {}, infs {}", target, common.str_infs(state, target));
	}

	fn ref_discard(prev: &Game, game: &mut Game, action: &ClueAction) -> ClueInterp {
		let Game { state, .. } = game;
		let ClueAction { target: receiver, list, clue, .. } = &action;
		let hand = &state.hands[*receiver];
		let newly_touched = list.iter().filter(|&&o| !prev.state.deck[o].clued).copied().collect::<Vec<_>>();

		if let Some(lock_order) = hand.iter().filter(|&&o| !prev.state.deck[o].clued).min() {
			if list.contains(lock_order) {
				info!("locked!");

				// Lock pink promise
				if clue.kind == ClueKind::RANK && state.includes_variant(&PINKISH) {
					game.common.thoughts[*lock_order].inferred.retain(|i| i.rank == clue.value);
					game.meta[*lock_order].focused = true;
				}

				for &order in hand {
					let meta = &mut game.meta[order];

					if !state.deck[order].clued && meta.status == CardStatus::None {
						meta.status = CardStatus::ChopMoved;
					}
				}
				return ClueInterp::Lock;
			}
		}

		let focus = newly_touched.iter().max().unwrap();
		let focus_pos = hand.iter().position(|o| o == focus).unwrap();
		let target_index = hand.iter().enumerate().position(|(i, &o)| i > focus_pos && !state.deck[o].clued).unwrap();
		info!("ref discard on {}'s slot {}", state.player_names[*receiver], target_index + 1);

		let meta = &mut game.meta[hand[target_index]];
		meta.status = CardStatus::CalledToDiscard;
		meta.trash = true;
		ClueInterp::RefDiscard
	}
}
