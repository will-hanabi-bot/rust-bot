use log::{info, warn};

use crate::basics::action::DiscardAction;
use crate::basics::card::{CardStatus, Identifiable, Identity, MatchOptions};
use crate::basics::game::Game;
use crate::basics::identity_set::IdentitySet;
use crate::basics::player::Link;
use crate::reactor::{DiscardInterp, Reactor};

impl Reactor {
	pub(super) fn interpret_useful_dc(game: &mut Game, action: &DiscardAction) -> DiscardInterp {
		let Game { common, state, meta, players, .. } = game;
		let me = &players[state.our_player_index];
		let DiscardAction { player_index, order, suit_index, rank, .. } = action;
		let id = Identity { suit_index: *suit_index as usize, rank: *rank as usize };

		let gd = common.hypo_plays.contains(order);

		if let Some(dupe) = state.hands.concat().iter().find(|&&o| state.deck[o].is(&id)) {
			let holder = state.holder_of(*dupe);

			if holder == *player_index {
				if players[*player_index].thoughts[*dupe].matches(&id, &MatchOptions { infer: true, ..Default::default() }) {
					info!("discarded dupe of own hand");
				}
				else {
					warn!("discarded useful {} but dupe was in their own hand!", state.log_id(id));
					return DiscardInterp::None;
				}
			}

			if gd {
				let target = state.hands[holder].iter().rev().find(|&&o| common.thoughts[o].possible.contains(id)).unwrap();

				if target != dupe {
					warn!("transfer to {dupe} was not to rightmost {target}!");
					return DiscardInterp::Mistake;
				}

				info!("gd to {}'s {target}", state.player_names[holder]);
				common.thoughts[*target].inferred = IdentitySet::single(id);
				meta[*target].status = CardStatus::GentlemansDiscard;
				return DiscardInterp::GentlemansDiscard;
			}
			else {
				let orders = state.hands[holder].iter().filter(|&&o| common.thoughts[o].possible.contains(id)).copied().collect::<Vec<_>>();
				info!("sarcastic to {}'s {orders:?}", state.player_names[holder]);
				common.links.push(Link::Sarcastic { orders, id });
				return DiscardInterp::Sarcastic;
			}
		}

		// We discarded a card that we don't see nor have the other copy of
		if *player_index == state.our_player_index {
			return DiscardInterp::Mistake;
		}

		// Since we can't find it, we must be the target
		if gd {
			if let Some(target) = state.our_hand().iter().rev().find(|&&o| me.thoughts[o].possible.contains(id)) {
				info!("gd to our {target}");
				common.thoughts[*target].inferred = IdentitySet::single(id);
				meta[*target].status = CardStatus::GentlemansDiscard;
				DiscardInterp::GentlemansDiscard
			}
			else {
				warn!("looked like gd but we don't see it and impossible for us to have!");
				DiscardInterp::Mistake
			}
		}
		else {
			let orders = state.our_hand().iter().filter(|&&o| common.thoughts[o].possible.contains(id)).copied().collect::<Vec<_>>();
			info!("sarcastic to our {orders:?}");
			common.links.push(Link::Sarcastic { orders, id });
			DiscardInterp::Sarcastic
		}
	}
}
