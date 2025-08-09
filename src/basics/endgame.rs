use colored::Colorize;
use fraction::{ConstOne, ConstZero, GenericFraction};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use itertools::Itertools;
use log::info;

use crate::basics::action::{Action, PerformAction};
use crate::basics::card::{CardStatus, IdOptions, Identifiable, Identity, MatchOptions};
use crate::basics::game::Game;
use crate::basics::player::Link;
use crate::basics::util::{self, FastMap};
use crate::basics::variant::{all_ids};
use winnable::SimpleResult;

type Frac = fraction::Fraction;
type RemainingMap = HashMap<Identity,RemainingEntry>;
mod winnable;

type WinnableResult = Result<(Vec<PerformAction>, Frac), &'static str>;
const UNWINNABLE: WinnableResult = Err("");
const TIMEOUT: WinnableResult = Err("timeout");

pub fn remove_remaining(remaining: &RemainingMap, id: Identity) -> RemainingMap {
	let RemainingEntry { missing, .. } = &remaining[&id];
	let mut new_remaining = remaining.clone();

	if *missing == 1 {
		new_remaining.remove(&id);
	} else {
		new_remaining.get_mut(&id).unwrap().missing -= 1;
	}
	new_remaining
}

#[derive(Clone)]
struct GameArr {
	game: Game,
	prob: Frac,
	remaining: RemainingMap,
	drew: Option<Identity>,
}

#[derive(Default)]
pub struct EndgameSolver {
	simple_cache: FastMap<WinnableResult>,
	simpler_cache: FastMap<bool>,
	if_cache: HashMap<String, SimpleResult>,
	success_rate: Vec<HashMap<PerformAction, (Frac, usize)>>,
	monte_carlo: bool,
}

impl EndgameSolver {
	pub fn new(monte_carlo: bool) -> Self {
		EndgameSolver { simple_cache: FastMap::default(), simpler_cache: FastMap::default(), if_cache: HashMap::new(), success_rate: Vec::new(), monte_carlo }
	}

	pub fn solve_game(&mut self, game: &Game) -> Result<(PerformAction, Frac), String> {
		let deadline = Instant::now() + Duration::from_secs(1);
		let (remaining_ids, own_ids) = find_remaining_ids(game);

		if remaining_ids.iter().filter(|(id, v)| !game.state.is_basic_trash(**id) && v.all).count() > 2 {
			return Err(format!("couldn't find any {}!", remaining_ids.keys().filter_map(|i|
				(!game.state.is_basic_trash(*i)).then_some(game.state.log_id(*i))).join(",")));
		}

		let level = log::max_level();
		log::set_max_level(log::LevelFilter::Off);

		let mut state = game.state.clone();
		let mut unknown_own = Vec::new();
		let linked_orders = game.me().linked_orders(&state);

		for (order, id) in &own_ids {
			if let Some(id) = id {
				state.deck[*order].base = Some(*id);
			}
			else {
				unknown_own.push(order);
			}
		}

		let total_unknown = state.cards_left + unknown_own.len();
		info!("unknown_own {:?}, cards left {}", unknown_own, state.cards_left);

		if total_unknown == 0 {
			let mut hypo_game = game.clone();
			hypo_game.state = state;

			for (order, id) in &own_ids {
				if let Some(id) = id {
					hypo_game.deck_ids[*order] = Some(*id);
				}
			}

			match self.winnable(&hypo_game, game.state.our_player_index, &remaining_ids, 0, &deadline) {
				Err(_) => {
					log::set_max_level(level);
					return Err("couldn't find a winning strategy.".to_owned());
				},
				Ok((actions, winrate)) => {
					log::set_max_level(level);
					info!("{}", format!("endgame winnable! found actions {}", actions.iter().map(|action| action.fmt(&hypo_game)).join(",")).purple());
					return Ok((actions[0], winrate));
				}
			}
		}

		info!("remaining ids: {}", remaining_ids.iter().map(|(id, entry)| format!("{} (missing {})", state.log_id(*id), entry.missing)).join(", "));

		struct Arrangement {
			ids: Vec<Identity>,
			prob: Frac,
			remaining: RemainingMap
		}

		let expand_arr = |arrangement: &Arrangement| -> Vec<Arrangement> {
			let Arrangement { ids, prob, remaining } = arrangement;
			let total_cards = remaining.values().map(|entry| entry.missing).sum::<usize>();

			remaining.iter().filter_map(|(id, RemainingEntry { missing, .. })| {
				let order = unknown_own[ids.len()];
				let thought = &game.me().thoughts[*order];

				// Check if this id cannot be assigned to this order
				let impossible = state.deck[*order].id().is_some_and(|i| i != *id) ||
					!thought.possible.contains(*id) ||
					if !state.is_basic_trash(*id) {
						!thought.possibilities().contains(*id)
					}
					else {
						!thought.possibilities().is_empty() && !thought.possibilities().iter().any(|i| state.is_basic_trash(i)) &&
						// We cannot assign a trash id if it is linked and all other orders are already trash
						(!linked_orders.contains(order) || game.me().links.iter().all(|l| {
							match l {
								Link::Promised { orders, .. } | Link::Unpromised { orders, .. } => {
									!orders.contains(order) || orders.iter().all(|o| o == order || (0..ids.len()).any(|i| o == unknown_own[i] && state.is_basic_trash(ids[i])))
								}
							}
						}))
					};

				if impossible {
					return None;
				}

				let new_remaining = remove_remaining(remaining, *id);

				let mut new_ids = ids.clone();
				new_ids.push(*id);

				let new_prob = *prob * *missing / total_cards;

				Some(Arrangement { ids: new_ids, prob: new_prob, remaining: new_remaining })
			}).collect()
		};

		let mut all_arrangements = vec![Arrangement { ids: vec![], prob: Frac::ONE, remaining: remaining_ids.clone() }];

		for _ in 0..unknown_own.len() {
			if Instant::now() > deadline {
				log::set_max_level(level);
				return Err("timed out".to_string());
			}
			all_arrangements = all_arrangements.iter().flat_map(expand_arr).collect();
		}

		// Normalize all probabilities: some of the potential generated ones may be impossible, so the total prob may be less than 1.
		let sum_prob = all_arrangements.iter().map(|a| a.prob).sum::<GenericFraction<u64>>();
		for arr in &mut all_arrangements {
			assert_eq!(arr.remaining.values().map(|a| a.missing).sum::<usize>(), state.cards_left);
			arr.prob /= sum_prob;
		}

		let mut arrangements = if self.monte_carlo {
			let mut trash_arrs: HashMap<String, Vec<Arrangement>> = HashMap::new();

			for arr in all_arrangements {
				let trash_arr = arr.ids.iter().enumerate().filter_map(|(i, id)| state.is_basic_trash(*id).then_some(i)).join("");
				trash_arrs.entry(trash_arr).or_default().push(arr);
			}

			let mut arrangements = trash_arrs.remove("").unwrap_or_default();

			for (_, arrs) in trash_arrs.drain() {
				let total_winrate = arrs.iter().map(|arr| arr.prob).sum::<GenericFraction<u64>>();
				let amt = std::cmp::min(arrs.len(), 3);
				let selected_arrs = arrs.into_iter().take(amt);
				arrangements.extend(selected_arrs.map(|arr| Arrangement { prob: total_winrate / amt, ..arr }));
			}
			arrangements
		} else { all_arrangements };

		arrangements.sort_by_key(|a| -a.prob);

		info!("arrangements {}", arrangements.len());

		let arranged_games = if arrangements.is_empty() {
			vec![GameArr { game: game.clone(), prob: Frac::ONE, remaining: HashMap::new(), drew: None }]
		}
		else {
			arrangements.iter().map(|Arrangement { ids, prob, remaining }| {
				let mut hypo_game = game.clone();
				let mut new_deck = state.deck.clone();

				for i in 0..ids.len() {
					let order = unknown_own[i];
					new_deck[*order].base = Some(ids[i]);
					hypo_game.deck_ids[*order] = Some(ids[i]);
				}

				hypo_game.state.deck = new_deck;

				GameArr { game: hypo_game, prob: *prob, remaining: remaining.clone(), drew: None }
			}).collect()
		};

		let mut best_performs: HashMap<PerformAction, (Frac, usize)> = HashMap::new();

		for GameArr { game, prob, remaining, .. } in arranged_games {
			if Instant::now() > deadline {
				log::set_max_level(level);
				return Err("timed out".to_string());
			}

			info!("\n{}", format!("arrangement {} {}", game.state.our_hand().iter().map(|&o| game.state.log_iden(&game.state.deck[o])).join(","), prob).purple());
			let all_actions = self.possible_actions(&game, state.our_player_index, &remaining, &deadline);

			if all_actions.is_empty() {
				info!("couldn't find any valid actions");
				continue;
			}

			info!("{}", format!("possible actions: {:?}", all_actions.iter().map(|(action,_)| action.fmt(&game)).join(", ")).green());

			let hypo_games = EndgameSolver::gen_hypo_games(&game, &remaining, all_actions.iter().all(|(p,_)| p.is_clue()));
			let best_result = self.optimize(hypo_games, all_actions, state.our_player_index, 0, &deadline);

			if let Ok((performs, winrate)) = best_result {
				info!("arrangement winnable! {} (winrate {})", performs.iter().map(|perform| perform.fmt(&game)).join(","), winrate);
				for perform in performs {
					let index = best_performs.len();
					best_performs.entry(perform).and_modify(|(w,_)| *w += winrate * prob).or_insert((winrate * prob, index));
				}
			}
		}

		log::set_max_level(level);

		if best_performs.is_empty() {
			Err("couldn't find any winning actions".to_owned())
		}
		else {
			let (best_action, (winrate, _)) = best_performs.into_iter().max_by_key(|(_, (winrate, index))| *winrate * 1000 - Frac::new(*index as u64, 1_u64)).unwrap();
			info!("endgame winnable! {} (winrate {})", best_action.fmt(game), winrate);
			Ok((best_action, winrate))
		}
	}

	fn winnable(&mut self, game: &Game, player_turn: usize, remaining: &RemainingMap, depth: usize, deadline: &Instant) -> WinnableResult {
		let Game { state, .. } = game;

		let hash = game.hash();
		if self.simple_cache.contains_key(&hash) {
			// info!("cached!!");
			return self.simple_cache[&hash].clone();
		}

		if Instant::now() > *deadline {
			return TIMEOUT;
		}

		match EndgameSolver::trivially_winnable(game, player_turn) {
			Ok(action) => {
				self.simple_cache.insert(hash, Ok(action.clone()));
				Ok(action)
			},
			Err(_) => {
				let bottom_decked = !remaining.is_empty() && remaining.keys().all(|id| state.is_critical(*id) && id.rank != 5);

				if EndgameSolver::unwinnable_state(state, player_turn) || bottom_decked {
					// info!("unwinnable");
					self.simple_cache.insert(hash, UNWINNABLE);
					return UNWINNABLE;
				}

				let performs = self.possible_actions(game, player_turn, remaining, deadline);

				if performs.is_empty() {
					// info!("no possible actions in winnable");
					self.simple_cache.insert(hash, UNWINNABLE);
					return UNWINNABLE;
				}

				info!("{}", format!("{}possible actions: {}",
					(0..depth).map(|_| "  ").join(""),
					performs.iter().map(|(p, _)| p.fmt_obj(game, player_turn)).join(", ")).green());

				let hypo_games = EndgameSolver::gen_hypo_games(game, remaining, false);
				let result = self.optimize(hypo_games, performs, player_turn, depth, deadline);
				self.simple_cache.insert(hash, result.clone());
				result
			}
		}
	}

	fn possible_actions(&mut self, game: &Game, player_turn: usize, remaining: &RemainingMap, deadline: &Instant) -> Vec<(PerformAction, Vec<Identity>)> {
		let Game { common, state, meta, .. } = game;
		let mut actions = Vec::new();

		let try_action = |solver: &mut EndgameSolver, perform: PerformAction| {
			match solver.winnable_if(state, player_turn, &perform, remaining, 0, deadline) {
				SimpleResult::Unwinnable => None,
				SimpleResult::WinnableWithDraws(winnable_draws) => Some((perform, winnable_draws)),
				SimpleResult::AlwaysWinnable => Some((perform, Vec::new()))
			}
		};

		if let Some(urgent) = state.hands[player_turn].iter().find(|o| meta[**o].urgent) {
			let perform = if meta[*urgent].status == CardStatus::CalledToPlay {
				PerformAction::Play { table_id: Some(game.table_id), target: *urgent }
			} else {
				PerformAction::Discard { table_id: Some(game.table_id), target: *urgent }
			};

			return match try_action(self, perform) {
				None => Vec::new(),
				Some(r) => vec![r],
			};
		}

		let playables = game.players[player_turn].thinks_playables(&game.frame(), player_turn);
		for order in playables {
			if Instant::now() > *deadline {
				return Vec::new();
			}

			match state.deck[order].id() {
				None => {
					info!("can't identify {order} {:?}", game.players[player_turn].thoughts[order]);
					continue;
				},
				Some(_) => {
					let perform = PerformAction::Play { table_id: Some(game.table_id), target: order };
					match try_action(self, perform) {
						None => continue,
						Some(r) => actions.push(r),
					};
				}
			}
		}

		let default_clue = PerformAction::Rank { table_id: Some(game.table_id), target: 0, value: 0 };
		let too_many_clues = game.state.action_list.concat().iter().rev()
			.take_while(|action| !matches!(action, Action::Play(_) | Action::Discard(_)))
			.filter(|action| matches!(action, Action::Clue(_))).count() > game.state.num_players;
		let clue_winnable = state.clue_tokens > 0 && !too_many_clues && match self.winnable_if(state, player_turn, &default_clue, remaining, 0, deadline) {
			SimpleResult::Unwinnable => false,
			SimpleResult::AlwaysWinnable => true,
			_ => panic!("Shouldn't return WinnableWithDraws enum variant from giving a clue!")
		};

		if clue_winnable {
			// If everyone knows exactly where all the remaining useful cards are, clues are only useful for stalling, so we only need to consider 1 clue
			let fully_known = (remaining.is_empty() || (remaining.len() == 1 && state.is_basic_trash(*remaining.iter().next().unwrap().0))) &&
				state.hands.concat().iter().all(|&o| {
					match state.deck[o].id() {
						None => true,
						Some(id) => state.is_basic_trash(id) || common.thoughts[o].matches(&id, &MatchOptions { infer: true, ..Default::default() })
					}
				});

			for perform in game.convention.find_all_clues(game, player_turn) {
				actions.push((perform, Vec::new()));
				if fully_known {
					break;
				}
			}
		}

		if state.pace() > 0 {
			for perform in game.convention.find_all_discards(game, player_turn) {
				if Instant::now() > *deadline {
					return Vec::new();
				}

				match try_action(self, perform) {
					None => continue,
					Some(r) => actions.push(r),
				};
			}
		}

		actions
	}

	fn advance_game(game: &Game, player_turn: usize, action: &PerformAction) -> Game {
		let Game { state, .. } = game;
		game.simulate_action(&util::perform_to_action(state, action, player_turn, None))
	}

	fn optimize(&mut self, hypo_games: (Vec<GameArr>, Vec<GameArr>), mut actions: Vec<(PerformAction, Vec<Identity>)>, player_turn: usize, depth: usize, deadline: &Instant) -> WinnableResult {
		let (undrawn, drawn) = hypo_games;
		let next_player_index = undrawn[0].game.state.next_player_index(player_turn);
		let mut best_winrate = Frac::ZERO;
		let mut best_actions = Vec::new();

		if let Some(sr) = self.success_rate.get(depth) {
			actions.sort_by_key(|a| sr.get(&a.0).map(|(frac, _)| -frac).unwrap_or(Frac::ZERO));
		}

		for (perform, winnable_draws) in actions {
			if Instant::now() > *deadline {
				return TIMEOUT;
			}

			let mut action_winrate = Frac::ZERO;
			let mut rem_prob = Frac::ONE;

			let hypo_games = if perform.is_clue() { &undrawn } else { &drawn };

			for GameArr { game, prob, remaining, drew } in hypo_games {
				if let Some(id) = drew && !winnable_draws.contains(id) {
					// Drew an unwinnable identity
					continue;
				}

				let new_game = EndgameSolver::advance_game(game, player_turn, &perform);

				// Some critical was lost
				if new_game.state.max_score() < game.state.max_score() {
					continue;
				}

				if perform.is_clue() {
					info!("{}{} cards_left {} endgame_turns {:?} {{",
						(0..depth).map(|_| "  ").join(""),
						perform.fmt_obj(&new_game, player_turn),
						new_game.state.cards_left,
						new_game.state.endgame_turns);
				}
				else {
					info!("{}drawing {} ({}) after {} {} cards_left {} endgame_turns {:?} {{",
						(0..depth).map(|_| "  ").join(""),
						new_game.state.log_oid(drew),
						new_game.state.hands[player_turn][0],
						perform.fmt_obj(&new_game, player_turn),
						new_game.state.hands[player_turn].iter().map(|&o| new_game.state.log_iden(&new_game.state.deck[o])).join(","),
						new_game.state.cards_left,
						new_game.state.endgame_turns);
				}

				let res = match self.winnable(&new_game, next_player_index, remaining, depth + 1, deadline) {
					Err(msg) => {
						format!("{}}} {} unwinnable ({})",
							(0..depth).map(|_| "  ").join(""),
							perform.fmt_obj(game, player_turn),
							msg)
					},
					Ok((performs, winrate)) => {
						action_winrate += prob * winrate;

						if action_winrate > Frac::ONE {
							println!("{}", hypo_games.iter().map(|h| h.prob).join(","));
							panic!("Winrate exceeds 100% {prob} {winrate}");
						}

						format!("{}}} {} prob {} winrate {}",
							(0..depth).map(|_| "  ").join(""),
							performs.iter().map(|p| p.fmt_obj(game, next_player_index)).join(", "),
							prob,
							winrate)
					}
				};

				info!("{}", if depth == 0 { res.yellow() } else { res.white() });

				rem_prob -= prob;

				if action_winrate + rem_prob <= best_winrate {
					break;
				}
			}

			while depth >= self.success_rate.len() {
				self.success_rate.push(HashMap::new());
			}

			self.success_rate[depth].entry(perform).and_modify(|entry| {
				let (frac, times) = entry;
				let new_frac = (*frac * *times + action_winrate) / (*times + 1);
				*frac = new_frac;
				*times += 1;
			}).or_insert((action_winrate, 1));

			if action_winrate == Frac::ONE {
				return Ok((vec![perform], Frac::ONE));
			}

			if best_winrate < action_winrate {
				best_winrate = action_winrate;
				best_actions = vec![perform];
			}
			else if action_winrate > Frac::ZERO && best_winrate == action_winrate {
				best_actions.push(perform);
			}
		}

		if best_actions.is_empty() {
			Err("no action wins")
		} else {
			Ok((best_actions, best_winrate))
		}
	}

	/**
	 * Generates a map of game arrangements for the possible actions.
	 */
	fn gen_hypo_games(game: &Game, remaining: &RemainingMap, clue_only: bool) -> (Vec<GameArr>, Vec<GameArr>) {
		let Game { state, .. } = game;
		let default_game = GameArr { game: game.clone(), prob: Frac::ONE, remaining: remaining.clone(), drew: None };

		if clue_only {
			return (vec![default_game], Vec::new());
		}

		let mut drawn = Vec::new();
		assert_eq!(remaining.values().map(|r| r.missing).sum::<usize>(), state.cards_left);

		for (id, RemainingEntry { missing, .. }) in remaining {
			let mut new_game = game.clone();
			assert_eq!(new_game.state.deck.len(), state.card_order);

			new_game.deck_ids.push(Some(*id));

			let new_remaining = remove_remaining(remaining, *id);
			drawn.push(GameArr { game: new_game, prob: Frac::new(*missing as u64, state.cards_left as u64), remaining: new_remaining, drew: Some(*id) });
		}

		if drawn.is_empty() {
			drawn.push(default_game.clone());
		}

		(vec![default_game], drawn)
	}
}

#[derive(Debug, Clone)]
pub struct RemainingEntry {
	pub missing: usize,
	pub all: bool,
}

fn find_remaining_ids(game: &Game) -> (RemainingMap, Vec<(usize, Option<Identity>)>) {
	let Game { state, .. } = game;
	let mut seen_ids = HashMap::new();
	let mut own_ids: Vec<(usize, Option<Identity>)> = Vec::new();
	let mut infer_ids: HashMap<Identity, Vec<usize>> = HashMap::new();

	for i in 0..state.num_players {
		for &order in &state.hands[i] {
			// Identify all the cards we know for sure
			if let Some(id) = game.me().thoughts[order].id() {
				seen_ids.entry(id).and_modify(|e| *e += 1).or_insert(1);

				if i == state.our_player_index {
					own_ids.push((order, Some(id)));
				}
			}
			else if i == state.our_player_index {
				match game.me().thoughts[order].identity(&IdOptions { infer: true, ..Default::default() }) {
					Some(id) => {
						infer_ids.entry(id).and_modify(|e| e.push(order)).or_insert(vec![order]);
					}
					None => own_ids.push((order, None))
				}
			}
		}
	}

	// Check that the inferred ids don't add up to too many
	for (id, orders) in infer_ids {
		let seen = seen_ids.get(&id).unwrap_or(&0);
		let too_many = seen + orders.len() + state.base_count(id) > state.card_count(id);

		if !too_many {
			seen_ids.insert(id, seen + orders.len());
		}

		for o in orders {
			own_ids.push((o, (!too_many).then_some(id)));
		}
	}

	let mut remaining_ids = HashMap::new();

	for id in all_ids(&state.variant) {
		let total = state.card_count(id);
		let missing = total - state.base_count(id) - seen_ids.get(&id).unwrap_or(&0);

		if missing > 0 {
			remaining_ids.insert(id, RemainingEntry { missing, all: missing == total });
		}
	}

	(remaining_ids, own_ids)
}
