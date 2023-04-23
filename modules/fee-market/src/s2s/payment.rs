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

// crates.io
use scale_info::TypeInfo;
// darwinia-network
use crate::{Config, Orders, Pallet, *};
use bp_messages::{source_chain::DeliveryConfirmationPayments, MessageNonce, UnrewardedRelayer};
// --- paritytech ---
use frame_support::{
	log,
	traits::{Currency as CurrencyT, ExistenceRequirement, Get},
};
use sp_runtime::traits::{CheckedDiv, Saturating, UniqueSaturatedInto, Zero};
use sp_std::{
	cmp::{max, min},
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	ops::RangeInclusive,
};

pub struct FeeMarketPayment<T, I, Currency> {
	_phantom: sp_std::marker::PhantomData<(T, I, Currency)>,
}

impl<T, I, Currency> DeliveryConfirmationPayments<T::AccountId> for FeeMarketPayment<T, I, Currency>
where
	T: frame_system::Config + Config<I>,
	I: 'static,
	Currency: CurrencyT<T::AccountId>,
{
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		_messages_relayers: VecDeque<UnrewardedRelayer<T::AccountId>>,
		_confirmation_relayer: &T::AccountId,
		_received_range: &RangeInclusive<MessageNonce>,
	) {
		unimplemented!("TODO")
		// let rewards_items = calculate_rewards::<T, I>(
		// 	lane_id,
		// 	messages_relayers,
		// 	confirmation_relayer.clone(),
		// 	received_range,
		// );

		// let mut deliver_sum = BTreeMap::<T::AccountId, BalanceOf<T, I>>::new();
		// let mut confirm_sum = BalanceOf::<T, I>::zero();
		// let mut assigned_relayers_sum = BTreeMap::<T::AccountId, BalanceOf<T, I>>::new();
		// let mut treasury_sum = BalanceOf::<T, I>::zero();
		// for item in rewards_items {
		// 	for (k, v) in item.to_assigned_relayers.iter() {
		// 		assigned_relayers_sum
		// 			.entry(k.clone())
		// 			.and_modify(|r| *r = r.saturating_add(*v))
		// 			.or_insert(*v);
		// 	}

		// 	if let Some(reward) = item.to_treasury {
		// 		treasury_sum = treasury_sum.saturating_add(reward);
		// 	}

		// 	if let Some((id, reward)) = item.to_message_relayer {
		// 		deliver_sum
		// 			.entry(id)
		// 			.and_modify(|r| *r = r.saturating_add(reward))
		// 			.or_insert(reward);
		// 	}

		// 	if let Some((_id, reward)) = item.to_confirm_relayer {
		// 		confirm_sum = confirm_sum.saturating_add(reward);
		// 	}
		// }

		// // Pay rewards to the message confirm relayer
		// do_reward::<T, I>(relayer_fund_account, confirmation_relayer, confirm_sum);
		// // Pay rewards to the messages deliver relayers
		// for (relayer, reward) in deliver_sum {
		// 	do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		// }
		// // Pay rewards to the assigned relayers
		// for (relayer, reward) in assigned_relayers_sum {
		// 	do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		// }
		// // Pay to treasury
		// do_reward::<T, I>(
		// 	relayer_fund_account,
		// 	&T::TreasuryPalletId::get().into_account_truncating(),
		// 	treasury_sum,
		// );
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
) -> Vec<RewardItem<T::AccountId, BalanceOf<T, I>>>
where
	T: frame_system::Config + Config<I>,
	I: 'static,
{
	let mut rewards_items = Vec::new();
	for entry in messages_relayers {
		let nonce_begin = max(entry.messages.begin, *received_range.start());
		let nonce_end = min(entry.messages.end, *received_range.end());

		for message_nonce in nonce_begin..nonce_end + 1 {
			// The order created when message was accepted, so we can always get the order info.
			if let Some(order) = <Orders<T, I>>::get(&(lane_id, message_nonce)) {
				let mut reward_item = RewardItem::new();
				let order_collater = order.collateral_per_assigned_relayer;

				let (message_reward, treasury_reward) = match order.confirmed_info() {
					// When the order is confirmed at the first slot, no assigned relayers will be
					// not slashed in this case. The total reward to the message deliver relayer and
					// message confirm relayer is the confirmed slot price(first slot price), the
					// duty relayers would be rewarded with the 20% of the message fee, and all
					// the duty relayers share the duty_rewards equally. Finally, the
					// surplus of the message fee goes to the treasury.
					Some((slot_index, slot_price)) if slot_index == 0 => {
						let mut message_surplus = order.fee().saturating_sub(slot_price);
						let slot_duty_rewards = T::DutyRelayersRewardRatio::get() * message_surplus;

						// All assigned relayers successfully are on duty in this case, no slash
						// happens, just calculate the duty relayers rewards.
						let duty_relayers: Vec<_> =
							order.assigned_relayers_slice().iter().map(|r| r.id.clone()).collect();
						let average_reward = slot_duty_rewards
							.checked_div(&(duty_relayers.len()).unique_saturated_into())
							.unwrap_or_default();
						for id in duty_relayers {
							reward_item.to_assigned_relayers.insert(id.clone(), average_reward);
							message_surplus = message_surplus.saturating_sub(average_reward);
						}

						(slot_price, Some(message_surplus))
					},
					// When the order is confirmed not at the first slot but within the deadline,
					// some other assigned relayers will be slashed in this case. The total reward
					// to the message deliver relayer and message confirm relayer is the confirmed
					// slot price(first slot price) + slot_offensive_slash part, the
					// duty relayers would be rewarded with the 20% of the message surplus, and all
					// the duty relayers share the duty_rewards equally. Finally, the
					// surplus of the message fee goes to the treasury.
					Some((slot_index, slot_price)) if slot_index >= 1 => {
						let mut message_surplus = order.fee().saturating_sub(slot_price);
						let slot_duty_rewards = T::DutyRelayersRewardRatio::get() * message_surplus;

						// Since part of the assigned relayers are on duty, calculate the duty
						// relayers slash part first.
						let mut offensive_relayers: Vec<_> =
							order.assigned_relayers_slice().iter().map(|r| r.id.clone()).collect();
						let duty_relayers = offensive_relayers.split_off(slot_index);

						// Calculate the assigned relayers slash part
						let mut slot_offensive_slash = BalanceOf::<T, I>::zero();
						for r in offensive_relayers {
							let amount = slash_assigned_relayer::<T, I>(
								&order,
								&r,
								relayer_fund_account,
								T::AssignedRelayerSlashRatio::get() * order_collater,
							);
							slot_offensive_slash += amount;
						}

						// Calculate the duty relayers rewards
						let average_reward = slot_duty_rewards
							.checked_div(&(duty_relayers.len()).unique_saturated_into())
							.unwrap_or_default();
						for id in duty_relayers {
							reward_item.to_assigned_relayers.insert(id.clone(), average_reward);
							message_surplus = message_surplus.saturating_sub(average_reward);
						}

						(slot_price.saturating_add(slot_offensive_slash), Some(message_surplus))
					},
					// When the order is confirmed delayer, all assigned relayers will be slashed in
					// this case. So, no confirmed slot price here. All reward will distribute to
					// the message deliver relayer and message confirm relayer. No duty rewards
					// and treasury reward.
					_ => {
						let mut slot_offensive_slash = BalanceOf::<T, I>::zero();
						for r in order.assigned_relayers_slice() {
							// The fixed part
							let mut total = T::AssignedRelayerSlashRatio::get() * order_collater;
							// The dynamic part
							total += T::Slasher::calc_amount(
								order_collater,
								order.comfirm_delay().unwrap_or_default(),
							);

							// The total_slash_amount can't be greater than the slash_protect.
							if let Some(slash_protect) = Pallet::<T, I>::collateral_slash_protect()
							{
								total = total.min(slash_protect);
							}

							let actual_amount = slash_assigned_relayer::<T, I>(
								&order,
								&r.id,
								relayer_fund_account,
								total,
							);
							slot_offensive_slash += actual_amount;
						}

						(order.fee().saturating_add(slot_offensive_slash), None)
					},
				};

				if let Some(treasury_reward) = treasury_reward {
					reward_item.to_treasury = Some(treasury_reward);
				}

				let deliver_rd = T::MessageRelayersRewardRatio::get() * message_reward;
				let confirm_rd = T::ConfirmRelayersRewardRatio::get() * message_reward;
				reward_item.to_message_relayer = Some((entry.relayer.clone(), deliver_rd));
				reward_item.to_confirm_relayer = Some((confirm_relayer.clone(), confirm_rd));

				Pallet::<T, I>::deposit_event(Event::OrderReward(
					lane_id,
					message_nonce,
					reward_item.clone(),
				));

				rewards_items.push(reward_item);
			}
		}
	}
	rewards_items
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
	let slash_amount = amount.min(order.collateral_per_assigned_relayer);

	T::Currency::remove_lock(T::LockId::get(), who);
	let pay_result = <T as Config<I>>::Currency::transfer(
		who,
		fund_account,
		slash_amount,
		ExistenceRequirement::AllowDeath,
	);

	let locked_collateral = Pallet::<T, I>::relayer_locked_collateral(who);
	let report = SlashReport::new(order, who.clone(), slash_amount);
	match pay_result {
		Ok(_) => {
			crate::Pallet::<T, I>::update_relayer_after_slash(
				who,
				locked_collateral.saturating_sub(slash_amount),
				report,
			);
			log::trace!("Slash {:?} slash_amount: {:?}", who, slash_amount);
			return slash_amount;
		},
		Err(e) => {
			crate::Pallet::<T, I>::update_relayer_after_slash(who, locked_collateral, report);
			log::error!("Slash {:?} amount {:?}, err {:?}", who, slash_amount, e)
		},
	}

	BalanceOf::<T, I>::zero()
}

/// Do reward
pub(crate) fn _do_reward<T: Config<I>, I: 'static>(
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
