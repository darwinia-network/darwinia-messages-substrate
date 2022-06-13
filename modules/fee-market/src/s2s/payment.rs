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
		let (treasury_sum, slot_relayers_sum, messages_relayers_sum, confirmation_sum) =
			slash_and_calculate_rewards::<T, I>(
				lane_id,
				messages_relayers,
				received_range,
				relayer_fund_account,
			);

		// Pay treasury_sum reward
		do_reward::<T, I>(
			relayer_fund_account,
			&T::TreasuryPalletId::get().into_account(),
			treasury_sum,
		);

		// Pay assign relayer reward
		for (relayer, reward) in slot_relayers_sum {
			do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		}

		// Pay messages relayers rewards
		for (relayer, reward) in messages_relayers_sum {
			do_reward::<T, I>(relayer_fund_account, &relayer, reward);
		}

		// Pay confirmation relayer rewards
		do_reward::<T, I>(relayer_fund_account, confirmation_relayer, confirmation_sum);
	}
}

/// Slash and calculate rewards for messages_relayers, confirmation relayers, treasury_sum,
/// assigned_relayers
pub fn slash_and_calculate_rewards<T, I>(
	lane_id: LaneId,
	messages_relayers: VecDeque<UnrewardedRelayer<T::AccountId>>,
	received_range: &RangeInclusive<MessageNonce>,
	relayer_fund_account: &T::AccountId,
) -> (
	BalanceOf<T, I>,
	BTreeMap<T::AccountId, BalanceOf<T, I>>,
	BTreeMap<T::AccountId, BalanceOf<T, I>>,
	BalanceOf<T, I>,
)
where
	T: frame_system::Config + Config<I>,
	I: 'static,
{
	let mut treasury_sum = BalanceOf::<T, I>::zero();
	let mut slot_relayers_sum = BTreeMap::new();
	let mut messages_relayers_sum = BTreeMap::new();
	let mut confirmation_sum = BalanceOf::<T, I>::zero();

	for messages_relayer in messages_relayers {
		let nonce_begin =
			sp_std::cmp::max(messages_relayer.messages.begin, *received_range.start());
		let nonce_end = sp_std::cmp::min(messages_relayer.messages.end, *received_range.end());

		for message_nonce in nonce_begin..nonce_end + 1 {
			// The order created when message was accepted, so we can always get the order info.
			if let Some(order) = <Orders<T, I>>::get(&(lane_id, message_nonce)) {
				// 1. Calc rewards of this order
				let (
					to_treasury,
					to_slot_relayer,
					to_messages_relayer,
					to_confirmation_relayer
				) = slash_and_calc_rewards_for_order::<T, I>(relayer_fund_account, &order);

				// 2. Aggregate by category
				if let Some(to_treasury) = to_treasury {
					treasury_sum = treasury_sum.saturating_add(to_treasury);
				}

				if let Some((who, slot_relayer_reward)) = to_slot_relayer.clone() {
					slot_relayers_sum
						.entry(who)
						.and_modify(|r: &mut BalanceOf<T, I>| {
							*r = r.saturating_add(slot_relayer_reward)
						})
						.or_insert(slot_relayer_reward);
				}

				messages_relayers_sum
					.entry(messages_relayer.clone().relayer)
					.and_modify(|r: &mut BalanceOf<T, I>| *r = r.saturating_add(to_messages_relayer))
					.or_insert(to_messages_relayer);

				confirmation_sum = confirmation_sum.saturating_add(to_confirmation_relayer);

				// 3. Emit a OrderReward event
				Pallet::<T, I>::deposit_event(Event::OrderReward(
					lane_id,
					message_nonce,
					to_treasury,
					to_slot_relayer,
					(messages_relayer.clone().relayer, to_messages_relayer),
					to_confirmation_relayer,
				));
			}
		}
	}

	(treasury_sum, slot_relayers_sum, messages_relayers_sum, confirmation_sum)
}

fn slash_and_calc_rewards_for_order<T, I>(
	relayer_fund_account: &T::AccountId,
	order: &Order<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
) -> (Option<BalanceOf<T, I>>, Option<(T::AccountId, BalanceOf<T, I>)>, BalanceOf<T, I>, BalanceOf<T, I>)
where
	T: frame_system::Config + Config<I>,
	I: 'static,
{
	let mut to_treasury = None;
	let mut to_slot_relayer = None;
	let to_messages_relayer;
	let to_confirmation_relayer;

	if let Some((who, base_fee)) =
		order.confirmed_by_prior_relayer_on_time(frame_system::Pallet::<T>::block_number())
	{
		// message fee - base fee => treasury_sum
		to_treasury = Some(order.fee().saturating_sub(base_fee));

		// AssignedRelayersRewardRatio * base fee => slot relayer
		let slot_relayer_reward = T::AssignedRelayersRewardRatio::get() * base_fee;
		to_slot_relayer = Some((who.clone(), slot_relayer_reward));

		let bridger_relayers_reward = base_fee.saturating_sub(slot_relayer_reward);
		// MessageRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * base_fee =>
		// message relayer
		to_messages_relayer = T::MessageRelayersRewardRatio::get() * bridger_relayers_reward;

		// ConfirmRelayersRewardRatio * (1 - AssignedRelayersRewardRatio) * base_fee =>
		// confirm relayer
		to_confirmation_relayer = T::ConfirmRelayersRewardRatio::get() * bridger_relayers_reward;
	} else {
		let total_slash = slash::<T, I>(relayer_fund_account, &order);

		// MessageRelayersRewardRatio total slash => message relayer
		to_messages_relayer = T::MessageRelayersRewardRatio::get() * total_slash;
		// ConfirmRelayersRewardRatio total slash => confirm relayer
		to_confirmation_relayer = T::ConfirmRelayersRewardRatio::get() * total_slash;
	}
	(to_treasury, to_slot_relayer, to_messages_relayer, to_confirmation_relayer)
}

fn slash<T, I>(
	relayer_fund_account: &T::AccountId,
	order: &Order<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
) -> BalanceOf<T, I>
where
	T: Config<I>,
	I: 'static,
{
	// The order delivery is delay, slash occurs.
	let mut total_slash = order.fee();

	// calculate slash amount
	let mut amount: BalanceOf<T, I> =
		T::Slasher::slash(order.locked_collateral, order.delivery_delay().unwrap_or_default());
	if let Some(slash_protect) = Pallet::<T, I>::collateral_slash_protect() {
		amount = sp_std::cmp::min(amount, slash_protect);
	}

	// Slash order's assigned relayers
	let mut assigned_relayers_slash = BalanceOf::<T, I>::zero();
	for assigned_relayer in order.relayers_slice() {
		let report = SlashReport::new(&order, assigned_relayer.id.clone(), amount);
		let slashed = do_slash::<T, I>(&assigned_relayer.id, relayer_fund_account, amount, report);
		assigned_relayers_slash += slashed;
	}
	total_slash += assigned_relayers_slash;
	total_slash
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
