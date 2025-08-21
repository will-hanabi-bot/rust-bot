use std::collections::{HashMap};

use rust_bot::basics;
use rust_bot::basics::action::{Action, ClueAction, DiscardAction, DrawAction, PlayAction, TurnAction};
use rust_bot::basics::clue::{BaseClue, CardClue, ClueKind};
use rust_bot::basics::card::{Identifiable, Identity};
use rust_bot::basics::game::{Convention, Game};
use rust_bot::basics::identity_set::IdentitySet;
use rust_bot::basics::util::visible_find;
use rust_bot::basics::state::State;
use rust_bot::basics::variant::{all_ids, id_touched, Variant};
use std::sync::{Arc, LazyLock};

pub enum Colour {
	Red,Yellow,Green,Blue,Purple
}

#[derive(Clone, Copy, PartialEq)]
pub enum Player {
	Alice,Bob,Cathy,Donald,Emily
}

static VARIANTS: LazyLock<HashMap<&str, Variant>> = LazyLock::new(|| {
    HashMap::from([
        ("No Variant", Variant::new(0, "No Variant", &["Red", "Yellow", "Green", "Blue", "Purple"], &["r", "y", "g", "b", "p"])),
        ("6 Suits", Variant::new(0, "6 Suits", &["Red", "Yellow", "Green", "Blue", "Purple", "Teal"], &["r", "y", "g", "b", "p", "t"])),
        ("Rainbow (5 Suits)", Variant::new(16, "Rainbow", &["Red", "Yellow", "Green", "Blue", "Rainbow"], &["r", "y", "g", "b", "m"])),
        ("Black (5 Suits)", Variant::new(2, "Black", &["Red", "Yellow", "Green", "Blue", "Black"], &["r", "y", "g", "b", "k"])),
        ("Pink (5 Suits)", Variant::new(2, "Pink", &["Red", "Yellow", "Green", "Blue", "Pink"], &["r", "y", "g", "b", "i"])),
        ("Brown (5 Suits)", Variant::new(2, "Brown", &["Red", "Yellow", "Green", "Blue", "Brown"], &["r", "y", "g", "b", "n"])),
    ])
});

static NAMES: [&str; 5] = ["Alice", "Bob", "Cathy", "Donald", "Emily"];

pub struct TestOptions<'a> {
	pub min_level: u8,
	pub max_level: u8,
	pub play_stacks: Option<&'a[usize]>,
	pub discarded: &'a[&'a str],
	pub strikes: u8,
	pub clue_tokens: usize,
	pub starting: Player,
	pub variant: &'a str,
	pub init: Box<dyn Fn(&mut Game)>,
}

impl<'a> Default for TestOptions<'a> {
	fn default() -> Self {
		Self {
			min_level: 1,
			max_level: 100,
			play_stacks: None,
			discarded: &[],
			strikes: 0,
			clue_tokens: 8,
			starting: Player::Alice,
			variant: "No Variant",
			init: Box::new(|_| {}),
		}
	}
}

pub fn setup(convention: Arc<dyn Convention + Send + Sync + 'static>, hands: &[&[&str]], test_options: TestOptions) -> Game {
	let _ = rust_bot::logger::init();

	let player_names = NAMES[..hands.len()].iter().map(|&name| name.to_string()).collect();
    let state = State::new(player_names, 0, Arc::new(VARIANTS.get(test_options.variant).unwrap().clone()));
    let mut game = Game::new(0, state, false, convention);
    game.catchup = true;

    let Game { common, players, state, .. } = &mut game;

    match &test_options.play_stacks {
		None => {
			state.play_stacks = vec![0; state.variant.suits.len()];
		}
		Some(stacks) => {
			if stacks.len() != state.variant.suits.len() {
				panic!("Invalid play stacks length");
			}
			state.play_stacks = stacks.to_vec();
		}
	}

	for player in players {
		player.hypo_stacks = state.play_stacks.clone();
	}
	common.hypo_stacks = state.play_stacks.clone();

    let mut order_counter = 0;

    // Draw all the hands
    for (player_index, hand) in hands.iter().enumerate() {
	    for &short in hand.iter().rev() {
			let action = Action::Draw(if short == "xx" {
				DrawAction {
					order: order_counter,
					suit_index: -1,
					rank: -1,
					player_index,
				}
			} else {
				let Identity { suit_index, rank } = game.state.expand_short(short);
				DrawAction {
					order: order_counter,
					suit_index: suit_index as i32,
					rank: rank as i32,
					player_index,
				}
			});

	    	game.handle_action(&action);
	    	order_counter += 1;
    	}
    }

    let Game { players, state, .. } = &mut game;

	for short in test_options.discarded {
		let id = state.expand_short(short);
		let Identity { suit_index, rank } = id;
		state.discard_stacks[suit_index][rank - 1].push(99);

		if state.discard_stacks[suit_index][rank - 1].len() > state.card_count(id) {
			state.max_ranks[suit_index] = std::cmp::min(state.max_ranks[suit_index], rank - 1);
		}
	}

	for id in all_ids(&state.variant) {
		let count = state.base_count(id) + visible_find(state, &players[state.our_player_index], id, Default::default(), |_, _| true).len();

		if count > state.card_count(id) {
			panic!("Found {count} copies of {}!", state.log_id(id));
		}
	}

	state.cards_left -= state.score() + test_options.discarded.len();

	state.current_player_index = test_options.starting as usize;
	state.clue_tokens = test_options.clue_tokens;
	state.strikes = test_options.strikes;

    // Apply init hook, overwrite base state
    (test_options.init)(&mut game);

    basics::elim(&mut game, true);
    game.base = Arc::new((game.state.clone(), game.meta.clone(), game.players.clone(), game.common.clone()));

    game
}

pub fn take_turn(game: &mut Game, raw_action: &str) {
	let Game { state, .. } = game;
	let (action, draw) = parse_action(state, raw_action);
	let turn_taker = match action {
		Action::Clue(ClueAction { giver, .. }) => {
			if state.clue_tokens == 0 {
				panic!("Tried to clue with 0 clue tokens");
			}
			giver
		},
		Action::Play(PlayAction { player_index, .. }) => player_index,
		Action::Discard(DiscardAction { player_index, .. }) => {
			if state.clue_tokens == 8 {
				panic!("Tried to discard with 8 clue tokens");
			}
			player_index
		},
		_ => state.current_player_index
	};

	if turn_taker != state.current_player_index {
		panic!("Expected {}'s turn for action ({})!", state.player_names[turn_taker], action.fmt(state));
	}

	game.catchup = true;
	game.handle_action(&action);

	let Game { state, .. } = &game;

	match action {
		Action::Play(PlayAction { .. }) | Action::Discard(DiscardAction { .. }) => {
			match draw {
				Some(draw) => {
					if state.cards_left == 0 {
						panic!("Cannot draw at 0 cards left");
					}

					let Identity { suit_index, rank } = draw;
					let count = state.base_count(draw) + visible_find(state, game.me(), draw, Default::default(), |_, _| true).len();

					if count + 1 > state.card_count(draw) {
						panic!("Found {} copies of {}!", count + 1, state.log_id(draw));
					}
					game.handle_action(&Action::Draw(DrawAction { player_index: turn_taker, order: state.card_order, suit_index: suit_index as i32, rank: rank as i32 }))
				},
				None => {
					if turn_taker != state.our_player_index {
						panic!("Missing draw for {}'s action {:?}", state.player_names[turn_taker], action);
					}
					game.handle_action(&Action::Draw(DrawAction { player_index: turn_taker, order: state.card_order, suit_index: -1, rank: -1 }))
				}
			}
		}
		_ => {
			if draw.is_some() {
				panic!("Unexpected draw for action {action:?}");
			}
		}
	}

	let Game { state, .. } = &game;

	game.handle_action(&Action::Turn(TurnAction {
		num: state.turn_count,
		current_player_index: state.next_player_index(turn_taker) as i32,
	}));

	game.catchup = false;

}

fn parse_slots(state: &State, parts: &Vec<&str>, parts_index: usize, expect_one: bool, insufficient_msg: &str) -> Vec<usize> {

	if parts.len() < parts_index + 1 || !parts[parts_index - 1].contains("slot") {
		panic!("Not enough arguments provided {} in '{}', needs '(slot x)'", insufficient_msg, parts.join(" "));
	}

	let original = format!("{} {}", parts[parts_index - 1], parts[parts_index]);

	let slots: Vec<usize> = parts[parts_index].trim_end_matches(|c: char| !c.is_numeric()).split(",").map(|s| s.parse().unwrap()).collect();
	if slots.is_empty() || slots.iter().any(|&slot| slot < 1 || slot > state.our_hand().len()) {
		panic!("Failed to parse '{}'", parts.join(" "));
	}

	if expect_one && slots.len() != 1 {
		panic!("Expected one slot, got {} in '{}'", slots.len(), original);
	}
	slots
}

fn parse_action(state: &State, action: &str) -> (Action, Option<Identity>) {
	let parts = action.split_whitespace().collect::<Vec<&str>>();

	let player_name = parts[0];
	let player_index = state.player_names.iter().position(|name| name == player_name)
		.unwrap_or_else(|| panic!("Couldn't parse giver {player_name}, not in list of players {:?}", state.player_names));

	match parts[1] {
		"clues" => {
			let clue = if "12345".contains(parts[2]) {
				BaseClue { kind: ClueKind::RANK, value: parts[2].parse().unwrap() }
			}
			else {
				BaseClue { kind: ClueKind::COLOUR, value: state.variant.suits.iter().position(|suit| suit.to_lowercase() == parts[2].to_lowercase())
					.unwrap_or_else(|| panic!("Couldn't parse colour {}", parts[2])) }
			};
			let target_name = parts[4];
			let target = state.player_names.iter().position(|name| name == target_name)
				.unwrap_or_else(|| panic!("Couldn't parse target {target_name}, not in list of players {:?}", state.player_names));

			if target != state.our_player_index {
				let list = state.clue_touched(&state.hands[target], &clue);

				if list.is_empty() {
					panic!("No cards touched by clue {:?} to {}", clue, state.player_names[target]);
				}
				(Action::Clue(ClueAction { clue, giver: player_index, target, list }), None)
			}
			else {
				let slots = parse_slots(state, &parts, 6, false, "(clue to us)");
				let list = slots.iter().map(|slot| state.our_hand()[slot - 1]).collect();
				(Action::Clue(ClueAction { clue, giver: player_index, target, list }), None)
			}
		},
		"plays" => {
			// Bob plays r5 (slot 1), drawing r1
			let id = state.expand_short(parts[2]);
			let Identity { suit_index, rank } = id;

			if player_index != state.our_player_index {
				let matching = state.hands[player_index].iter().filter(|&&o| state.deck[o].is(&id)).collect::<Vec<_>>();
				let draw = (parts.len() >= 5 && parts[parts.len() - 2] == "drawing").then(|| state.expand_short(parts[parts.len() - 1]));

				if matching.is_empty() {
					panic!("Unable to find card {} to play in {}'s hand", parts[2], player_name);
				}
				else if matching.len() == 1 {
					// Brief check to make sure that if slot provided, it is correct
					if parts.len() > 4 && parts[3].contains("slot") {
						let slot = parse_slots(state, &parts, 4, true, "")[0];
						if &state.hands[player_index][slot - 1] != matching[0] {
							panic!("Identity {} not in slot {}", parts[2], slot);
						}
					}
					(Action::Play(PlayAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order: *matching[0]}), draw)
				}
				else {
					let slot = parse_slots(state, &parts, 4, true, "(ambiguous identity)")[0];
					let order = state.hands[player_index][slot - 1];

					if !state.deck[order].is(&id) {
						panic!("Identity {} not in slot {}", parts[2], slot);
					}

					(Action::Play(PlayAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order }), draw)
				}
			}
			else {
				let slot = parse_slots(state, &parts, 4, true, "(play from us)")[0];
				let order = state.hands[player_index][slot - 1];

				(Action::Play(PlayAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order }), None)
			}
		},
		"discards" | "bombs" => {
			let id = state.expand_short(parts[2]);
			let Identity { suit_index, rank } = id;
			let failed = parts[1] == "bombs";

			if player_index != state.our_player_index {
				let draw = (parts.len() >= 5 && parts[parts.len() - 2] == "drawing").then(|| state.expand_short(parts[parts.len() - 1]));
				let matching = state.hands[player_index].iter().filter(|&&o| state.deck[o].is(&id)).collect::<Vec<_>>();
				if matching.is_empty() {
					panic!("Unable to find card {} to discard in {}'s hand", parts[2], player_name);
				}
				else if matching.len() == 1 {
					(Action::Discard(DiscardAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order: *matching[0], failed}), draw)
				}
				else {
					let slot = parse_slots(state, &parts, 4, true, "(ambiguous identity)")[0];
					let order = state.hands[player_index][slot - 1];

					if !state.deck[order].is(&id) {
						panic!("Identity {} not in slot {}", parts[2], slot);
					}

					(Action::Discard(DiscardAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order, failed}), draw)
				}
			}
			else {
				let slot = parse_slots(state, &parts, 4, true, "(discard from us)")[0];
				let order = state.hands[player_index][slot - 1];

				(Action::Discard(DiscardAction { player_index, suit_index: suit_index as i32, rank: rank as i32, order, failed }), None)
			}
		}
		_ => {
			panic!("Unknown action {}", parts[1]);
		}
	}
}

pub struct TestClue {
	pub kind: ClueKind,
	pub value: usize,
	pub giver: Player
}

impl TestClue {
	pub fn base(&self) -> BaseClue {
		BaseClue { kind: self.kind, value: self.value }
	}
}

pub fn pre_clue(game: &mut Game, player_index: Player, slot: usize, clues: &[TestClue]) {
	let Game { state, common, .. } = game;
	let order = state.hands[player_index as usize][slot - 1];

	if let Some(id) = state.deck[order].id() {
		let non_touching = clues.iter().find(|clue| !id_touched(id, &state.variant, &clue.base()));

		if let Some(clue) = non_touching {
			panic!("Clue {} doesn't touch order {order}!", clue.base().fmt(state, player_index as usize))
		}
	}

	let possibilities = IdentitySet::from_iter(all_ids(&state.variant).filter(|i| clues.iter().all(|clue| id_touched(*i, &state.variant, &clue.base()))));

	let thought = &mut common.thoughts[order];
	thought.inferred = possibilities;
	thought.possible = possibilities;

	state.deck[order].clued = true;
	state.deck[order].clues = clues.iter().map(|&TestClue { kind, value, giver }|
		CardClue { kind, value, giver: giver as usize, turn: 0 }
	).collect();
}

/**
 * Pre-clues the slot with both colour and rank (only works for simple variants).
 */
pub fn fully_known(game: &mut Game, player_index: Player, slot: usize, short: &str) {
	let Game { state, .. } = game;
	let card = &state.deck[state.hands[player_index as usize][slot - 1]];
	let id = state.expand_short(short);

	if let Some(deck_id) = card.id() {
		if deck_id != id {
			panic!("{}'s card at slot {} is not {}! found {}", state.player_names[player_index as usize], slot, state.log_id(id), state.log_id(deck_id));
		}
	}

	let giver = if player_index == Player::Alice { Player::Bob } else { Player::Alice };

	pre_clue(game, player_index, slot, &[
		TestClue { kind: ClueKind::RANK, value: id.rank, giver },
		TestClue { kind: ClueKind::COLOUR, value: id.suit_index, giver },
	]);
}
