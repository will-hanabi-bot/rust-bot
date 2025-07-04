

use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::util::{take_turn, TestOptions, Player};

pub mod util;
pub mod ex_asserts;

#[test]
fn it_doesnt_elim_when_giver_holds_dupes() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["g5", "b1", "g2", "r2"],
		&["r3", "y5", "r1", "r2"],
		&["g3", "p1", "b3", "b5"],
	], TestOptions {
		play_stacks: Some(vec![0, 2, 0, 2, 0]),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues 2 to Bob");
	take_turn(&mut game, "Bob clues 2 to Cathy");

	// Bob's cards could be r2, g2 or p2.
	ex_asserts::has_inferences(&game, None, Player::Bob, 3, &["r2", "g2", "p2"]);
	ex_asserts::has_inferences(&game, None, Player::Bob, 4, &["r2", "g2", "p2"]);

	// Cathy's card could be r2, g2 or p2.
	ex_asserts::has_inferences(&game, None, Player::Cathy, 4, &["r2", "g2", "p2"]);
}
