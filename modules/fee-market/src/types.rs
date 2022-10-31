// This file is part of Darwinia.
//
// Copyright (C) 2018-2022 Darwinia Network
// SPDX-License-Identifier: GPL-3.0
//
// Darwinia is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Darwinia is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Darwinia. If not, see <https://www.gnu.org/licenses/>.

// core
use core::{cmp::Ordering, ops::Range};
// crates.io
use codec::{Decode, Encode};
use scale_info::TypeInfo;
// darwinia-network
use bp_messages::{LaneId, MessageNonce};
// paritytech
use sp_runtime::{traits::AtLeast32BitUnsigned, RuntimeDebug};
use sp_std::vec::Vec;

/// Relayer who has enrolled the fee market
#[derive(Clone, Default, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Relayer<AccountId, Balance> {
	pub id: AccountId,
	pub collateral: Balance,
	pub fee: Balance,
}

impl<AccountId, Balance> Relayer<AccountId, Balance> {
	pub fn new(id: AccountId, collateral: Balance, fee: Balance) -> Relayer<AccountId, Balance> {
		Relayer { id, collateral, fee }
	}
}

impl<AccountId, Balance> PartialOrd for Relayer<AccountId, Balance>
where
	AccountId: PartialEq,
	Balance: Ord,
{
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(match self.fee.cmp(&other.fee) {
			// We reverse the order here to turn the collateral value into rank.
			//
			// Use `other.cmp(self)` instead of `self.cmp(other)`.
			Ordering::Equal => other.collateral.cmp(&self.collateral),
			ordering => ordering,
		})
	}
}

impl<AccountId, Balance> Ord for Relayer<AccountId, Balance>
where
	AccountId: Eq,
	Balance: Ord,
{
	fn cmp(&self, other: &Self) -> Ordering {
		match self.fee.cmp(&other.fee) {
			// We reverse the order here to turn the collateral value into rank.
			//
			// Use `other.cmp(self)` instead of `self.cmp(other)`.
			Ordering::Equal => other.collateral.cmp(&self.collateral),
			ordering => ordering,
		}
	}
}

/// Order represent cross-chain message relay task. Only support sub-sub message for now.
#[derive(Clone, Default, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct Order<AccountId, BlockNumber, Balance> {
	pub lane: LaneId,
	pub message: MessageNonce,
	pub sent_time: BlockNumber,
	pub confirm_time: Option<BlockNumber>,
	pub collateral_per_assigned_relayer: Balance,
	pub assigned_relayers: Vec<AssignedRelayer<AccountId, BlockNumber, Balance>>,
}
impl<AccountId, BlockNumber, Balance> Order<AccountId, BlockNumber, Balance>
where
	AccountId: Clone,
	BlockNumber: Copy + AtLeast32BitUnsigned + Default,
	Balance: Copy + Default,
{
	pub fn new(
		lane: LaneId,
		message: MessageNonce,
		sent_time: BlockNumber,
		collateral_per_assigned_relayer: Balance,
		relayers: Vec<Relayer<AccountId, Balance>>,
		slot: BlockNumber,
	) -> Self {
		let relayers_len = relayers.len();
		let mut assigned_relayers = Vec::with_capacity(relayers_len);
		let mut start_time = sent_time;

		// AssignedRelayer has a duty time zone
		for i in 0..relayers_len {
			if let Some(r) = relayers.get(i) {
				let p = AssignedRelayer::new(r.id.clone(), r.fee, start_time, slot);

				start_time += slot;
				assigned_relayers.push(p);
			}
		}

		Self {
			lane,
			message,
			sent_time,
			confirm_time: None,
			collateral_per_assigned_relayer,
			assigned_relayers,
		}
	}

	pub fn set_confirm_time(&mut self, confirm_time: Option<BlockNumber>) {
		self.confirm_time = confirm_time;
	}

	pub fn assigned_relayers_slice(&self) -> &[AssignedRelayer<AccountId, BlockNumber, Balance>] {
		self.assigned_relayers.as_ref()
	}

	pub fn fee(&self) -> Balance {
		self.assigned_relayers.iter().last().map(|r| r.fee).unwrap_or_default()
	}

	pub fn is_confirmed(&self) -> bool {
		self.confirm_time.is_some()
	}

	pub fn range_end(&self) -> Option<BlockNumber> {
		self.assigned_relayers.iter().last().map(|r| r.valid_range.end)
	}

	pub fn comfirm_delay(&self) -> Option<BlockNumber> {
		if let (Some(confirm_time), Some(range_end)) = (self.confirm_time, self.range_end()) {
			if confirm_time > range_end {
				return Some(confirm_time - range_end);
			}
		}
		None
	}

	pub fn confirmed_info(&self) -> Option<(usize, Balance)> {
		// The confirm_time of the order is already set in the `OnDeliveryConfirmed`
		// callback.And the callback was called as source chain received message
		// delivery proof, before the reward payment.
		let order_confirmed_time = self.confirm_time.unwrap_or_default();
		for (index, assigned_relayer) in self.assigned_relayers.iter().enumerate() {
			if assigned_relayer.valid_range.contains(&order_confirmed_time) {
				return Some((index, assigned_relayer.fee));
			}
		}
		None
	}

	#[cfg(test)]
	pub fn relayer_valid_range(&self, id: AccountId) -> Option<Range<BlockNumber>>
	where
		AccountId: Clone + PartialEq,
	{
		for assigned_relayer in self.assigned_relayers.iter() {
			if assigned_relayer.id == id {
				return Some(assigned_relayer.valid_range.clone());
			}
		}
		None
	}
}

/// Relayers selected by the fee market. Each assigned relayer has a valid slot, if the order can
/// finished in time, will be rewarded with more percentage. AssignedRelayer are responsible for the
/// messages relay in most time.
#[derive(Clone, Debug, Default, Encode, Decode, TypeInfo)]
pub struct AssignedRelayer<AccountId, BlockNumber, Balance> {
	pub id: AccountId,
	pub fee: Balance,
	pub valid_range: Range<BlockNumber>,
}
impl<AccountId, BlockNumber, Balance> AssignedRelayer<AccountId, BlockNumber, Balance>
where
	BlockNumber: Copy + AtLeast32BitUnsigned + Default,
{
	pub fn new(
		id: AccountId,
		fee: Balance,
		start_time: BlockNumber,
		slot_time: BlockNumber,
	) -> Self {
		Self { id, fee, valid_range: Range { start: start_time, end: start_time + slot_time } }
	}
}

/// The detail information about slash behavior
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct SlashReport<AccountId, BlockNumber, Balance> {
	pub lane: LaneId,
	pub message: MessageNonce,
	pub sent_time: BlockNumber,
	pub confirm_time: Option<BlockNumber>,
	pub delay_time: Option<BlockNumber>,
	pub account_id: AccountId,
	pub amount: Balance,
}

impl<AccountId, BlockNumber, Balance> SlashReport<AccountId, BlockNumber, Balance>
where
	AccountId: Clone,
	BlockNumber: Copy + AtLeast32BitUnsigned + Default,
	Balance: Copy + Default,
{
	pub fn new(
		order: &Order<AccountId, BlockNumber, Balance>,
		account_id: AccountId,
		amount: Balance,
	) -> Self {
		Self {
			lane: order.lane,
			message: order.message,
			sent_time: order.sent_time,
			confirm_time: order.confirm_time,
			delay_time: order.comfirm_delay(),
			account_id,
			amount,
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	type AccountId = u32;
	type BlockNumber = u32;
	type Balance = u128;

	const TEST_LANE_ID: LaneId = [0, 0, 0, 1];
	const TEST_MESSAGE_NONCE: MessageNonce = 0;

	#[test]
	fn relayer_ord_should_work() {
		let mut relayers = vec![
			<Relayer<AccountId, Balance>>::new(1, 100, 30),
			<Relayer<AccountId, Balance>>::new(2, 100, 40),
			<Relayer<AccountId, Balance>>::new(3, 150, 30),
			<Relayer<AccountId, Balance>>::new(4, 100, 30),
		];

		relayers.sort();

		assert_eq!(relayers.into_iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 1, 4, 2]);
	}

	#[test]
	fn test_assign_order_relayers_one() {
		let order = <Order<AccountId, BlockNumber, Balance>>::new(
			TEST_LANE_ID,
			TEST_MESSAGE_NONCE,
			100,
			100,
			vec![<Relayer<AccountId, Balance>>::new(1, 100, 30)],
			50,
		);

		assert_eq!(order.relayer_valid_range(1).unwrap(), (100..150));
	}

	#[test]
	fn test_assign_order_relayers_two() {
		let order = <Order<AccountId, BlockNumber, Balance>>::new(
			TEST_LANE_ID,
			TEST_MESSAGE_NONCE,
			100,
			100,
			vec![
				<Relayer<AccountId, Balance>>::new(1, 100, 30),
				<Relayer<AccountId, Balance>>::new(2, 100, 30),
			],
			50,
		);

		assert_eq!(order.relayer_valid_range(1).unwrap(), (100..150));
		assert_eq!(order.relayer_valid_range(2).unwrap(), (150..200));
	}

	#[test]
	fn test_assign_order_relayers_three() {
		let order = <Order<AccountId, BlockNumber, Balance>>::new(
			TEST_LANE_ID,
			TEST_MESSAGE_NONCE,
			100,
			100,
			vec![
				<Relayer<AccountId, Balance>>::new(1, 100, 30),
				<Relayer<AccountId, Balance>>::new(2, 100, 40),
				<Relayer<AccountId, Balance>>::new(3, 100, 80),
			],
			50,
		);

		assert_eq!(order.relayer_valid_range(1).unwrap(), (100..150));
		assert_eq!(order.relayer_valid_range(2).unwrap(), (150..200));
		assert_eq!(order.relayer_valid_range(3).unwrap(), (200..250));
		assert_eq!(order.range_end(), Some(250));
		assert_eq!(order.fee(), 80);
	}

	#[test]
	fn test_assign_order_relayers_four() {
		let order = <Order<AccountId, BlockNumber, Balance>>::new(
			TEST_LANE_ID,
			TEST_MESSAGE_NONCE,
			100,
			100,
			vec![
				<Relayer<AccountId, Balance>>::new(1, 100, 30),
				<Relayer<AccountId, Balance>>::new(2, 100, 30),
				<Relayer<AccountId, Balance>>::new(3, 100, 30),
				<Relayer<AccountId, Balance>>::new(4, 100, 30),
			],
			50,
		);

		assert_eq!(order.relayer_valid_range(1).unwrap(), (100..150));
		assert_eq!(order.relayer_valid_range(2).unwrap(), (150..200));
		assert_eq!(order.relayer_valid_range(3).unwrap(), (200..250));
		assert_eq!(order.relayer_valid_range(4).unwrap(), (250..300));
	}
}
