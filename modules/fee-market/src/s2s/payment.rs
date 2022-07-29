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

// --- paritytech ---
use bp_messages::{
	source_chain::{MessageDeliveryAndDispatchPayment, SenderOrigin},
	MessageNonce, UnrewardedRelayer,
};
use frame_support::{
	log,
	traits::{Currency as CurrencyT, ExistenceRequirement, Get},
};
use scale_info::TypeInfo;
use sp_runtime::traits::{AccountIdConversion, CheckedDiv, Saturating, UniqueSaturatedInto, Zero};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	ops::RangeInclusive,
};
// --- darwinia-network ---
use crate::{Config, Orders, Pallet, *};

/// Error that occurs when message fee is non-zero, but payer is not defined.
const NON_ZERO_MESSAGE_FEE_CANT_BE_PAID_BY_NONE: &str =
	"Non-zero message fee can't be paid by <None>";

pub struct FeeMarketPayment<T, I, Currency> {
	_phantom: sp_std::marker::PhantomData<(T, I, Currency)>,
}

impl<T, I, Currency> MessageDeliveryAndDispatchPayment<T::Origin, T::AccountId, BalanceOf<T, I>>
	for FeeMarketPayment<T, I, Currency>
where
	T: frame_system::Config + Config<I>,
	I: 'static,
	T::Origin: SenderOrigin<T::AccountId>,
	Currency: CurrencyT<T::AccountId>,
{
	type Error = &'static str;

	fn pay_delivery_and_dispatch_fee(
		submitter: &T::Origin,
		fee: &BalanceOf<T, I>,
		relayer_fund_account: &T::AccountId,
	) -> Result<(), Self::Error> {
		let submitter_account = match submitter.linked_account() {
			Some(submitter_account) => submitter_account,
			None if !fee.is_zero() => {
				// if we'll accept some message that has declared that the `fee` has been paid but
				// it isn't actually paid, then it'll lead to problems with delivery confirmation
				// payments (see `pay_relayer_rewards` && `confirmation_relayer` in particular)
				return Err(NON_ZERO_MESSAGE_FEE_CANT_BE_PAID_BY_NONE);
			},
			None => {
				// message lane verifier has accepted the message before, so this message
				// is unpaid **by design**
				// => let's just do nothing
				return Ok(());
			},
		};

		<T as Config<I>>::Currency::transfer(
			&submitter_account,
			relayer_fund_account,
			*fee,
			// it's fine for the submitter to go below Existential Deposit and die.
			ExistenceRequirement::AllowDeath,
		)
		.map_err(Into::into)
	}

	fn pay_relayers_rewards(
		lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<T::AccountId>>,
		confirmation_relayer: &T::AccountId,
		received_range: &RangeInclusive<MessageNonce>,
		relayer_fund_account: &T::AccountId,
	) {
		let RewardsBook { deliver_sum, confirm_sum, assigned_relayers_sum, treasury_sum } =
			calculate_rewards::<T, I>(
				lane_id,
				messages_relayers,
				confirmation_relayer.clone(),
				received_range,
				relayer_fund_account,
			);

		// Pay confirmation relayer rewards
		do_reward::<T, I>(relayer_fund_account, confirmation_relayer, confirm_sum);
		// Pay messages relayers rewards
		for (relayer, reward) in deliver_sum {
			do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		}
		// Pay assign relayer reward
		for (relayer, reward) in assigned_relayers_sum {
			do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		}
		// Pay treasury_sum reward
		do_reward::<T, I>(
			relayer_fund_account,
			&T::TreasuryPalletId::get().into_account_truncating(),
			treasury_sum,
		);
	}
}

/// Calculate rewards for messages_relayers, confirmation relayers, treasury_sum,
/// assigned_relayers
pub fn calculate_rewards<T, I>(
	lane_id: LaneId,
	messages_relayers: VecDeque<UnrewardedRelayer<T::AccountId>>,
	confirm_relayer: T::AccountId,
	received_range: &RangeInclusive<MessageNonce>,
	relayer_fund_account: &T::AccountId,
) -> RewardsBook<T, I>
where
	T: frame_system::Config + Config<I>,
	I: 'static,
{
	let mut rewards_book = RewardsBook::new();
	for entry in messages_relayers {
		let nonce_begin = sp_std::cmp::max(entry.messages.begin, *received_range.start());
		let nonce_end = sp_std::cmp::min(entry.messages.end, *received_range.end());

		for message_nonce in nonce_begin..nonce_end + 1 {
			// The order created when message was accepted, so we can always get the order info.
			if let Some(order) = <Orders<T, I>>::get(&(lane_id, message_nonce)) {
				let mut reward_item = RewardItem::new();

				let (delivery_and_confirm_reward, treasury_reward) = match order.confirmed_info() {
					// When the order is confirmed at the first slot, no assigned relayers will be
					// not slashed in this case. The total reward to the message deliver relayer and
					// message confirm relayer is the confirmed slot price(first slot price), the
					// guarding relayers would be rewarded with the 20% remaining order_fee, and all
					// the guarding relayers share the guarding_rewards equally. Finally, the
					// remaining the order_fee goes to the treasury.
					Some((slot_index, slot_price)) if slot_index == 0 => {
						let mut order_remain_fee = order.fee().saturating_sub(slot_price);
						let guarding_rewards =
							T::GuardingRelayersRewardRatio::get() * order_remain_fee;

						// All assigned relayers successfully guarded in this case, no slash
						// happens, just calculate the guarding relayers rewards.
						let guarding_relayers_list: Vec<_> =
							order.assigned_relayers_slice().iter().map(|r| r.id.clone()).collect();
						let average_reward = guarding_rewards
							.checked_div(&(guarding_relayers_list.len()).unique_saturated_into())
							.unwrap_or_default();
						for id in guarding_relayers_list {
							reward_item.to_assigned_relayers.insert(id.clone(), average_reward);
							order_remain_fee = order_remain_fee.saturating_sub(average_reward);
						}

						(slot_price, Some(order_remain_fee))
					},
					// When the order is confirmed not at the first slot but within the deadline,
					// some other assigned relayers will be slashed in this case. The total reward
					// to the message deliver relayer and message confirm relayer is the confirmed
					// slot price(first slot price) + other_assigned_relayers_slash part, the
					// guarding relayers would be rewarded with the 20% remaining order_fee, and all
					// the guarding relayers share the guarding_rewards equally. Finally, the
					// remaining the order_fee goes to the treasury.
					Some((slot_index, slot_price)) if slot_index >= 1 => {
						let mut order_remain_fee = order.fee().saturating_sub(slot_price);
						let guarding_rewards =
							T::GuardingRelayersRewardRatio::get() * order_remain_fee;

						// Since part of the assigned relayers successfully guarded, calculate the
						// guarding relayers slash part first.
						let mut slashed_relayers_list: Vec<T::AccountId> =
							order.assigned_relayers_slice().iter().map(|r| r.id.clone()).collect();
						let guarding_relayers_list = slashed_relayers_list.split_off(slot_index);

						// Calculate the assigned relayers slash part
						let mut other_assigned_relayers_slash = BalanceOf::<T, I>::zero();
						for r in slashed_relayers_list {
							let amount = slash_assigned_relayer::<T, I>(
								&order,
								&r,
								relayer_fund_account,
								T::AssignedRelayerSlashRatio::get()
									* Pallet::<T, I>::relayer_locked_collateral(&r),
							);
							other_assigned_relayers_slash += amount;
						}

						// Calculate the guarding relayers rewards
						let average_reward = guarding_rewards
							.checked_div(&(guarding_relayers_list.len()).unique_saturated_into())
							.unwrap_or_default();
						for id in guarding_relayers_list {
							reward_item.to_assigned_relayers.insert(id.clone(), average_reward);
							order_remain_fee = order_remain_fee.saturating_sub(average_reward);
						}

						let delivery_and_confirm_reward =
							slot_price.saturating_add(other_assigned_relayers_slash);
						(delivery_and_confirm_reward, Some(order_remain_fee))
					},
					// When the order is confirmed delayer, all assigned relayers will be slashed in
					// this case. So, no confirmed slot price here. All reward will distribute to
					// the message deliver relayer and message confirm relayer. No guarding rewards
					// and treasury reward.
					_ => {
						let mut other_assigned_relayers_slash = BalanceOf::<T, I>::zero();
						for r in order.assigned_relayers_slice() {
							// 1. For the fixed part
							let mut slash_amount = T::AssignedRelayerSlashRatio::get()
								* Pallet::<T, I>::relayer_locked_collateral(&r.id);

							// 2. For the dynamic part
							slash_amount += T::Slasher::cal_slash_amount(
								order.locked_collateral,
								order.comfirm_delay().unwrap_or_default(),
							);

							// The total_slash_amount can't be greater than the slash_protect.
							if let Some(slash_protect) = Pallet::<T, I>::collateral_slash_protect()
							{
								// slash_amount = sp_std::cmp::min(slash_amount, slash_protect);
								slash_amount = slash_amount.min(slash_protect);
							}

							// The total_slash_amount can't be greater than the locked_collateral.
							let locked_collateral =
								Pallet::<T, I>::relayer_locked_collateral(&r.id);
							slash_amount = sp_std::cmp::min(slash_amount, locked_collateral);

							let amount = slash_assigned_relayer::<T, I>(
								&order,
								&r.id,
								relayer_fund_account,
								slash_amount,
							);
							other_assigned_relayers_slash += amount;
						}

						let delivery_and_confirm_reward =
							order.fee().saturating_add(other_assigned_relayers_slash);

						(delivery_and_confirm_reward, None)
					},
				};

				if let Some(treasury_reward) = treasury_reward {
					reward_item.to_treasury = Some(treasury_reward);
				}

				let deliver_rd = T::MessageRelayersRewardRatio::get() * delivery_and_confirm_reward;
				let confirm_rd = T::ConfirmRelayersRewardRatio::get() * delivery_and_confirm_reward;
				reward_item.to_message_relayer = Some((entry.relayer.clone(), deliver_rd));
				reward_item.to_confirm_relayer = Some((confirm_relayer.clone(), confirm_rd));

				Pallet::<T, I>::deposit_event(Event::OrderReward(
					lane_id,
					message_nonce,
					reward_item.clone(),
				));

				rewards_book.add_reward_item(reward_item);
			}
		}
	}
	rewards_book
}

/// Slash the assigned relayer and emit the slash report.
///
/// fund_account refers to the user who pays the cross-chain fee to this account when creating an
/// order. The slash part will be transferred to fund_account first, and then distributed to various
/// relayers.
pub(crate) fn slash_assigned_relayer<T: Config<I>, I: 'static>(
	order: &Order<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
	who: &T::AccountId,
	fund_account: &T::AccountId,
	amount: BalanceOf<T, I>,
) -> BalanceOf<T, I> {
	let locked_collateral = Pallet::<T, I>::relayer_locked_collateral(who);
	T::Currency::remove_lock(T::LockId::get(), who);
	debug_assert!(
		locked_collateral >= amount,
		"The locked collateral must alway greater than slash max"
	);

	let pay_result = <T as Config<I>>::Currency::transfer(
		who,
		fund_account,
		amount,
		ExistenceRequirement::AllowDeath,
	);
	let report = SlashReport::new(&order, who.clone(), amount);
	match pay_result {
		Ok(_) => {
			crate::Pallet::<T, I>::update_relayer_after_slash(
				who,
				locked_collateral.saturating_sub(amount),
				report,
			);
			log::trace!("Slash {:?} amount: {:?}", who, amount);
			return amount;
		},
		Err(e) => {
			crate::Pallet::<T, I>::update_relayer_after_slash(who, locked_collateral, report);
			log::error!("Slash {:?} amount {:?}, err {:?}", who, amount, e)
		},
	}

	BalanceOf::<T, I>::zero()
}

/// Do reward
pub(crate) fn do_reward<T: Config<I>, I: 'static>(
	from: &T::AccountId,
	to: &T::AccountId,
	reward: BalanceOf<T, I>,
) {
	if reward.is_zero() {
		return;
	}

	let pay_result = <T as Config<I>>::Currency::transfer(
		from,
		to,
		reward,
		// the relayer fund account must stay above ED (needs to be pre-funded)
		ExistenceRequirement::KeepAlive,
	);

	match pay_result {
		Ok(_) => log::trace!("Reward, from {:?} to {:?} reward: {:?}", from, to, reward),
		Err(e) => log::error!("Reward, from {:?} to {:?} reward {:?}: {:?}", from, to, reward, e,),
	}
}

/// Record the concrete reward distribution of certain order
#[derive(Clone, Debug, Encode, Decode, Eq, PartialEq, TypeInfo)]
pub struct RewardItem<AccountId, Balance> {
	pub to_assigned_relayers: BTreeMap<AccountId, Balance>,
	pub to_treasury: Option<Balance>,
	pub to_message_relayer: Option<(AccountId, Balance)>,
	pub to_confirm_relayer: Option<(AccountId, Balance)>,
}

impl<AccountId, Balance> RewardItem<AccountId, Balance> {
	fn new() -> Self {
		Self {
			to_assigned_relayers: BTreeMap::new(),
			to_treasury: None,
			to_message_relayer: None,
			to_confirm_relayer: None,
		}
	}
}

/// Record the calculation rewards result
#[derive(Clone, Debug, Eq, PartialEq, TypeInfo)]
pub struct RewardsBook<T: Config<I>, I: 'static> {
	pub deliver_sum: BTreeMap<T::AccountId, BalanceOf<T, I>>,
	pub confirm_sum: BalanceOf<T, I>,
	pub assigned_relayers_sum: BTreeMap<T::AccountId, BalanceOf<T, I>>,
	pub treasury_sum: BalanceOf<T, I>,
}

impl<T: Config<I>, I: 'static> RewardsBook<T, I> {
	fn new() -> Self {
		Self {
			deliver_sum: BTreeMap::new(),
			confirm_sum: BalanceOf::<T, I>::zero(),
			assigned_relayers_sum: BTreeMap::new(),
			treasury_sum: BalanceOf::<T, I>::zero(),
		}
	}

	fn add_reward_item(&mut self, item: RewardItem<T::AccountId, BalanceOf<T, I>>) {
		for (k, v) in item.to_assigned_relayers.iter() {
			self.assigned_relayers_sum
				.entry(k.clone())
				.and_modify(|r| *r = r.saturating_add(v.clone()))
				.or_insert(*v);
		}

		if let Some(reward) = item.to_treasury {
			self.treasury_sum = self.treasury_sum.saturating_add(reward);
		}

		if let Some((id, reward)) = item.to_message_relayer {
			self.deliver_sum
				.entry(id)
				.and_modify(|r| *r = r.saturating_add(reward))
				.or_insert(reward);
		}

		if let Some((_id, reward)) = item.to_confirm_relayer {
			self.confirm_sum = self.confirm_sum.saturating_add(reward);
		}
	}
}
