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

//! # Fee Market Pallet

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "128"]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod tests;

pub mod weight;
pub use weight::WeightInfo;

pub mod s2s;
pub mod types;

// --- paritytech ---
use bp_messages::{LaneId, MessageNonce};
use frame_support::{
	ensure,
	pallet_prelude::*,
	traits::{Currency, Get, LockIdentifier, LockableCurrency, WithdrawReasons},
	PalletId,
};
use frame_system::{ensure_signed, pallet_prelude::*};
use sp_runtime::{
	traits::{Saturating, Zero},
	Permill, SaturatedConversion,
};
use sp_std::vec::Vec;
// --- darwinia-network ---
use s2s::RewardItem;
use types::{Order, Relayer, SlashReport};

pub type AccountId<T> = <T as frame_system::Config>::AccountId;
pub type BalanceOf<T, I> = <<T as Config<I>>::Currency as Currency<AccountId<T>>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// Some reward goes to Treasury.
		#[pallet::constant]
		type TreasuryPalletId: Get<PalletId>;
		#[pallet::constant]
		type LockId: Get<LockIdentifier>;

		/// The minimum fee for relaying.
		#[pallet::constant]
		type MinimumRelayFee: Get<BalanceOf<Self, I>>;
		/// The collateral relayer need to lock for each order.
		#[pallet::constant]
		type CollateralPerOrder: Get<BalanceOf<Self, I>>;
		/// The slot times set
		#[pallet::constant]
		type Slot: Get<Self::BlockNumber>;

		/// Reward parameters
		#[pallet::constant]
		type GuardRelayersRewardRatio: Get<Permill>;
		#[pallet::constant]
		type MessageRelayersRewardRatio: Get<Permill>;
		#[pallet::constant]
		type ConfirmRelayersRewardRatio: Get<Permill>;

		/// The slash ratio for assigned relayers.
		#[pallet::constant]
		type AssignedRelayerSlashRatio: Get<Permill>;
		type Slasher: Slasher<Self, I>;

		type Currency: LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>;
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// Relayer enrollment. \[account_id, locked_collateral, relay_fee\]
		Enroll(T::AccountId, BalanceOf<T, I>, BalanceOf<T, I>),
		/// Update relayer locked collateral. \[account_id, new_collateral\]
		UpdateLockedCollateral(T::AccountId, BalanceOf<T, I>),
		/// Update relayer fee. \[account_id, new_fee\]
		UpdateRelayFee(T::AccountId, BalanceOf<T, I>),
		/// Relayer cancel enrollment. \[account_id\]
		CancelEnrollment(T::AccountId),
		/// Update collateral slash protect value. \[slash_protect_value\]
		UpdateCollateralSlashProtect(BalanceOf<T, I>),
		/// Update market assigned relayers numbers. \[new_assigned_relayers_number\]
		UpdateAssignedRelayersNumber(u32),
		/// Slash report
		FeeMarketSlash(SlashReport<T::AccountId, T::BlockNumber, BalanceOf<T, I>>),
		/// Create new order. \[lane_id, message_nonce, order_fee, assigned_relayers,
		/// out_of_slots_time\]
		OrderCreated(
			LaneId,
			MessageNonce,
			BalanceOf<T, I>,
			Vec<T::AccountId>,
			Option<T::BlockNumber>,
		),
		/// Reward distribute of the order. \[lane_id, message_nonce, rewards\]
		OrderReward(LaneId, MessageNonce, RewardItem<T::AccountId, BalanceOf<T, I>>),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Insufficient balance.
		InsufficientBalance,
		/// The relayer has been enrolled.
		AlreadyEnrolled,
		/// This relayer doesn't enroll ever.
		NotEnrolled,
		/// Locked collateral is too low to cover one order.
		CollateralTooLow,
		/// Update locked collateral is not allow since some orders are not confirm.
		StillHasOrdersNotConfirmed,
		/// The fee is lower than MinimumRelayFee.
		RelayFeeTooLow,
		/// The relayer is occupied, and can't cancel enrollment now.
		OccupiedRelayer,
	}

	// Enrolled relayers storage
	#[pallet::storage]
	#[pallet::getter(fn relayer)]
	pub type RelayersMap<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Relayer<T::AccountId, BalanceOf<T, I>>,
		OptionQuery,
	>;
	#[pallet::storage]
	#[pallet::getter(fn relayers)]
	pub type Relayers<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<T::AccountId>, OptionQuery>;

	// Priority relayers storage
	#[pallet::storage]
	#[pallet::getter(fn assigned_relayers)]
	pub type AssignedRelayers<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<Relayer<T::AccountId, BalanceOf<T, I>>>, OptionQuery>;

	// Order storage
	#[pallet::storage]
	#[pallet::getter(fn order)]
	pub type Orders<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Blake2_128Concat,
		(LaneId, MessageNonce),
		Order<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn collateral_slash_protect)]
	pub type CollateralSlashProtect<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BalanceOf<T, I>, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn assigned_relayers_number)]
	pub type AssignedRelayersNumber<T: Config<I>, I: 'static = ()> =
		StorageValue<_, u32, ValueQuery, DefaultAssignedRelayersNumber>;
	#[pallet::type_value]
	pub fn DefaultAssignedRelayersNumber() -> u32 {
		3
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn on_finalize(_: BlockNumberFor<T>) {
			for ((lane_id, message_nonce), order) in <Orders<T, I>>::iter() {
				// Once the order's confirm_time is not None, we consider this order has been
				// rewarded. Hence, clean the storage.
				if order.confirm_time.is_some() {
					<Orders<T, I>>::remove((lane_id, message_nonce));
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Any accounts can enroll to be a relayer by lock collateral. The relay fee is optional,
		/// the default value is MinimumRelayFee in runtime. (Update market needed)
		/// Note: One account can enroll only once.
		#[pallet::weight(<T as Config<I>>::WeightInfo::enroll_and_lock_collateral())]
		pub fn enroll_and_lock_collateral(
			origin: OriginFor<T>,
			lock_collateral: BalanceOf<T, I>,
			relay_fee: Option<BalanceOf<T, I>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(!Self::is_enrolled(&who), <Error<T, I>>::AlreadyEnrolled);
			ensure!(
				T::Currency::free_balance(&who) >= lock_collateral,
				<Error<T, I>>::InsufficientBalance
			);

			ensure!(
				Self::collateral_to_order_capacity(lock_collateral) > 0,
				<Error<T, I>>::CollateralTooLow
			);

			if let Some(fee) = relay_fee {
				ensure!(fee >= T::MinimumRelayFee::get(), <Error<T, I>>::RelayFeeTooLow);
			}
			let fee = relay_fee.unwrap_or_else(T::MinimumRelayFee::get);

			Self::update_market(
				|| {
					T::Currency::set_lock(
						T::LockId::get(),
						&who,
						lock_collateral,
						WithdrawReasons::all(),
					);
					// Store enrollment detail information.
					<RelayersMap<T, I>>::insert(
						&who,
						Relayer::new(who.clone(), lock_collateral, fee),
					);
					<Relayers<T, I>>::append(&who);
					Ok(())
				},
				Some(Event::<T, I>::Enroll(who.clone(), lock_collateral, fee)),
			)
		}

		/// Update locked collateral for enrolled relayer, only supporting lock more. (Update market
		/// needed)
		#[pallet::weight(<T as Config<I>>::WeightInfo::update_locked_collateral())]
		pub fn update_locked_collateral(
			origin: OriginFor<T>,
			new_collateral: BalanceOf<T, I>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_enrolled(&who), <Error<T, I>>::NotEnrolled);
			ensure!(
				T::Currency::free_balance(&who) >= new_collateral,
				<Error<T, I>>::InsufficientBalance
			);

			Self::update_market(
				|| {
					// Increase the locked collateral
					if new_collateral >= Self::relayer_locked_collateral(&who) {
						T::Currency::set_lock(
							T::LockId::get(),
							&who,
							new_collateral,
							WithdrawReasons::all(),
						);
					} else {
						// Decrease the locked collateral
						if let Some((_, orders_locked_collateral)) = Self::occupied(&who) {
							ensure!(
								new_collateral >= orders_locked_collateral,
								<Error<T, I>>::StillHasOrdersNotConfirmed
							);

							T::Currency::remove_lock(T::LockId::get(), &who);
							T::Currency::set_lock(
								T::LockId::get(),
								&who,
								new_collateral,
								WithdrawReasons::all(),
							);
						}
					}

					<RelayersMap<T, I>>::mutate(who.clone(), |relayer| {
						if let Some(ref mut r) = relayer {
							r.collateral = new_collateral;
						}
					});
					Ok(())
				},
				Some(Event::<T, I>::UpdateLockedCollateral(who.clone(), new_collateral)),
			)
		}

		/// Update relay fee for enrolled relayer. (Update market needed)
		#[pallet::weight(<T as Config<I>>::WeightInfo::update_relay_fee())]
		pub fn update_relay_fee(origin: OriginFor<T>, new_fee: BalanceOf<T, I>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_enrolled(&who), <Error<T, I>>::NotEnrolled);
			ensure!(new_fee >= T::MinimumRelayFee::get(), <Error<T, I>>::RelayFeeTooLow);

			Self::update_market(
				|| {
					<RelayersMap<T, I>>::mutate(who.clone(), |relayer| {
						if let Some(ref mut r) = relayer {
							r.fee = new_fee;
						}
					});
					Ok(())
				},
				Some(Event::<T, I>::UpdateRelayFee(who.clone(), new_fee)),
			)
		}

		/// Cancel enrolled relayer(Update market needed)
		#[pallet::weight(<T as Config<I>>::WeightInfo::cancel_enrollment())]
		pub fn cancel_enrollment(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_enrolled(&who), <Error<T, I>>::NotEnrolled);
			ensure!(Self::occupied(&who).is_none(), <Error<T, I>>::OccupiedRelayer);

			Self::update_market(
				|| {
					T::Currency::remove_lock(T::LockId::get(), &who);

					<RelayersMap<T, I>>::remove(who.clone());
					<Relayers<T, I>>::mutate(|relayers| {
						if let Some(ref mut r) = relayers {
							r.retain(|x| x != &who)
						}
					});
					<AssignedRelayers<T, I>>::mutate(|assigned_relayers| {
						if let Some(relayers) = assigned_relayers {
							relayers.retain(|x| x.id != who);
						}
					});
					Ok(())
				},
				Some(Event::<T, I>::CancelEnrollment(who.clone())),
			)
		}

		#[pallet::weight(<T as Config<I>>::WeightInfo::set_slash_protect())]
		pub fn set_slash_protect(
			origin: OriginFor<T>,
			slash_protect: BalanceOf<T, I>,
		) -> DispatchResult {
			ensure_root(origin)?;
			CollateralSlashProtect::<T, I>::put(slash_protect);
			Self::deposit_event(Event::<T, I>::UpdateCollateralSlashProtect(slash_protect));
			Ok(())
		}

		#[pallet::weight(<T as Config<I>>::WeightInfo::set_assigned_relayers_number())]
		pub fn set_assigned_relayers_number(origin: OriginFor<T>, number: u32) -> DispatchResult {
			ensure_root(origin)?;

			Self::update_market(
				|| {
					AssignedRelayersNumber::<T, I>::put(number);
					Ok(())
				},
				Some(Event::<T, I>::UpdateAssignedRelayersNumber(number)),
			)
		}
	}
}
pub use pallet::*;

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// An important update in this pallet, need to update market information in the following
	/// cases:
	///
	/// - New relayer enroll.
	/// - The enrolled relayer wants to update fee or order capacity.
	/// - The enrolled relayer wants to cancel enrollment.
	/// - The order didn't confirm in-time, slash occurred.
	pub(crate) fn update_market<F>(f: F, has_event: Option<Event<T, I>>) -> DispatchResult
	where
		F: FnOnce() -> DispatchResult,
	{
		f()?;

		if let Some(e) = has_event {
			Self::deposit_event(e);
		}

		// Sort all enrolled relayers who are able to accept orders.
		let mut relayers: Vec<Relayer<T::AccountId, BalanceOf<T, I>>> = Vec::new();
		if let Some(ids) = <Relayers<T, I>>::get() {
			for id in ids.iter() {
				if let Some(r) = RelayersMap::<T, I>::get(id) {
					if Self::usable_order_capacity(&r.id) >= 1 {
						relayers.push(r)
					}
				}
			}
		}

		// Select the first `AssignedRelayersNumber` relayers as AssignedRelayer.
		let assigned_relayers_len = <AssignedRelayersNumber<T, I>>::get() as usize;
		if relayers.len() >= assigned_relayers_len {
			relayers.sort();

			let assigned_relayers: Vec<_> = relayers.iter().take(assigned_relayers_len).collect();
			<AssignedRelayers<T, I>>::put(assigned_relayers);
		} else {
			// The market fee comes from the last item in AssignedRelayers,
			// It's would be essential to wipe this storage if relayers not enough.
			<AssignedRelayers<T, I>>::kill();
		}

		Ok(())
	}

	/// Update relayer after slash occurred, this will changes RelayersMap storage. (Update market
	/// needed)
	pub(crate) fn update_relayer_after_slash(
		who: &T::AccountId,
		new_collateral: BalanceOf<T, I>,
		report: SlashReport<T::AccountId, T::BlockNumber, BalanceOf<T, I>>,
	) {
		let _ = Self::update_market(
			|| {
				T::Currency::set_lock(
					T::LockId::get(),
					who,
					new_collateral,
					WithdrawReasons::all(),
				);
				<RelayersMap<T, I>>::mutate(who.clone(), |relayer| {
					if let Some(ref mut r) = relayer {
						r.collateral = new_collateral;
					}
				});
				Ok(())
			},
			Some(<Event<T, I>>::FeeMarketSlash(report)),
		);
	}

	/// Whether the relayer has enrolled
	pub(crate) fn is_enrolled(who: &T::AccountId) -> bool {
		<Relayers<T, I>>::get().map_or(false, |rs| rs.iter().any(|r| *r == *who))
	}

	/// Get market fee, If there is not enough relayers have order capacity to accept new order,
	/// return None.
	pub fn market_fee() -> Option<BalanceOf<T, I>> {
		Self::assigned_relayers().and_then(|relayers| relayers.last().map(|r| r.fee))
	}

	/// Get the relayer locked collateral value
	pub fn relayer_locked_collateral(who: &T::AccountId) -> BalanceOf<T, I> {
		RelayersMap::<T, I>::get(who).map_or(BalanceOf::<T, I>::zero(), |r| r.collateral)
	}

	/// Whether the enrolled relayer is occupied(Responsible for order relaying)
	/// Whether the enrolled relayer is occupied, If occupied, return the number of orders and
	/// orders locked collateral, otherwise, return None.
	pub(crate) fn occupied(who: &T::AccountId) -> Option<(u32, BalanceOf<T, I>)> {
		let mut count = 0u32;
		let mut orders_locked_collateral = BalanceOf::<T, I>::zero();
		for (_, order) in <Orders<T, I>>::iter() {
			if order.assigned_relayers_slice().iter().any(|r| r.id == *who) && !order.is_confirmed()
			{
				count += 1;
				orders_locked_collateral =
					orders_locked_collateral.saturating_add(order.locked_collateral);
			}
		}

		if count == 0 {
			return None;
		}
		Some((count, orders_locked_collateral))
	}

	/// The relayer collateral is composed of two part: fee_collateral and orders_locked_collateral.
	/// Calculate the order capacity with fee_collateral
	pub(crate) fn usable_order_capacity(who: &T::AccountId) -> u32 {
		let relayer_locked_collateral = Self::relayer_locked_collateral(who);
		if let Some((_, orders_locked_collateral)) = Self::occupied(who) {
			let free_collateral =
				relayer_locked_collateral.saturating_sub(orders_locked_collateral);
			return Self::collateral_to_order_capacity(free_collateral);
		}
		Self::collateral_to_order_capacity(relayer_locked_collateral)
	}

	fn collateral_to_order_capacity(collateral: BalanceOf<T, I>) -> u32 {
		// If the `CollateralPerOrder` is zero, the maximum order capacity is `collateral`.
		(collateral / T::CollateralPerOrder::get().min(1)).saturated_into::<u32>()
	}
}

/// The assigned relayers slash trait
pub trait Slasher<T: Config<I>, I: 'static> {
	fn cal_slash_amount(
		collateral_per_order: BalanceOf<T, I>,
		timeout: T::BlockNumber,
	) -> BalanceOf<T, I>;
}
