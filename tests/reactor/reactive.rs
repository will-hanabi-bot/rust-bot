use fraction::Fraction;
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

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r1", "y1", "b1", "p1"]);

	take_turn(&mut game, "Bob plays b1, drawing p1");

	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Cathy, 1, &["r1", "y1", "g1", "b2", "p1"]);
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

	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r1", "y1", "g1", "p1"]);

	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0] });
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
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][1]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 2, &["r1", "y1", "g2", "b1", "p1"]);
}

#[test]
fn it_elims_a_reactive_play_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "y3", "g5"],
		&["g3", "b5", "p2", "b1", "g4"],
	], TestOptions {
		init: Box::new(|game| {
			// Bob's r2 is clued with red.
			pre_clue(game, Player::Bob, 3, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);

			// Bob's y3 is clued with 3.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::RANK, value: 3, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues 4 to Cathy");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToPlay);

	take_turn(&mut game, "Bob plays b1, drawing p1");
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][3]].status, CardStatus::CalledToPlay);

	// Since Bob cannot play a known 3, Cathy can't write !playable on slot 1.
	assert!(["r1", "y1", "g1", "b1", "p1"].iter().all(|i| game.common.thoughts[game.state.hands[Player::Cathy as usize][0]].inferred.contains(game.state.expand_short(i))));

	// Since Bob cannot play r1 onto r1, Cathy can't write !r1 on slot 2.
	assert!(["y1", "g1", "b1", "p1"].iter().all(|i| !game.common.thoughts[game.state.hands[Player::Cathy as usize][1]].inferred.contains(game.state.expand_short(i))));
	assert!(game.common.thoughts[game.state.hands[Player::Cathy as usize][1]].inferred.contains(game.state.expand_short("r1")));

	// Bob can play slot 2, so Cathy can write !playable on slot 3.
	assert!(["r1", "y1", "g1", "b1", "p1"].iter().all(|i| !game.common.thoughts[game.state.hands[Player::Cathy as usize][2]].inferred.contains(game.state.expand_short(i))));
}

#[test]
fn it_understands_a_reactive_dc_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		clue_tokens: Fraction::from(7),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues blue to Cathy");

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToDiscard);
	// ex_asserts::has_inferences(&game, None, Player::Bob, 1, &["r1", "y1", "b1", "p1"]);
	assert!(game.common.thinks_trash(&game.frame(), Player::Bob as usize).contains(&game.state.hands[Player::Bob as usize][0]));

	take_turn(&mut game, "Bob discards r3 (slot 1), drawing p3");

	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Cathy, 1, &["r1", "y1", "g1", "p1"]);
}

#[test]
fn it_elims_a_reactive_dc_play() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "g5", "y3"],
		&["b3", "b5", "p2", "b1", "g4"],
	], TestOptions {
		play_stacks: Some(&[1, 1, 1, 0, 0]),
		clue_tokens: Fraction::from(7),
		init: Box::new(|game| {
			// Bob's r2 is clued with red.
			pre_clue(game, Player::Bob, 3, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);

			// Bob's g5 is clued with 5.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Alice clues green to Cathy");
	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToDiscard);

	take_turn(&mut game, "Bob discards b1, drawing b4");
	assert_eq!(game.meta[game.state.hands[Player::Cathy as usize][3]].status, CardStatus::CalledToPlay);

	// Since Bob cannot discard a known 5, Cathy can't write !playable on slot 1.
	assert!(["r1", "r2", "y1", "b1"].iter().all(|i| game.common.thoughts[game.state.hands[Player::Cathy as usize][0]].inferred.contains(game.state.expand_short(i))));

	// Bob can discard the other slots, so Cathy can write !playable on slots 2 and 3.
	assert!(["r2", "y2", "g2", "b1", "p1"].iter().all(|i| !game.common.thoughts[game.state.hands[Player::Cathy as usize][1]].inferred.contains(game.state.expand_short(i))));
	assert!(["r2", "y2", "g2", "b1", "p1"].iter().all(|i| !game.common.thoughts[game.state.hands[Player::Cathy as usize][2]].inferred.contains(game.state.expand_short(i))));
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
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r1"]);

	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0] });
}

#[test]
fn it_doesnt_play_target_an_unclued_dupe() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[2, 0, 1, 0, 0]),
		clue_tokens: Fraction::from(7),
		init: Box::new(|game| {
			// Bob's r3 in slot 4 is clued.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues green to Bob");

	// We should discard slot 5 (so that Bob plays slot 2).
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][4]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_doesnt_play_target_a_discarding_dupe() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "g2", "y3", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[1, 0, 0, 0, 0]),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues red to Bob");
	take_turn(&mut game, "Alice plays r2 (slot 3)");	// Targeting discard on r3

	assert_eq!(game.meta[game.state.hands[Player::Bob as usize][0]].status, CardStatus::CalledToDiscard);

	take_turn(&mut game, "Bob clues 1 to Cathy");
	take_turn(&mut game, "Cathy clues yellow to Bob");

	// We should discard slot 4 (so that Bob plays the non-discarding dupe in slot 4).
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::CalledToDiscard);
}

#[test]
fn it_doesnt_dc_target_an_unclued_dupe() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r3", "y1", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[0, 1, 0, 0, 0]),
		init: Box::new(|game| {
			// Bob's r3 in slot 4 is clued.
			pre_clue(game, Player::Bob, 4, &[TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues green to Bob");

	// We should play slot 4 (so that Bob discards slot 1).
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][3]].status, CardStatus::CalledToPlay);
}

#[test]
fn it_reacts_to_a_sacrifice() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["r4", "b2", "y3", "p3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		play_stacks: Some(&[2, 0, 0, 0, 0]),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues green to Bob");

	// We should play slot 2 (Bob discards y3 in slot 3).
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][1]].status, CardStatus::CalledToPlay);
}

#[test]
fn it_shifts_a_reaction() {
	let mut game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "g2", "r2", "r3", "g5"],
		&["g1", "b5", "p2", "b1", "g4"],
	], TestOptions {
		starting: Player::Cathy,
		init: Box::new(|game| {
			// Alice has a clued 5 in slot 3.
			pre_clue(game, Player::Alice, 3, &[TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Cathy }]);
		}),
		..TestOptions::default()
	});

	take_turn(&mut game, "Cathy clues 3 to Bob");

	// Normally, Alice would play slot 3 -> Bob slot 1 = 4. However, slot 3 is a known 5.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][2]].status, CardStatus::None);

	// Instead, Alice should play slot 1 -> Bob slot 3 as a finesse.
	assert_eq!(game.meta[game.state.hands[Player::Alice as usize][0]].status, CardStatus::CalledToPlay);
	ex_asserts::has_inferences(&game, None, Player::Alice, 1, &["r1"]);

	let action = game.take_action();
	assert_eq!(action, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0] });

	take_turn(&mut game, "Alice plays r1 (slot 1)");

	// Bob's slot 1 should still be allowed to be b1.
	assert!(game.common.thoughts[game.state.hands[Player::Bob as usize][0]].inferred.contains(game.state.expand_short("b1")));
}
