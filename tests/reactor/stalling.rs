use rust_bot::basics::card::CardStatus;
use rust_bot::reactor::{Reactor};
use std::sync::Arc;

use crate::util::{self, take_turn, Player, TestOptions};

#[test]
fn it_understands_a_bad_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "r4", "y4", "y4", "g4"],
		&["g1", "p4", "p4", "b4", "g4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues blue to Cathy");

	// Bob's slot 3 should be called to discard, as p4 is not playable.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][2]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_doesnt_react_to_a_cathy_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "r4", "y4", "y4", "g4"],
		&["g4", "p4", "p1", "b4", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues blue to Cathy");

	// Bob's slot 1 should not called to discard, as this is an allowable play clue on turn 1.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::None);
}

#[test]
fn it_reacts_to_cathy_1s() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "r4", "y4", "y4", "g1"],
		&["g4", "p4", "p1", "b1", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 1 to Cathy");

	// Bob's slot 5 is called to play, since colour can be given to Cathy.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][4]].status, CardStatus::CalledToPlay);
}

#[test]
fn it_doesnt_react_to_untargetable_cathy_1s() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "r4", "y4", "y4", "g1"],
		&["g4", "p4", "p1", "p3", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 1 to Cathy");

	// Bob's slot 5 is not called to play, since colour can't given to Cathy.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][4]].status, CardStatus::None);
}
