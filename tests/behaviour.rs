use rust_bot::basics::action::PerformAction;
use rust_bot::basics::clue::ClueKind;
use std::sync::Arc;

use rust_bot::basics::{game::Game};
use rust_bot::reactor::Reactor;

use crate::util::{fully_known, pre_clue, take_turn, Player, TestClue, TestOptions};

pub mod util;
pub mod ex_asserts;

#[test]
fn it_doesnt_discard_an_easily_winnable_endgame() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["g1", "p1", "r1", "b3", "y5"],
		&["r3", "b2", "p3", "y3", "y4"],
	], TestOptions {
		clue_tokens: 4,
		play_stacks: Some(&[5, 1, 5, 5, 5]),
		discarded: &[
			"r1", "r2",
			"y1", "y4",
			"g1", "g3",
			"b4",
			"p4"
		],
		starting: Player::Cathy,
		init: Box::new(|game: &mut Game| {
			// Bob has a clued 3. If we play our y2, Bob will likely bomb.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::RANK, value: 3, giver: Player::Alice }]);
			fully_known(game, Player::Bob, 5, "y5");

			fully_known(game, Player::Cathy, 4, "y3");
			fully_known(game, Player::Cathy, 5, "y4");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 6);

	take_turn(&mut game, "Cathy clues 1 to Alice (slots 2,3,4)");

	// We should not discard.
	let action = game.take_action();
	assert!(!matches!(action, PerformAction::Discard { .. }));
}
