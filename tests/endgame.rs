use fraction::ConstOne;
use rust_bot::basics::action::PerformAction;
use std::sync::Arc;

use rust_bot::basics::{clue::ClueKind, endgame::EndgameSolver, game::Game};
use rust_bot::reactor::Reactor;

type Frac = fraction::Fraction;

use crate::util::{pre_clue, fully_known, Colour, Player, TestClue, TestOptions};

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
		play_stacks: Some(vec![4, 4, 5, 3, 5]),
		init: Box::new(|game: &mut Game| {
			// Alice has y5 in slot 1.
			pre_clue(game, Player::Alice, 1, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Bob },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Yellow as usize, giver: Player::Bob },
			]);

			// Bob has b4 in slot 1 and b5 in slot 4.
			pre_clue(game, Player::Bob, 1, &[
				TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Blue as usize, giver: Player::Alice },
			]);
			pre_clue(game, Player::Bob, 4, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Blue as usize, giver: Player::Alice },
			]);

			// Cathy has r5 in slot 4.
			pre_clue(game, Player::Cathy, 4, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice },
			]);

			// Donald has b4 in slot 1.
			pre_clue(game, Player::Donald, 1, &[
				TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Blue as usize, giver: Player::Alice },
			]);

			game.state.cards_left = 1;
		}),
		..TestOptions::default()
	});

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
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
		play_stacks: Some(vec![5, 3, 4, 5, 5]),
		init: Box::new(|game: &mut Game| {
			// Bob has g5 in slot 1 and y4 in slot 2.
			pre_clue(game, Player::Bob, 1, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Green as usize, giver: Player::Alice },
			]);
			pre_clue(game, Player::Bob, 2, &[
				TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Yellow as usize, giver: Player::Alice },
			]);

			// Donald has y4 in slot 1 and y5 in slot 4.
			pre_clue(game, Player::Donald, 1, &[
				TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Yellow as usize, giver: Player::Alice },
			]);
			pre_clue(game, Player::Donald, 4, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Yellow as usize, giver: Player::Alice },
			]);

			game.state.cards_left = 1;
		}),
		..TestOptions::default()
	});

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
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
		&["b2", "y1", "g1", "b5"],
		&["g1", "b1", "b1", "r1"],
		&["r1", "p1", "p1", "r5"],
	], TestOptions {
		play_stacks: Some(vec![3, 5, 5, 4, 5]),
		init: Box::new(|game: &mut Game| {
			// Alice has r4 in slots 1 and 2.
			for i in 1..=2 {
				pre_clue(game, Player::Alice, i, &[
					TestClue { kind: ClueKind::RANK, value: 4, giver: Player::Bob },
					TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Bob },
				]);
			}

			// Bob has b5 in slot 4.
			pre_clue(game, Player::Bob, 4, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Blue as usize, giver: Player::Alice },
			]);

			// Donald has r5 in slot 4.
			pre_clue(game, Player::Donald, 4, &[
				TestClue { kind: ClueKind::RANK, value: 5, giver: Player::Alice },
				TestClue { kind: ClueKind::COLOUR, value: Colour::Red as usize, giver: Player::Alice },
			]);

			game.state.cards_left = 1;
		}),
		..TestOptions::default()
	});

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
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
		play_stacks: Some(vec![5, 5, 5, 5, 2]),
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 1, "p3");

			fully_known(game, Player::Bob, 2, "p4");
			fully_known(game, Player::Bob, 4, "p4");

			fully_known(game, Player::Donald, 4, "p5");

			game.state.cards_left = 1;
		}),
		..TestOptions::default()
	});

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
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
		play_stacks: Some(vec![3, 5, 4, 5, 1]),
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 2, "p3");
			fully_known(game, Player::Alice, 3, "p4");
			fully_known(game, Player::Alice, 4, "r5");
			fully_known(game, Player::Alice, 5, "r4");

			fully_known(game, Player::Bob, 4, "p5");
			fully_known(game, Player::Bob, 5, "p2");

			fully_known(game, Player::Cathy, 5, "g5");

			game.state.cards_left = 4;
		}),
		..TestOptions::default()
	});

	// Alice plays r4 (3 left), Bob plays p2 (2 left), Cathy stalls
	// Alice plays p3 (1 left), Bob stalls, Cathy stalls
	// Alice plays p4 (0 left), Bob plays p5, Cathy plays g5
	// Alice plays r5.
	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
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
		play_stacks: Some(vec![2, 4, 5, 5, 5]),
		discarded: vec!["r3", "r4"],
		clue_tokens: 0,
		init: Box::new(|game: &mut Game| {
			fully_known(game, Player::Alice, 5, "r3");

			fully_known(game, Player::Bob, 5, "r4");

			fully_known(game, Player::Cathy, 5, "r5");

			game.state.cards_left = 2;
		}),
		..TestOptions::default()
	});

	match EndgameSolver::new().solve_game(&game, Player::Alice as usize) {
		Err(msg) => panic!("Game should be winnable! {}", msg),
		Ok((perform, winrate)) => {
			assert_eq!(winrate, Frac::new(1_u64, 6_u64));
			// Alice should play r3.
			assert_eq!(perform, PerformAction::Play { target: game.state.hands[Player::Alice as usize][4], table_id: Some(0) });
		}
	}
}
