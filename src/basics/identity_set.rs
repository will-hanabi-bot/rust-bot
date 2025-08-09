use crate::basics::card::Identity;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub struct IdentitySet(usize);

impl IdentitySet {
	pub const EMPTY: Self = IdentitySet(0);

	pub fn value(&self) -> usize {
		self.0
	}

	#[inline]
	pub fn len(&self) -> usize {
		self.0.count_ones() as usize
	}

	#[inline]
	pub fn is_empty(&self) -> bool {
		self.0 == 0
	}

	pub fn single(id: Identity) -> Self {
		IdentitySet(1 << id.to_ord())
	}

	pub fn with(&self, id: Identity) -> Self {
		IdentitySet(self.0 | 1 << id.to_ord())
	}

	pub fn concat(&self, ids: &[Identity]) -> Self {
		let mut val = self.0;
		for id in ids {
			val |= 1 << id.to_ord();
		}
		IdentitySet(val)
	}

	pub fn contains(&self, id: Identity) -> bool {
		let bit = 1 << id.to_ord();
		(self.0 & bit) != 0
	}

	pub fn intersect(self, other: &Self) -> Self {
		IdentitySet(self.0 & other.0)
	}

	pub fn union(self, other: &Self) -> Self {
		IdentitySet(self.0 | other.0)
	}

	pub fn difference(self, other: &Self) -> Self {
		IdentitySet(self.0 & !other.0)
	}

	pub fn filter<F>(&self, mut cond: F) -> Self where F: FnMut(Identity) -> bool {
		let mut bits = self.0;
		let mut res = *self;

		while bits != 0 {
			let tz = bits.trailing_zeros() as usize;
			bits &= bits - 1;

			let id = Identity::from_ord(tz);
			if !cond(id) {
				res.0 &= !(1 << tz);
			}
		}
		res
	}

	pub fn retain<F>(&mut self, mut cond: F) where F: FnMut(Identity) -> bool {
		let mut bits = self.0;

		while bits != 0 {
			let tz = bits.trailing_zeros() as usize;
			bits &= bits - 1;

			let id = Identity::from_ord(tz);
			if !cond(id) {
				self.0 &= !(1 << tz);
			}
		}
	}

	pub fn iter(&self) -> IdentitySetIter {
		IdentitySetIter { bits: self.0 }
	}

	pub fn to_vec(&self) -> Vec<Identity> {
		self.iter().collect()
	}
}

impl FromIterator<Identity> for IdentitySet {
	fn from_iter<T: IntoIterator<Item = Identity>>(iter: T) -> Self {
		let mut val = 0;

		for id in iter {
			val += 1 << id.to_ord();
		}

		IdentitySet(val)
	}
}

pub struct IdentitySetIter {
	bits: usize,
}

impl Iterator for IdentitySetIter {
	type Item = Identity;

	#[inline]
	fn next(&mut self) -> Option<Identity> {
		if self.bits == 0 {
			return None;
		}
		let tz = self.bits.trailing_zeros();
		self.bits &= self.bits - 1;
		Some(Identity::from_ord(tz as usize))
	}
}

impl ExactSizeIterator for IdentitySetIter {
	#[inline]
	fn len(&self) -> usize {
		self.bits.count_ones() as usize
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn it_inserts() {
		let ids = IdentitySet::EMPTY;
		let id = Identity { suit_index: 2, rank: 4 };
		let new_ids = ids.with(id);

		assert!(new_ids.contains(id));
		assert_eq!(new_ids.len(), 1);
		assert_eq!(ids.len(), 0);
	}

	#[test]
	fn it_froms() {
		let ids = IdentitySet::from_iter(vec![
			Identity { suit_index: 0, rank: 1 },
			Identity { suit_index: 1, rank: 4 },
			Identity { suit_index: 2, rank: 2 },
		]);

		assert_eq!(ids.len(), 3);
		assert!(ids.contains(Identity { suit_index: 0, rank: 1 }));
		assert!(ids.contains(Identity { suit_index: 1, rank: 4 }));
		assert!(ids.contains(Identity { suit_index: 2, rank: 2 }));
	}

	#[test]
	fn it_retains() {
		let mut ids = IdentitySet::from_iter((0..5).flat_map(|suit_index| (1..=5).map(move |rank| Identity { suit_index, rank })));
		let stacks = [3, 1, 4, 1, 5];
		ids.retain(|i| i.rank > stacks[i.suit_index]);

		assert_eq!(ids.len(), 11);
	}

	#[test]
	fn it_filters() {
		let ids = IdentitySet::from_iter((0..5).flat_map(|suit_index| (1..=5).map(move |rank| Identity { suit_index, rank })));
		let stacks = [3, 1, 4, 1, 5];
		let new_ids = ids.filter(|i| i.rank > stacks[i.suit_index]);

		assert_eq!(ids.len(), 25);
		assert_eq!(new_ids.len(), 11);
	}
}
