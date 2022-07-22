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
				// The confirm_time of the order is set in the `OnDeliveryConfirmed` callback. And
				// the callback function was called as source chain received message delivery proof,
				// before the reward payment.
				let order_confirm_time =
					order.confirm_time.unwrap_or_else(|| frame_system::Pallet::<T>::block_number());

				let mut extra_slash_amount = BalanceOf::<T, I>::zero();

				match order.required_delivery_relayer_for_time(order_confirm_time) {
					Some((index, slot_id, base_fee)) if index == 0 => {
						// finished in the first slot
					},
					Some((index, slot_id, base_fee)) if index >= 1 => {
						let lazy_relayers =
							order.relayers_slice().into_iter().take(index - 1).collect::<Vec<_>>();

						// Slash 20%
						for relayer in lazy_relayers {
							let locked_collateral =
								Pallet::<T, I>::relayer_locked_collateral(&relayer.id);
							T::Currency::remove_lock(T::LockId::get(), &relayer.id);
							let amount = T::AssignedRelayerSlashRatio::get() * locked_collateral;

							let pay_result = <T as Config<I>>::Currency::transfer(
								&relayer.id,
								relayer_fund_account,
								amount,
								ExistenceRequirement::AllowDeath,
							);
							match pay_result {
								Ok(_) => {
									// crate::Pallet::<T, I>::update_relayer_after_slash(
									// 	relayer,
									// 	locked_collateral.saturating_sub(slash_amount),
									// 	report,
									// );
									log::trace!("Slash {:?} amount: {:?}", relayer, amount);
									// return amount;
								},
								Err(e) => {
									// crate::Pallet::<T, I>::update_relayer_after_slash(
									// 	who,
									// 	locked_collateral,
									// 	report,
									// );
									log::error!(
										"Slash {:?} amount {:?}, err {:?}",
										relayer,
										amount,
										e
									)
								},
							}
						}
					},
					Some(_) => {
						todo!()
					},
					None => {
						todo!()
					},
				}

				let message_fee = order.fee() + extra_slash_amount;

				let mut reward_item = RewardItem::new();
				let message_reward;
				let confirm_reward;

				if let Some((index, who, base_fee)) =
					order.required_delivery_relayer_for_time(order_confirm_time)
				{
					// message fee - base fee => treasury_sum
					reward_item.to_treasury = Some(message_fee.saturating_sub(base_fee));

					// AssignedRelayersRewardRatio * base fee => slot relayer
					let slot_relayer_reward = T::AssignedRelayersRewardRatio::get() * base_fee;
					reward_item.to_slot_relayer = Some((who, slot_relayer_reward));

					let bridger_relayers_reward = base_fee.saturating_sub(slot_relayer_reward);
					// MessageRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * base_fee =>
					// message relayer
					message_reward = T::MessageRelayersRewardRatio::get() * bridger_relayers_reward;
					// ConfirmRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * base_fee =>
					// confirm relayer
					confirm_reward = T::ConfirmRelayersRewardRatio::get() * bridger_relayers_reward;
				} else {
					// The order delivery is delay, slash occurs.
					let mut total_slash = message_fee;

					// calculate slash amount
					let mut amount: BalanceOf<T, I> = T::Slasher::cal_slash_amount(
						order.locked_collateral,
						order.delivery_delay().unwrap_or_default(),
					);
					if let Some(slash_protect) = Pallet::<T, I>::collateral_slash_protect() {
						amount = sp_std::cmp::min(amount, slash_protect);
					}

					// Slash order's assigned relayers
					let mut assigned_relayers_slash = BalanceOf::<T, I>::zero();
					for assigned_relayer in order.relayers_slice() {
						let report = SlashReport::new(&order, assigned_relayer.id.clone(), amount);
						let slashed = do_slash::<T, I>(
							&assigned_relayer.id,
							relayer_fund_account,
							amount,
							report,
						);
						assigned_relayers_slash += slashed;
					}
					total_slash += assigned_relayers_slash;

					// MessageRelayersRewardRatio total slash => message relayer
					message_reward = T::MessageRelayersRewardRatio::get() * total_slash;
					// ConfirmRelayersRewardRatio total slash => confirm relayer
					confirm_reward = T::ConfirmRelayersRewardRatio::get() * total_slash;
				}

				reward_item.to_message_relayer = Some((entry.clone().relayer, message_reward));
				reward_item.to_confirm_relayer = Some((confirm_relayer.clone(), confirm_reward));

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

/// Do slash for absent assigned relayers
pub(crate) fn do_slash<T: Config<I>, I: 'static>(
	who: &T::AccountId,
	fund_account: &T::AccountId,
	amount: BalanceOf<T, I>,
	report: SlashReport<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
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
	pub to_slot_relayer: Option<(AccountId, Balance)>,
	pub to_treasury: Option<Balance>,
	pub to_message_relayer: Option<(AccountId, Balance)>,
	pub to_confirm_relayer: Option<(AccountId, Balance)>,
}

impl<AccountId, Balance> RewardItem<AccountId, Balance> {
	fn new() -> Self {
		Self {
			to_slot_relayer: None,
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
		if let Some((id, reward)) = item.to_slot_relayer {
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
