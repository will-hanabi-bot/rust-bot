use rust_bot::basics::card::CardStatus;
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::util::{self, take_turn, Player, TestOptions};

#[test]
fn it_cancels_a_missed_reaction_1() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g1", "r1", "g4", "b4", "b4"],
		&["b1", "r3", "r4", "y4", "y4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 4 to Cathy");

	// Bob is called to play r1 (slot 2) -> Cathy plays b1 (slot 1).
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob discards g1, drawing y3");

	// Bob is no longer called to play r1, and that card can be anything.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::None);
	assert_eq!(game.common.thoughts[game.state.hands[Player::Bob as usize][1]].inferred.len(), game.common.thoughts[game.state.hands[Player::Bob as usize][1]].possible.len());

	// Cathy is not called to play slot 1 (Cathy might have some wrong priority elim notes).
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][0]].status, CardStatus::None);
}

#[test]
fn it_cancels_a_missed_reaction_2() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g1", "r1", "g4", "b4", "b4"],
		&["b1", "r1", "r4", "y4", "y4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 4 to Cathy");

	// Bob is called to play r1 (slot 2) -> Cathy plays b1 (slot 1).
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays g1, drawing y3");

	// Bob is no longer called to play r1, and that card can be anything.
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::None);
	assert_eq!(game.common.thoughts[game.state.hands[Player::Bob as usize][1]].inferred.len(), game.common.thoughts[game.state.hands[Player::Bob as usize][1]].possible.len());

	// Cathy is not called to play slot 1 (Cathy might have some wrong priority elim notes).
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][0]].status, CardStatus::None);
}
