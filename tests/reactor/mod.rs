use rust_bot::basics::action::{Action, ClueAction, PerformAction};
use rust_bot::basics::card::CardStatus;
use rust_bot::basics::clue::BaseClue;
use rust_bot::basics::{clue::ClueKind};
use rust_bot::basics::game::{Game, SimOpts};
use rust_bot::reactor::{Reactor};
use std::sync::Arc;

use crate::ex_asserts;
use crate::util::{self, fully_known, pre_clue, take_turn, Colour, Player, TestClue, TestOptions};

mod stable;
mod reactive;
mod invert;
mod variants;
mod mistakes;
mod stalling;

#[test]
fn it_understands_good_touch() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "g2", "r2", "r3", "g5"],
		&["p4", "b5", "p2", "b1", "g4"],
	], TestOptions {
		play_stacks: Some(&[2, 0, 0, 0, 0]),
		starting: Player::Cathy,
		init: Box::new(|game: &mut Game| {
			// Bob has a known r4 in slot 1.
			pre_clue(game, Player::Bob, 1, &[
				TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }
			]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues red to Alice (slots 1,2)");	// Targeting r3 in slot 1
	take_turn(&mut game, "Alice plays r3 (slot 1)");

	// Alice's slot 2 should be r4,r5.
	ex_asserts::has_inferences(&game, None, Player::Alice, 2, &["r4", "r5"]);

	// Bob's slot 1 should be known r4.
	ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r4"]);
}

#[test]
fn it_elims_from_focus() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["y4", "g2", "r2", "r3", "g5"],
		&["p4", "b5", "p2", "b1", "g4"],
	], TestOptions {
		play_stacks: Some(&[4, 0, 0, 0, 0]),
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues red to Alice (slots 1,2)");

	// Alice's slot 1 should be known r5.
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r5"]);

	// Alice's slot 2 should be known trash.
	let hand = &game.state.hands[Player::Alice as usize];
	assert!(game.common.thinks_trash(&game.frame(), Player::Alice as usize).contains(&hand[1]));
}

#[test]
fn it_understands_a_stable_clue_to_cathy() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r4", "r4", "y4", "y4"],
		&["g1", "g4", "g4", "b4", "b4"],
	], TestOptions {
		// Bob's slot 1 is clued with 1.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Bob, 1, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues green to Cathy");

	// Cathy is called to play g1.
	// ex_asserts::has_inferences(&game, None, Player::Cathy, 1, &["g1"]);
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][0]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays b1, drawing p4");
}

#[test]
fn it_understands_a_reverse_reactive_clue() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "r4", "y4", "y4"],
		&["g4", "g1", "g4", "b4", "b4"],
	], TestOptions {
		clue_tokens: 7,
		// Bob's slot 2 is clued with 1.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Bob, 2, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues 4 to Bob");

	// Cathy is called to play g1.
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][1]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays b1, drawing y3");

	assert!(game.common.obvious_playables(&game.frame(), Player::Cathy as usize).contains(&game.state.hands[Player::Cathy as usize][1]));
}

#[test]
fn it_doesnt_give_a_bad_reverse_reactive_clue() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "r4", "y4", "y5"],
		&["y1", "g4", "g4", "b4", "b4"],
	], TestOptions {
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 5 to Bob");
	take_turn(&mut game, "Alice plays g1 (slot 4)");		// targeting Bob's b1
	take_turn(&mut game, "Bob clues green to Cathy");

	take_turn(&mut game, "Cathy plays y1, drawing b1");

	// let action = game.take_action();
	// println!("action {:?}", action);

	// We cannot give 4 to Bob as a reverse reactive, since Cathy would play b1 to react into r1 and Bob is already playing b1.
	let clue = ClueAction {
		giver: Player::Alice as usize,
		target: Player::Bob as usize,
		list: vec![6, 7],
		clue: BaseClue { kind: ClueKind::RANK, value: 4 }
	};

	let result = Reactor::eval_action(&game, &Action::Clue(clue));

	// let new_game = game.simulate_clue(&clue, SimOpts { log: true, ..SimOpts::default() });
	// let result = Reactor::get_result(&game, &new_game, &clue);

	assert!(result < 8.0);
}

#[test]
fn it_understands_targeting_dupes() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b3", "r4", "r4", "y4", "y5"],
		&["g4", "g1", "g4", "b4", "b4"],
	], TestOptions {
		starting: Player::Cathy,
		// Bob's slots 2 and 3 are clued with red.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Bob, 2, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);
			pre_clue(game, Player::Bob, 3, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	// 4 + 2 = 1
	take_turn(&mut game, "Cathy clues blue to Bob");

	// Alice is called to play slot 4.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Alice plays r1 (slot 4)");

	// Bob is called to discard slot 2.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_understands_a_known_delayed_stable_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g3", "y5", "g4", "b4", "b4"],
		&["b1", "r1", "r4", "y4", "y4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[0, 0, 1, 0, 0]),
		// Alice has a known r1 (slot 1) and a known g2 (slot 2).
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 1, "r1");
			fully_known(game, Player::Alice, 2, "g2");
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues yellow to Bob");

	// Bob is called to play g3.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);

	let action = game.take_action();

	// We should play g2 into it.
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][1] });
}

#[test]
fn it_understands_an_unknown_delayed_stable_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g2", "y5", "g4", "b4", "b4"],
		&["b1", "r1", "r4", "y4", "y4"],
	], TestOptions {
		starting: Player::Cathy,
		// Alice's slot 1 is clued with 1.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Alice, 1, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Bob }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues yellow to Bob");

	// Bob is called to play g2.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);

	let action = game.take_action();

	// We should play our 1 into it as g1.
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0] });
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["g1"]);
}

#[test]
fn it_doesnt_give_a_bad_connecting_clue() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "y1", "g1", "g2", "p2"],
		&["y1", "p3", "g5", "p5", "r5"],
	], TestOptions {
		play_stacks: Some(&[1, 1, 1, 1, 1]),
		// Bob's slots 4 and 5 are clued with 2.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::RANK, value: 2, giver: Player::Alice }]);
			pre_clue(game, Player::Bob, 5, &[TestClue { kind: ClueKind::RANK, value: 2, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	let clue = ClueAction {
		giver: Player::Alice as usize,
		target: Player::Cathy as usize,
		list: vec![12],
		clue: BaseClue { kind: ClueKind::COLOUR, value: Colour::Green as usize }
	};

	let new_game = game.simulate_clue(&clue, SimOpts { log: true, ..Default::default() });
	let result = Reactor::get_result(&game, &new_game, &clue);

	assert_eq!(result, -100.0);
}

#[test]
fn it_understands_bob_wont_react_if_alternative_is_on_them() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["y1", "r2", "g1", "g2", "p2"],
		&["r3", "p4", "g5", "y4", "r4"],
	], TestOptions {
		play_stacks: Some(&[1, 0, 0, 0, 0]),
		// Cathy's g5 is clued with green.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Cathy, 3, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Green as usize, giver: Player::Alice }]);
		}),
		clue_tokens: 8,
		..TestOptions::default()
	});

	// Trying to get a finesse, but Bob doesn't see another clue for Alice to give.
	take_turn(&mut game, "Alice clues 5 to Cathy");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::None);
}
