// This file is part of Darwinia.
//
// Copyright (C) 2018-2021 Darwinia Network
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

//! # Fee Market Module

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "128"]

#[cfg(test)]
mod tests;

pub mod weights;
use crate::weights::WeightInfo;

use codec::{Decode, Encode};
// use darwinia_support::balance::{LockFor, LockableCurrency};
use frame_support::{
	dispatch::DispatchError,
	ensure,
	pallet_prelude::*,
	traits::{Currency, Get, LockIdentifier, LockableCurrency, WithdrawReasons},
	transactional, PalletId,
};
use frame_system::{ensure_signed, pallet_prelude::*};
use sp_std::{
	cmp::{Ord, Ordering},
	default::Default,
	vec::Vec,
};

pub type AccountId<T> = <T as frame_system::Config>::AccountId;
pub type RingBalance<T> = <<T as Config>::RingCurrency as Currency<AccountId<T>>>::Balance;
pub type Fee<T> = RingBalance<T>;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		#[pallet::constant]
		type PalletId: Get<PalletId>;
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		#[pallet::constant]
		type MiniumLockValue: Get<RingBalance<Self>>;
		#[pallet::constant]
		type MinimumFee: Get<Fee<Self>>;
		#[pallet::constant]
		type PriorRelayersNumber: Get<u64>;
		#[pallet::constant]
		type LockId: Get<LockIdentifier>;

		type RingCurrency: LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>;
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	#[pallet::metadata(T::AccountId = "AccountId")]
	pub enum Event<T: Config> {
		/// Relayer register
		Register(T::AccountId, RingBalance<T>, Fee<T>),
		/// Update relayer lock balance
		UpdateLockedBalance(T::AccountId, RingBalance<T>),
		/// Update relayer fee
		UpdateFee(T::AccountId, Fee<T>),
		/// Cancel relayer register
		CancelRelayerRegister(T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Insufficient balance
		InsufficientBalance,
		/// The lock value is lower than MiniumLockLimit
		TooLowLockValue,
		/// The relayer has been registered
		AlreadyRegistered,
		/// Register before update lock value
		RegisterBeforeUpdateLock,
		/// Invalid new lock value
		InvalidNewLockValue,
		/// Only Relayer can submit fee
		InvalidSubmitPriceOrigin,
		/// The fee is lower than MinimumFee
		TooLowFee,
	}

	#[pallet::storage]
	#[pallet::getter(fn get_relayer)]
	pub type RelayersMap<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Relayer<T>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn relayers)]
	pub type Relayers<T: Config> = StorageValue<_, Vec<T::AccountId>, ValueQuery>;

	/// The lowest n fees, p.0 < p.1 < p.2 ... < p.n
	#[pallet::storage]
	#[pallet::getter(fn prior_relayers)]
	pub type PriorRelayers<T: Config> = StorageValue<_, Vec<(T::AccountId, Fee<T>)>, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn best_relayer)]
	pub type TopRelayer<T: Config> = StorageValue<_, (T::AccountId, Fee<T>), ValueQuery>;

	// FeeMarket Progress Design:

	// market pallet
	// Storage:
	// 		- pub type Orders = StorageMap<nonce, OrderState>, OrderState {send_time, confirm_time, first_relayer: { relayer1, start_time: send_time, deadline: send_time + T1 },
	// 				second_relayer: {relayer2, start_time: send_time + T1, deadline: send_time + T1 + T2}, third_relayer: {relayer3, start_time: send_time + T1 + T2 , deadline: send_time + T1 + T2 + T3} }
	//
	// Config:
	//      -  Relay time config: T1, T2, T3. Now, we assume that T1=T2=T3=50.
	//      -  Provides `best_relayer(P3)`, `prior_relayers(P1, P2, P3)` according to their fee proposal(P1, P2, P3, P4....) before


	// In fee market, we assume that there is a callback function already in messages pallet send_message() call, the fee pallet can catch the generated nonce and create an order in Orders storage above.
	// *Something note: in current parity-bridges-common, this callback function is not implemented, we need add this*
	// In this way, the fee market will record each message sent in messages pallet. Another thing is assigning first, second, third relayer to this message.

	// Relay process(We don't need to hack relayer process after discuss with denny):
	//      - a new `Market` relayer Mode

	// In relayer race delivery process, Before select nonce to delivery, the relayer needs to check the following things:
	// 		1. Get the latest source header number (Sh)
	// 		2. Get the nonce order record in fee market pallet (sent_time, confirm_time(no yet), first_relayer{...}, second_relayer{...}, third_relayer{...})
	//      3. Make a decision according to the `Our` delivery rule that whether the relayer should pick this nonce to deliver. If yes, pick this one, otherwise, do noting and check next nonce.

	// `Our` delivery rule:
	// For example: a message(m1) with four relayers(r1, r2, r3, r4), T1=T2=T3=50, Fee proposal: r1 -> P1, r2 -> P2 , r3 -> P3, r4 -> P4
	// 1. Assume that the message order in fee market pallet like: <m1 nonce, OrderState {send_time: 30, confirm_time: none, first_relayer: {r3, deadline: 80}, second_relayer: {r1, deadline: 130(80+50)}, first_relayer: {r2, deadline: 180(80+50+50)}}>
	// 2. Now, the relayer(r1), relayer(r2), relayer(r3), relayer(r4) all setup the relay loop and starting relaying message, the following analysis from each relayer perspective.
	// - At source chain height 35:
	//      - For r1, find this nonce in source chain and check the order relayers list, he is the second relayer and his relay rang is (80~130) and current source number is 35 < 80, so skip it.
	//      - For r2, the same as r1.
	//      - For r3, find this nonce in source chian and check the order relayers list, he is the first relayer and 35 is in his relay rang (30~80), so he pick it and deliver it to the target chain.
	//      - For r4, the same as r1, r2
	// - At source chain height 35 ~ 180
	//      - similar to case of `At source chain height 35`
	// - At source chain height 180~
	//    - In most cases, the message won't waiting for so long and no relayer relaying message. If occurs, all relayers(r1, r2, r3, r4) have rights to pick this message and delivery.


	// Confirm message and reward process:
	// Once the message had delivered and dispatched, the confirmation relayer will send message delivery proof to the source chain.
	// we assume that there is a callback function already in messages pallet receive_messages_delivery_proof() call, the fee pallet can catch the confirmation content `DeliveredMessages{begin, end, dispatch_result}`.
	// *Something note*, in current parity-bridges-common, the call back input params does not contain the message delivery relayer info, we need to add this.
	// The fee pallet check the delivered nonce corresponding order recorded before:




	#[pallet::genesis_config]
	pub struct GenesisConfig {}
	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {}
		}
	}
	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register to be a relayer
		#[pallet::weight(10000)]
		#[transactional]
		pub fn register(
			origin: OriginFor<T>,
			lock_value: RingBalance<T>,
			fee: Option<Fee<T>>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(lock_value >= T::MiniumLockValue::get(), <Error<T>>::TooLowLockValue);
			ensure!(
				T::RingCurrency::free_balance(&who) >= lock_value,
				<Error<T>>::InsufficientBalance
			);
			ensure!(!Self::is_registered(&who), <Error<T>>::AlreadyRegistered);
			if let Some(p) = fee {
				ensure!(p >= T::MinimumFee::get(), <Error<T>>::TooLowFee);
			}

			let fee = fee.unwrap_or_else(T::MinimumFee::get);
			T::RingCurrency::set_lock(
				T::LockId::get(),
				&who,
				// LockFor::Common { amount: lock_value },
				lock_value,
				WithdrawReasons::all(),
			);

			<RelayersMap<T>>::insert(&who, Relayer::new(who.clone(), lock_value, fee));
			<Relayers<T>>::append(who.clone());

			Self::update_relayer_fees()?;
			Self::deposit_event(Event::<T>::Register(who, lock_value, fee));
			Ok(().into())
		}

		/// Relayer update locked balance
		#[pallet::weight(10000)]
		#[transactional]
		pub fn update_locked_balance(origin: OriginFor<T>, new_lock: RingBalance<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_registered(&who), <Error<T>>::RegisterBeforeUpdateLock);
			ensure!(
				T::RingCurrency::free_balance(&who) >= new_lock,
				<Error<T>>::InsufficientBalance
			);
			ensure!(
				new_lock > Self::get_relayer(&who).lock_balance,
				<Error<T>>::InvalidNewLockValue
			);

			T::RingCurrency::extend_lock(T::LockId::get(), &who, new_lock, WithdrawReasons::all());
			<RelayersMap<T>>::mutate(who.clone(), |relayer| {
				relayer.lock_balance = new_lock;
			});
			Self::deposit_event(Event::<T>::UpdateLockedBalance(who, new_lock));
			Ok(().into())
		}

		/// Relayer cancel register
		#[pallet::weight(10000)]
		#[transactional]
		pub fn cancel_register(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_registered(&who), <Error<T>>::RegisterBeforeUpdateLock);

			T::RingCurrency::remove_lock(T::LockId::get(), &who);
			RelayersMap::<T>::remove(who.clone());
			Relayers::<T>::mutate(|relayers| relayers.retain(|x| x != &who));

			Self::update_relayer_fees()?;
			Self::deposit_event(Event::<T>::CancelRelayerRegister(who));
			Ok(().into())
		}

		/// Relayer update fee
		#[pallet::weight(10000)]
		#[transactional]
		pub fn update_fee(origin: OriginFor<T>, p: Fee<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(Self::is_registered(&who), <Error<T>>::InvalidSubmitPriceOrigin);
			ensure!(p >= T::MinimumFee::get(), <Error<T>>::TooLowFee);

			<RelayersMap<T>>::mutate(who.clone(), |relayer| {
				relayer.fee = p;
			});

			Self::update_relayer_fees()?;
			Self::deposit_event(Event::<T>::UpdateFee(who, p));
			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Update fees in the following cases:
	/// 1. New relayer register
	/// 2. Already registered relayer update fee
	/// 3. Cancel registered relayer
	pub fn update_relayer_fees() -> Result<(), DispatchError> {
		<PriorRelayers<T>>::kill();

		let mut relayers: Vec<Relayer<T>> = <Relayers<T>>::get().iter().map(RelayersMap::<T>::get).collect();
		relayers.sort();

		// If the registered relayers number >= the PriorRelayersNumber,
		// append the lowest PriorRelayersNumber relayers to PriorRelayers and choose the last one as TopRelayer.
		if relayers.len() >= T::PriorRelayersNumber::get() as usize {
			for i in 0..T::PriorRelayersNumber::get() as usize {
				let r = &relayers[i];
				<PriorRelayers<T>>::append((r.id.clone(), r.fee));
			}
		} else {
			// If the registered relayers number < the PriorRelayersNumber,
			// append all submit fee to PriorRelayers and choose the last one as TopRelayer
			for r in relayers.iter() {
				<PriorRelayers<T>>::append((r.id.clone(), r.fee));
			}
		}
		<TopRelayer<T>>::put(
			<PriorRelayers<T>>::get()
				.iter()
				.last()
				.map(|(r, p)| ((*r).clone(), *p))
				.unwrap_or_default(),
		);
		Ok(())
	}

	/// Whether the relayer has registered
	pub fn is_registered(who: &T::AccountId) -> bool {
		<Relayers<T>>::get().iter().any(|r| *r == *who)
	}

	// Get relayer fee
	pub fn relayer_price(who: &T::AccountId) -> Fee<T> {
		Self::get_relayer(who).fee
	}

	// Get relayer locked balance
	pub fn relayer_locked_balance(who: &T::AccountId) -> RingBalance<T> {
		Self::get_relayer(who).lock_balance
	}

	pub fn slash_relayer() {
		// slash relayers
		// if the lock ring lower than limit, remove it auto
		todo!()
	}
}
#[derive(Encode, Decode, Clone, Eq, Debug)]
pub struct Relayer<T: Config> {
	id: T::AccountId,
	lock_balance: RingBalance<T>,
	fee: Fee<T>,
}

impl<T: Config> Relayer<T> {
	pub fn new(id: T::AccountId, lock_balance: RingBalance<T>, fee: Fee<T>) -> Relayer<T> {
		Relayer { id, lock_balance, fee }
	}
}

impl<T: Config> PartialOrd for Relayer<T> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		self.fee.partial_cmp(&other.fee)
	}
}

impl<T: Config> Ord for Relayer<T> {
	fn cmp(&self, other: &Self) -> Ordering {
		self.fee.cmp(&other.fee)
	}
}

impl<T: Config> PartialEq for Relayer<T> {
	fn eq(&self, other: &Self) -> bool {
		self.fee == other.fee && self.id == other.id && self.lock_balance == other.lock_balance
	}
}

impl<T: Config> Default for Relayer<T> {
	fn default() -> Self {
		Relayer {
			id: T::AccountId::default(),
			lock_balance: RingBalance::<T>::default(),
			fee: Fee::<T>::default(),
		}
	}
}
