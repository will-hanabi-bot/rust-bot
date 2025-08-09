use serde::{Deserialize};
use regex::Regex;
use std::sync::LazyLock;

use super::card::{Identity, Identifiable};
use super::clue::{BaseClue, ClueKind};

#[derive(Debug, Deserialize, Clone)]
pub struct JSONVariant {
	pub id: u32,
	pub name: String,
	pub suits: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Variant {
	pub id: u32,
	pub name: String,
	pub suits: Vec<String>,
	pub colourable_suits: Vec<String>,
	pub short_forms: Vec<String>,
}

impl Variant {
	pub fn new(id: u32, name: &str, suit_strs: &[&str], short_strs: &[&str]) -> Self {
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
			colourable_suits,
			short_forms
		}
	}
}

#[derive(Debug, Deserialize, Clone)]
pub struct Suit {
	pub name: String,
	pub abbreviation: Option<String>,
}

pub struct VariantManager {
	variants: Vec<JSONVariant>,
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

	pub fn get_variant(&self, name: &str) -> Variant {
		let JSONVariant { id, name, suits } = self.variants.iter().find(|variant| variant.name == name).unwrap_or_else(|| panic!("Variant '{name}' not found")).clone();

		let mut short_forms: Vec<String> = Vec::new();
		for suit in &suits {
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
							suit.to_lowercase().split("").find(|c| !short_forms.contains(&c.to_string())).unwrap_or_else(|| panic!("No unused character found for suit '{suit}' in {suits:?}")).to_string()
						}
					} else {
						panic!("Colour '{suit}' not found");
					}
				}
			};
			short_forms.push(short);
		}

		let colourable_suits = suits.iter().filter(|suit| !NO_COLOUR.is_match(suit)).map(|suit| suit.to_string()).collect();

		Variant { id, name, suits, colourable_suits, short_forms }
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

		if PRISM.is_match(suit) {
			return ((rank - 1) % variant.colourable_suits.len()) == *value;
		}

		variant.suits[suit_index] == variant.colourable_suits[*value]
	}
	else {
		if BROWNISH.is_match(suit) {
			return false;
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
	if DARK.is_match(&variant.suits[suit_index]) {
		1
	}
	else {
		[3, 2, 2, 2, 1][rank - 1]
	}
}
