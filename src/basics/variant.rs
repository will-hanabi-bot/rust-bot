use serde::{Deserialize};
use regex::Regex;
use std::sync::LazyLock;

use super::card::{Identity, Identifiable};
use super::clue::{BaseClue, ClueKind};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
	pub id: u32,
	pub name: String,
	pub suits: Vec<String>,

	#[serde(rename="criticalRank")]
	pub critical_rank: Option<usize>,
	#[serde(rename="clueStarved")]
	pub clue_starved: Option<bool>,
	#[serde(rename="specialRank")]
	pub special_rank: Option<usize>,
	#[serde(rename="specialRankAllClueColors")]
	pub rainbow_s: Option<bool>,
	#[serde(rename="specialRankNoClueColors")]
	pub white_s: Option<bool>,
	#[serde(rename="specialRankAllClueRanks")]
	pub pink_s: Option<bool>,
	#[serde(rename="specialRankNoClueRanks")]
	pub brown_s: Option<bool>,
	#[serde(rename="specialRankDeceptive")]
	pub deceptive_s: Option<bool>,

	pub short_forms: Option<Vec<String>>,
	pub colourable_suits: Option<Vec<String>>,
}

#[derive(Default)]
pub struct VariantOpts {
	pub critical_rank: Option<usize>,
	pub clue_starved: Option<bool>,
	pub special_rank: Option<usize>,
	pub rainbow_s: Option<bool>,
	pub white_s: Option<bool>,
	pub pink_s: Option<bool>,
	pub brown_s: Option<bool>,
	pub deceptive_s: Option<bool>,
}

impl Variant {
	pub fn new(id: u32, name: &str, suit_strs: &[&str], short_strs: &[&str], opts: VariantOpts) -> Self {
		let VariantOpts { critical_rank, clue_starved, special_rank, rainbow_s, white_s, pink_s, brown_s, deceptive_s } = opts;

		let mut suits = Vec::new();
		let mut short_forms = Vec::new();
		let mut colourable_suits = Vec::new();

		for i in 0..suit_strs.len() {
			let suit = suit_strs[i].to_string();
			let colourable = !NO_COLOUR.is_match(&suit);
			suits.push(suit.clone());
			short_forms.push(short_strs[i].to_string());

			if colourable {
				colourable_suits.push(suit);
			}
		}

		Self {
			id,
			name: name.to_string(),
			suits,
			colourable_suits: Some(colourable_suits),
			short_forms: Some(short_forms),
			critical_rank,
			clue_starved,
			special_rank,
			rainbow_s,
			white_s,
			pink_s,
			brown_s,
			deceptive_s
		}
	}
}

#[derive(Debug, Deserialize, Clone)]
pub struct Suit {
	pub name: String,
	pub abbreviation: Option<String>,
}

pub struct VariantManager {
	variants: Vec<Variant>,
	colours: Vec<Suit>,
}

impl VariantManager {
	pub async fn new() -> Self {
		let variants_raw = reqwest::get(VARIANTS_URL).await.expect("Failed to fetch variants.")
			.text().await.expect("Failed to parse variants response.");

		let variants = serde_json::from_str(&variants_raw).expect("Failed to parse variants response as JSON.");

		let colours_raw = reqwest::get(COLOURS_URL).await.expect("Failed to fetch colours.")
			.text().await.expect("Failed to parse colours response.");

		let colours: Vec<Suit> = serde_json::from_str(&colours_raw).expect("Failed to parse colours response as JSON.");

		Self { variants, colours }
	}

	pub fn get_variant(&mut self, name: &str) -> Variant {
		let mut var = self.variants.iter_mut().find(|variant| variant.name == name).unwrap_or_else(|| panic!("Variant '{name}' not found")).clone();

		if var.short_forms.is_some() {
			return var;
		}

		let mut short_forms: Vec<String> = Vec::new();
		for suit in &var.suits {
			let short = match suit.as_str() {
				"Black" => "k".to_string(),
				"Pink" => "i".to_string(),
				"Brown" => "n".to_string(),
				_ => {
					if let Some(colour) = self.colours.iter().find(|colour| &colour.name == suit) {
						let abbreviation = colour.abbreviation.clone().unwrap_or(suit[0..1].to_lowercase().to_string());
						if !short_forms.contains(&abbreviation) {
							abbreviation.clone()
						} else {
							// Look for the first unused character
							suit.to_lowercase().split("").find(|c| !short_forms.contains(&c.to_string())).unwrap_or_else(|| panic!("No unused character found for suit '{suit}' in {:?}", var.suits)).to_string()
						}
					} else {
						panic!("Colour '{suit}' not found");
					}
				}
			};
			short_forms.push(short);
		}

		let colourable_suits = var.suits.iter().filter(|suit| !NO_COLOUR.is_match(suit)).map(|suit| suit.to_string()).collect();

		var.short_forms = Some(short_forms);
		var.colourable_suits = Some(colourable_suits);
		var
	}
}

static VARIANTS_URL: &str = "https://raw.githubusercontent.com/Hanabi-Live/hanabi-live/main/packages/game/src/json/variants.json";
static COLOURS_URL: &str = "https://raw.githubusercontent.com/Hanabi-Live/hanabi-live/main/packages/game/src/json/suits.json";

pub static WHITISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"White|Gray|Light|Null").unwrap());
pub static RAINBOWISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Rainbow|Omni").unwrap());
pub static PINKISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Pink|Omni").unwrap());
pub static BROWNISH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Brown|Muddy|Cocoa|Null").unwrap());
pub static DARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Black|Dark|Gray|Cocoa").unwrap());
pub static PRISM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Prism").unwrap());
pub static NO_COLOUR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"White|Gray|Light|Null|Rainbow|Omni|Prism").unwrap());

pub fn all_ids(variant: &Variant) -> impl Iterator<Item = Identity> {
	(0..variant.suits.len()).flat_map(move |suit_index|
		(1..=5).map(move |rank| Identity { suit_index, rank })
	)
}

pub fn touch_possibilities(clue: &BaseClue, variant: &Variant) -> Vec<Identity> {
	all_ids(variant).filter(|id| card_touched(id, variant, clue)).collect()
}

pub fn id_touched(id: Identity, variant: &Variant, clue: &BaseClue) -> bool {
	let BaseClue { kind, value } = clue;

	let Identity { suit_index, rank } = id;
	let suit = &variant.suits[suit_index];

	if *kind == ClueKind::COLOUR {
		if WHITISH.is_match(suit) {
			return false;
		}

		if RAINBOWISH.is_match(suit) {
			return true;
		}

		if variant.special_rank.is_some_and(|r| r == rank) {
			if variant.rainbow_s.is_some_and(|c| c) {
				return true;
			}
			else if variant.white_s.is_some_and(|c| c) {
				return false;
			}
		}

		if PRISM.is_match(suit) {
			return ((rank - 1) % variant.colourable_suits.as_ref().unwrap().len()) == *value;
		}

		variant.suits[suit_index] == variant.colourable_suits.as_ref().unwrap()[*value]
	}
	else {
		if BROWNISH.is_match(suit) {
			return false;
		}

		if variant.special_rank.is_some_and(|r| r == rank) {
			if variant.pink_s.is_some_and(|c| c) {
				return rank != *value;
			}
			else if variant.brown_s.is_some_and(|c| c) {
				return false;
			}
			else if variant.deceptive_s.is_some_and(|c| c) {
				return (suit_index % 4) + (if variant.special_rank.unwrap() == 1 { 2 } else { 1 }) == *value;
			}
		}

		if PINKISH.is_match(suit) {
			return true;
		}

		rank == *value
	}
}

pub fn card_touched(card: &impl Identifiable, variant: &Variant, clue: &BaseClue) -> bool {
	match card.id() {
		None => false,
		Some(id) => id_touched(id, variant, clue)
	}
}

pub fn card_count(variant: &Variant, identity: Identity) -> usize {
	let Identity { suit_index, rank } = identity;
	if DARK.is_match(&variant.suits[suit_index]) || variant.critical_rank.is_some_and(|r| r == rank) {
		1
	}
	else {
		[3, 2, 2, 2, 1][rank - 1]
	}
}
