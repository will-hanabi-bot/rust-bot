use rust_bot::basics::{clue::ClueKind};
use rust_bot::basics::game::Game;
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::util::{pre_clue, take_turn, TestClue, TestOptions, Colour, Player};

pub mod util;
pub mod ex_asserts;

#[test]
#[should_panic]
fn it_fails_impossible_setup() {
	util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r1", "r1", "r1", "r1", "r1"],
	], TestOptions::default());
}

#[test]
fn it_elims_from_count() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r1", "r1", "r1", "r2", "r2"],
	], TestOptions { starting: Player::Bob, ..TestOptions::default() });

	take_turn(&mut game, "Bob clues red to Alice (slot 5)");

	ex_asserts::has_possible(&game, Some(Player::Alice), Player::Alice, 5, &["r3","r4","r5"]);
}

#[test]
fn it_visibly_elims_5s() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["g2", "b1", "r1", "g5"],
		&["g3", "p1", "b3", "b5"],
		&["r3", "b2", "r1", "y5"],
	], TestOptions {
		starting: Player::Donald,
		play_stacks: Some(vec![5, 0, 0, 0, 0, 0]),
		variant: "6 Suits",
		..TestOptions::default() });

	take_turn(&mut game, "Donald clues green to Alice (slot 1)");
	take_turn(&mut game, "Alice clues 5 to Bob");
	take_turn(&mut game, "Bob clues 5 to Cathy");
	take_turn(&mut game, "Cathy clues 5 to Donald");
	take_turn(&mut game, "Donald clues 5 to Alice (slots 3,4)");

	ex_asserts::has_possible(&game, None, Player::Alice, 3, &["p5","t5"]);
	ex_asserts::has_possible(&game, None, Player::Alice, 4, &["p5","t5"]);
	ex_asserts::has_possible(&game, None, Player::Bob, 4, &["g5"]);
	ex_asserts::has_possible(&game, None, Player::Cathy, 4, &["b5"]);
	ex_asserts::has_possible(&game, None, Player::Donald, 4, &["y5"]);
}

#[test]
fn it_visibly_elims_mixed_cards() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g2", "b1", "r4", "r5", "y3"],
		&["y5", "p1", "b3", "b5", "g3"],
	], TestOptions {
		starting: Player::Donald,
		play_stacks: Some(vec![3, 0, 0, 0, 0]),
		discarded: vec!["r1", "r1", "r2", "r3"],
		init: Box::new(|game: &mut Game| {
			// Alice's slot 5 is clued red.
			pre_clue(game, Player::Alice, 5, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Cathy }]);

			// Bob's slots 3 and 4 are clued red.
			pre_clue(game, Player::Bob, 3, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Cathy }]);
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Cathy }]);
		}),
		..TestOptions::default() });

	// Everyone knows that ALice's card is known r4.
	ex_asserts::has_possible(&game, None, Player::Alice, 5, &["r4"]);

	// Bob's cards could be r4 or r5.
	ex_asserts::has_possible(&game, None, Player::Bob, 3, &["r4", "r5"]);
	ex_asserts::has_possible(&game, None, Player::Bob, 4, &["r4", "r5"]);
}
