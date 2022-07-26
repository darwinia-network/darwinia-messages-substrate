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
use sp_runtime::traits::{AccountIdConversion, Saturating, Zero};
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
			slash_and_calculate_rewards::<T, I>(
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

/// Slash and calculate rewards for messages_relayers, confirmation relayers, treasury_sum,
/// assigned_relayers
pub fn slash_and_calculate_rewards<T, I>(
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
				// The confirm_time of the order is already set in the `OnDeliveryConfirmed`
				// callback. the callback function was called as source chain received message
				// delivery proof, before the reward payment.
				let confirmed_at =
					order.confirm_time.unwrap_or_else(|| frame_system::Pallet::<T>::block_number());

				let mut total_reward = order.fee();
				let mut reward_item = RewardItem::new();
				match order.confirmed_assigned_relayer_info(confirmed_at) {
					// The order has been confirmed at the first assigned relayers slot, there is
					// only reward, no slash happens.The total_reward all comes from the cross-chain
					// fee paid by the user.
					Some((assigned_relayer_index, assigned_relayer_id, assigned_relayer_fee))
						if assigned_relayer_index == 0 =>
					{
						cal_rewards_before_deadline::<T, I>(
							&assigned_relayer_id,
							&entry.relayer,
							&confirm_relayer,
							total_reward,
							assigned_relayer_fee,
							&mut reward_item,
						);
					},
					// Since the slot whose order is confirmed is not the first one, it is
					// necessary to obtain the assigned relayers corresponding to all the
					// previous slots. The slash consists only one fixed part: a
					// fixed percentage of locked collateral is deducted.
					// The total_reward is the sum of the cross-chain fee paid by the user and the
					// slash part.
					Some((assigned_relayer_index, assigned_relayer_id, assigned_relayer_fee))
						if assigned_relayer_index >= 1 =>
					{
						// Obtain the previous assigned relayers
						let previous_assigned_relayers = order
							.assigned_relayers_slice()
							.into_iter()
							.take(assigned_relayer_index)
							.collect::<Vec<_>>();

						// Calculate the slash part
						for r in previous_assigned_relayers {
							let amount = slash_assigned_relayer::<T, I>(
								&order,
								&r.id,
								relayer_fund_account,
								T::AssignedRelayerSlashRatio::get()
									* Pallet::<T, I>::relayer_locked_collateral(&r.id),
							);
							total_reward += amount;
						}

						cal_rewards_before_deadline::<T, I>(
							&assigned_relayer_id,
							&entry.relayer,
							&confirm_relayer,
							total_reward,
							assigned_relayer_fee,
							&mut reward_item,
						);
					},
					// When the order is delayed and confirmed, all assigned relayers responsible
					// for the order relay will be slashed. The slash consists of two parts.
					// 1. For the fixed part, a fixed percentage of locked collateral is deducted.
					// 2. For the dynamic part, calculated by Slasher based on confirmed delay.
					// The total_reward is the sum of the cross-chain fee paid by the user and the
					// slash part.
					_ => {
						for assigned_relayer in order.assigned_relayers_slice() {
							// 1. For the fixed part
							let mut total_slash_amount = T::AssignedRelayerSlashRatio::get()
								* Pallet::<T, I>::relayer_locked_collateral(&assigned_relayer.id);

							// 2. For the dynamic part
							total_slash_amount += T::Slasher::cal_slash_amount(
								order.locked_collateral,
								order.comfirm_delay().unwrap_or_default(),
							);

							// The total_slash_amount can't be greater than the slash_protect.
							if let Some(slash_protect) = Pallet::<T, I>::collateral_slash_protect()
							{
								total_slash_amount =
									sp_std::cmp::min(total_slash_amount, slash_protect);
							}

							// The total_slash_amount can't be greater than the locked_collateral.
							let locked_collateral =
								Pallet::<T, I>::relayer_locked_collateral(&assigned_relayer.id);
							total_slash_amount =
								sp_std::cmp::min(total_slash_amount, locked_collateral);

							let slashed_amount = slash_assigned_relayer::<T, I>(
								&order,
								&assigned_relayer.id,
								relayer_fund_account,
								total_slash_amount,
							);
							total_reward += slashed_amount;
						}

						cal_reward_after_deadline::<T, I>(
							&entry.relayer,
							&confirm_relayer,
							total_reward,
							&mut reward_item,
						);
					},
				}

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

/// Calculate the reward for the order which has been confirmed in time.
pub(crate) fn cal_rewards_before_deadline<T: Config<I>, I: 'static>(
	assigned_relayer_id: &T::AccountId,
	message_relayer_id: &T::AccountId,
	confirm_relayer_id: &T::AccountId,
	total_reward: BalanceOf<T, I>,
	assigned_relayer_fee: BalanceOf<T, I>,
	reward_item: &mut RewardItem<T::AccountId, BalanceOf<T, I>>,
) {
	// total_reward - assigned_relayer_fee => treasury
	reward_item.to_treasury = Some(total_reward.saturating_sub(assigned_relayer_fee));

	// AssignedRelayersRewardRatio * assigned_relayer_fee => slot_relayer
	let slot_relayer_reward = T::AssignedRelayersRewardRatio::get() * assigned_relayer_fee;
	reward_item.to_assigned_relayer = Some((assigned_relayer_id.clone(), slot_relayer_reward));

	let message_and_confirm_reward = assigned_relayer_fee.saturating_sub(slot_relayer_reward);
	// MessageRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * assigned_relayer_fee
	// => message_relayer
	let message_reward = T::MessageRelayersRewardRatio::get() * message_and_confirm_reward;
	// ConfirmRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * assigned_relayer_fee
	// => confirm_relayer
	let confirm_reward = T::ConfirmRelayersRewardRatio::get() * message_and_confirm_reward;

	reward_item.to_message_relayer = Some((message_relayer_id.clone(), message_reward));
	reward_item.to_confirm_relayer = Some((confirm_relayer_id.clone(), confirm_reward));
}

/// Calculate the reward for the order which has been confirmed out of deadline.
pub(crate) fn cal_reward_after_deadline<T: Config<I>, I: 'static>(
	message_relayer_id: &T::AccountId,
	confirm_relayer_id: &T::AccountId,
	total_reward: BalanceOf<T, I>,
	reward_item: &mut RewardItem<T::AccountId, BalanceOf<T, I>>,
) {
	// MessageRelayersRewardRatio total_reward => message relayer
	let message_reward = T::MessageRelayersRewardRatio::get() * total_reward;
	// ConfirmRelayersRewardRatio total_reward => confirm relayer
	let confirm_reward = T::ConfirmRelayersRewardRatio::get() * total_reward;

	reward_item.to_message_relayer = Some((message_relayer_id.clone(), message_reward));
	reward_item.to_confirm_relayer = Some((confirm_relayer_id.clone(), confirm_reward));
}

/// Slash the assigned relayer and emit the slash report.
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
	pub to_assigned_relayer: Option<(AccountId, Balance)>,
	pub to_treasury: Option<Balance>,
	pub to_message_relayer: Option<(AccountId, Balance)>,
	pub to_confirm_relayer: Option<(AccountId, Balance)>,
}

impl<AccountId, Balance> RewardItem<AccountId, Balance> {
	fn new() -> Self {
		Self {
			to_assigned_relayer: None,
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
		if let Some((id, reward)) = item.to_assigned_relayer {
			self.assigned_relayers_sum
				.entry(id)
				.and_modify(|r| *r = r.saturating_add(reward))
				.or_insert(reward);
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
