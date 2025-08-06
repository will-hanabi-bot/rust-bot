use rust_bot::basics::action::{PerformAction};
use rust_bot::basics::card::CardStatus;
use rust_bot::basics::clue::ClueKind;
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::ex_asserts;
use crate::util::{self, pre_clue, take_turn, Colour, Player, TestClue, TestOptions};

#[test]
fn it_understands_a_reactive_play_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 5 to Cathy");

	assert_eq!(&game.meta[game.state.hands[Player::Bob as usize][0]].status, &CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r1", "y1", "b1", "p1"]);

	assert_eq!(&game.meta[game.state.hands[Player::Cathy as usize][0]].status, &CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays b1, drawing p1");
	ex_asserts::has_inferences(&game, None, Player::Cathy, 1, &["r1", "y1", "g1", "b2", "p1"]);
}

#[test]
fn it_understands_a_reactive_dc_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues blue to Cathy");

	assert_eq!(&game.meta[game.state.hands[Player::Bob as usize][0]].status, &CardStatus::CalledToDiscard);
	// ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r1", "y1", "b1", "p1"]);
	assert!(game.common.thinks_trash(&game.frame(), Player::Bob as usize).contains(&game.state.hands[Player::Bob as usize][0]));

	assert_eq!(&game.meta[game.state.hands[Player::Cathy as usize][0]].status, &CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Cathy, 1, &["r1", "y1", "g1", "p1"]);
}

#[test]
fn it_reacts_to_a_reactive_play_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 2 to Bob");

	assert_eq!(&game.meta[game.state.hands[Player::Alice as usize][0]].status, &CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r1", "y1", "g1", "p1"]);

	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { table_id: Some(0), target: game.state.hands[Player::Alice as usize][0] });
}

#[test]
fn it_reacts_to_a_reactive_finesse() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b2", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 3 to Bob");

	// We should play slot 1 to target Bob's r2.
	assert_eq!(&game.meta[game.state.hands[Player::Alice as usize][0]].status, &CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r1"]);

	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { table_id: Some(0), target: game.state.hands[Player::Alice as usize][0] });
}

#[test]
fn it_receives_a_reactive_play_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Bob,
		..TestOptions::default()
	});

	take_turn(&mut game, "Bob clues 4 to Alice (slot 3)");
	take_turn(&mut game, "Cathy plays g1, drawing y3");

	// Alice's slot 2 is called to play.
	assert_eq!(&game.meta[game.state.hands[Player::Alice as usize][1]].status, &CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 2, &["r1", "y1", "g2", "b1", "p1"]);
}

#[test]
fn it_doesnt_target_an_unclued_dupe() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[2, 0, 1, 0, 0]),
		init: Box::new(|game| {
			// Bob's r3 in slot 4 is clued.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues green to Bob");

	// We should discard slot 5 (so that Bob plays slot 2).
	assert_eq!(&game.meta[game.state.hands[Player::Alice as usize][4]].status, &CardStatus::CalledToDiscard);
}
