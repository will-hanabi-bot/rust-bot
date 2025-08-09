use rust_bot::basics::action::{PerformAction};
use rust_bot::basics::card::CardStatus;
use rust_bot::basics::{clue::ClueKind};
use rust_bot::basics::game::{Game};
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::util::{self, pre_clue, take_turn, Player, TestClue, TestOptions};

#[test]
fn it_reacts_to_a_response_inversion() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g1", "y5", "g4", "b4", "b4"],
		&["b1", "r1", "r4", "y4", "y4"],
	], TestOptions {
		starting: Player::Cathy,
		// Alice has a clued 1 in slot 5.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Alice, 5, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Bob }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues green to Bob");

	// We are called to discard slot 2.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][1]].status, CardStatus::CalledToDiscard);

	let action = game.take_action();

	// We should discard slot 2 urgently.
	assert_eq!(action, PerformAction::Discard { table_id: Some(0), target: game.state.hands[Player::Alice as usize][1] });

	take_turn(&mut game, "Alice discards r4 (slot 2)");

	// Bob is called to play g1.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);
}

#[test]
fn it_receives_a_response_inversion() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g4", "y5", "g4", "b4", "b4"],
		&["y4", "r1", "r4", "y4", "y1"],
	], TestOptions {
		starting: Player::Bob,
		// Cathy has a clued 1 in slot 5.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Cathy, 5, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Bob clues green to Alice (slot 4)");

	// We are called to play slot 3.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][2]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Cathy plays r1, drawing r4");

	// We are called to discard slot 2.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][1]].status, CardStatus::CalledToDiscard);

	// Slot 3 is no longer called to play.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][2]].status, CardStatus::None);
}

#[test]
fn it_does_not_receive_a_declined_inversion_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g4", "y5", "g4", "b4", "b4"],
		&["y4", "r1", "r4", "y4", "y1"],
	], TestOptions {
		starting: Player::Bob,
		// Cathy has a clued 1 in slot 5.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Cathy, 5, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Bob clues green to Alice (slot 4)");

	// We are called to play slot 3.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][2]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Cathy plays y1, drawing r4");

	// We are not called to discard slot 4.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::None);

	// Slot 3 is still called to play.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][2]].status, CardStatus::CalledToPlay);
}

#[test]
fn it_does_not_receive_a_declined_inversion_discard() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g4", "y5", "g4", "b4", "b3"],
		&["y4", "r1", "r4", "y4", "y1"],
	], TestOptions::default());

	// Lock Bob
	take_turn(&mut game, "Alice clues 3 to Bob");

	take_turn(&mut game, "Bob clues 3 to Alice (slot 3)");

	// We are called to discard slot 4.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::CalledToDiscard);

	take_turn(&mut game, "Cathy discards y4 (slot 1) drawing r4");

	// We are not called to play slot 2.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][1]].status, CardStatus::None);

	// Slot 4 is still called to discard.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_understands_a_bad_lock() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g4", "y5", "g1", "b4", "b1"],
		&["y4", "r1", "b4", "y4", "y3"],
	], TestOptions {
		// Bob has a clued 1 in slot 5.
		init: Box::new(|game: &mut Game| {
			pre_clue(game, Player::Bob, 5, &[TestClue { kind: ClueKind::RANK, value: 1, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	// Blue is available to push Cathy's r1.
	take_turn(&mut game, "Alice clues 3 to Cathy");

	// Bob is called to play slot 3.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][2]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays g1, drawing r4");

	// Cathy is called to play slot 2.
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][1]].status, CardStatus::CalledToPlay);
	assert!(!game.common.thinks_locked(&game.frame(), Player::Cathy as usize));
}
