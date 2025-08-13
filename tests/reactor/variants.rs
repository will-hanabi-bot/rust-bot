use rust_bot::basics::action::{PerformAction};
use rust_bot::basics::card::CardStatus;
use rust_bot::reactor::Reactor;
use std::sync::Arc;

use crate::ex_asserts;
use crate::util::{self, take_turn, Player, TestOptions};

#[test]
fn it_understands_a_playable_pink_promise() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "r4", "y4", "y4"],
		&["g4", "g1", "g4", "b4", "b4"],
	], TestOptions {
		play_stacks: Some(&[1, 2, 1, 1, 2]),
		variant: "Pink (5 Suits)",
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 2 to Alice (slots 2,4)");

	// Alice should play slot 2.
	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][1] });
	ex_asserts::has_inferences(&game, None, Player::Alice, 2, &["r2", "g2", "b2"]);

	// Alice's slot 4 is not playable.
	let playables = game.common.thinks_playables(&game.frame(), Player::Alice as usize);
	assert!(playables.len() == 1 && playables[0] == game.state.hands[Player::Alice as usize][1]);
}

#[test]
fn it_understands_a_brown_tcm() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "r4", "y4", "y4"],
		&["g4", "g1", "g4", "b4", "b4"],
	], TestOptions {
		play_stacks: Some(&[1, 2, 1, 1, 2]),
		variant: "Brown (5 Suits)",
		starting: Player::Cathy,
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 1 to Alice (slots 2,4)");

	// Alice does not have a playable.
	let playables = game.common.thinks_playables(&game.frame(), Player::Alice as usize);
	assert!(playables.is_empty());
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][0]].status, CardStatus::None);
}
