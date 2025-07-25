use fraction::ConstOne;
use rust_bot::basics::action::PerformAction;
use std::sync::Arc;

use rust_bot::basics::{endgame::EndgameSolver, game::Game};
use rust_bot::reactor::Reactor;

type Frac = fraction::Fraction;

use crate::util::{fully_known, Player, TestOptions};

pub mod util;
pub mod ex_asserts;

#[test]
fn it_clues_to_start_b45_endgame() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["b4", "y1", "g1", "b5"],
		&["g1", "b1", "b1", "r5"],
		&["b4", "p1", "p1", "r1"],
	], TestOptions {
		play_stacks: Some(&[4, 4, 5, 3, 5]),
		discarded: &[
			"r2", "r3",
			"y2", "y3",
			"g2", "g3", "g4",
			"b2", "b3",
			"p2", "p3", "p4"
		],	// Missing: r1, y1, r4, y4
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 1, "y5");

			fully_known(game, Player::Bob, 1, "b4");
			fully_known(game, Player::Bob, 4, "b5");

			fully_known(game, Player::Cathy, 4, "r5");

			fully_known(game, Player::Donald, 1, "b4");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 1);

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::ONE);
			assert!(perform.is_clue());
		}
	}
}

#[test]
fn it_clues_to_start_endgame_on_a_double_player() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["g5", "y4", "g1", "r1"],
		&["g1", "b1", "b1", "r1"],
		&["y4", "p1", "p1", "y5"],
	], TestOptions {
		play_stacks: Some(&[5, 3, 4, 5, 5]),
		discarded: &[
			"r2", "r3",
			"y2", "y3",
			"g2", "g3",
			"b2", "b3",
			"p2", "p3", "p4"
		],	// Missing: y1, y1, r4, g4, b4
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Bob, 1, "g5");
			fully_known(game, Player::Bob, 2, "y4");

			fully_known(game, Player::Donald, 1, "y4");
			fully_known(game, Player::Donald, 4, "y5");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 1);

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::ONE);
			assert!(perform.is_clue());
		}
	}
}

#[test]
fn it_plays_to_start_simple_endgame() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["r1", "r1", "y1", "b5"],
		&["g1", "g1", "y1", "p1"],
		&["b1", "b1", "p1", "r5"],
	], TestOptions {
		play_stacks: Some(&[3, 5, 5, 4, 5]),
		discarded: &[
				  "r3",
				  "y3", "y4",
				  "g3", "g4",
			"b2", "b3", "b4",
			"p2", "p3",	"p4"
		],	// Missing: r2, y2, b2
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 1, "r4");
			fully_known(game, Player::Alice, 2, "r4");

			fully_known(game, Player::Bob, 4, "b5");

			fully_known(game, Player::Donald, 4, "r5");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 1);

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::ONE);
			assert_eq!(perform, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0], table_id: Some(0) });
		}
	}
}

#[test]
fn it_plays_to_start_endgame_when_other_has_dupes() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx"],
		&["b1", "p4", "b1", "p4"],
		&["r1", "y1", "g1", "p1"],
		&["r1", "y1", "g1", "p5"],
	], TestOptions {
		play_stacks: Some(&[5, 5, 5, 5, 2]),
		discarded: &[
			"r2", "r3",
			"y2", "y3",
			"g2", "g3",
			"b2", "b3", "b4",
			"p2", "p3",
		],	// Missing: p1, r4, y4, g4
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 1, "p3");

			fully_known(game, Player::Bob, 2, "p4");
			fully_known(game, Player::Bob, 4, "p4");

			fully_known(game, Player::Donald, 4, "p5");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 1);

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::ONE);
			// Alice should play p3.
			assert_eq!(perform, PerformAction::Play { target: game.state.hands[Player::Alice as usize][0], table_id: Some(0) });
		}
	}
}

#[test]
fn it_plays_to_start_a_complex_endgame_where_all_cards_are_seen() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "g1", "p5", "p2"],
		&["g1", "b1", "r4", "r1", "g5"],
	], TestOptions {
		play_stacks: Some(&[3, 5, 4, 5, 1]),
		discarded: &[
			"r2", "r3",
			"y2", "y3", "y4",
			"g2", "g3", "g4",
			"b2", "b3", "b4",
			"p2", "p3"
		],	// Missing: y1, y1, p1, p1, p4
		init: Box::new(|game: &mut Game| {
			// fully_known(game, Player::Alice, 1, "p1");
			fully_known(game, Player::Alice, 2, "p3");
			fully_known(game, Player::Alice, 3, "p4");
			fully_known(game, Player::Alice, 4, "r5");
			fully_known(game, Player::Alice, 5, "r4");

			fully_known(game, Player::Bob, 4, "p5");
			fully_known(game, Player::Bob, 5, "p2");

			fully_known(game, Player::Cathy, 5, "g5");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 4);

	// Alice plays r4 (3 left), Bob plays p2 (2 left), Cathy stalls
	// Alice plays p3 (1 left), Bob stalls, Cathy stalls
	// Alice plays p4 (0 left), Bob plays p5, Cathy plays g5
	// Alice plays r5.
	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::ONE);
			// Alice should play r4.
			assert_eq!(perform, PerformAction::Play { target: game.state.hands[Player::Alice as usize][4], table_id: Some(0) });
		}
	}
}

#[test]
fn it_calculates_basic_winrate_correctly() {
	let game = util::setup(Arc::new(Reactor), &[
		&["xx", "xx", "xx", "xx", "xx"],
		&["b1", "r1", "g1", "y1", "r4"],
		&["b1", "r1", "g1", "y1", "r5"],
	], TestOptions {
		play_stacks: Some(&[2, 4, 5, 5, 5]),
		discarded: &[
			"r2", "r3",
			"y2", "y3",
			"g2", "g3",
			"b2", "b3", "b4",
			"p2", "p3", "p4"
		],	// Missing: p1, p1, r4, y4, g4, y5
		clue_tokens: 0,
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 5, "r3");

			fully_known(game, Player::Bob, 5, "r4");

			fully_known(game, Player::Cathy, 5, "r5");
		}),
		..TestOptions::default()
	});

	assert_eq!(game.state.cards_left, 2);

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {msg}"),
		Ok((perform, winrate)) => {
			// We win if Bob draws y5, and lose if Bob doesn't. There are 6 locations that y5 could be.
			assert_eq!(winrate, Frac::new(1_u64, 6_u64));
			// Alice should play r3.
			assert_eq!(perform, PerformAction::Play { target: game.state.hands[Player::Alice as usize][4], table_id: Some(0) });
		}
	}
}
