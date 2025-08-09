use rust_bot::basics::action::{PerformAction};
use rust_bot::basics::card::CardStatus;
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::ex_asserts;
use crate::util::{self, take_turn, Player, TestOptions};

#[test]
fn it_understands_a_ref_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "r3", "g5"],
		&["p4", "b5", "p2", "b1", "g4"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues green to Bob");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r1", "y1", "b1", "p1"]);
}

#[test]
fn it_understands_a_gapped_ref_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["p4", "b1", "p2", "b5", "g4"],
		&["b1", "g2", "r2", "r3", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues purple to Bob");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Bob, 2, &["r1", "y1", "g1", "b1"]);
}

#[test]
fn it_understands_a_chop_ref_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "b2", "p2", "b5", "g4"],
		&["b1", "g2", "r2", "r3", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues blue to Bob");
	ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["b1"]);
}

#[test]
fn it_understands_a_ref_discard() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["p4", "p2", "p2", "b5", "g3"],
		&["b1", "g2", "r2", "r3", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 4 to Bob");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][1]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_gives_a_ref_discard() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["p4", "p2", "p2", "b5", "g3"],
		&["b3", "g2", "r2", "r3", "g5"],
	], TestOptions::default());

	let perform = game.take_action();

	// Alice should clue 4 to Bob.
	assert_eq!(perform, PerformAction::Rank { table_id: Some(0), target: Player::Bob as usize, value: 4 });
}

#[test]
fn it_understands_a_lock() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["p4", "p2", "p2", "b5", "g4"],
		&["b1", "g2", "r2", "r3", "g5"],
	], TestOptions::default());

	take_turn(&mut game, "Alice clues 4 to Bob");

	assert!(game.common.thinks_locked(&game.frame(), Player::Bob as usize));
}
