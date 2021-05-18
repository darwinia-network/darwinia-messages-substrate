// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use bp_eth_clique::{Address, CliqueHeader};
use codec::{Decode, Encode};
use frame_support::{decl_error, decl_module, decl_storage, ensure, traits::Get};
use primitive_types::U256;
use sp_runtime::RuntimeDebug;
use sp_std::{cmp::Ord, collections::btree_set::BTreeSet, prelude::*};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

extern crate parity_crypto as crypto;

#[macro_use]
extern crate lazy_static;

mod error;
mod utils;
mod verification;

/// CliqueVariant  pallet configuration parameters.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct CliqueVariantConfiguration {
	/// Minimum gas limit.
	pub min_gas_limit: U256,
	/// Maximum gas limit.
	pub max_gas_limit: U256,
	/// epoch length
	pub epoch_length: u64,
	/// block period
	pub period: u64,
}

/// The storage that is used by the client.
///
/// Storage modification must be discarded if block import has failed.
pub trait Storage {
	/// Header submitter identifier.
	type Submitter: Clone + Ord;

	/// Get finalized authority set
	fn finalized_authority(&self) -> Vec<Address>;

	/// Get finalized checkpoint header
	fn finalized_checkpoint(&self) -> CliqueHeader;

	/// Put updated authority set into storage
	fn save_finalized_authority(&mut self, authority_set: Vec<Address>);

	/// Save finalized checkpoint header
	fn save_checkpoint(&mut self, checkpoint: CliqueHeader);

	/// Return true if signer in finanlized authority set
	fn contains(&self, signers: Vec<Address>, signer: Address) -> bool;
}

/// ChainTime represents the runtime on-chain time
pub trait ChainTime: Default {
	/// Is a header timestamp ahead of the current on-chain time.
	///
	/// Check whether `timestamp` is ahead (i.e greater than) the current on-chain
	/// time. If so, return `true`, `false` otherwise.
	fn is_timestamp_ahead(&self, timestamp: u64) -> bool;
}

/// ChainTime implementation for the empty type.
///
/// This implementation will allow a runtime without the timestamp pallet to use
/// the empty type as its ChainTime associated type.
impl ChainTime for () {
	/// Is a header timestamp ahead of the current on-chain time.
	///
	/// Check whether `timestamp` is ahead (i.e greater than) the current on-chain
	/// time. If so, return `true`, `false` otherwise.
	fn is_timestamp_ahead(&self, timestamp: u64) -> bool {
		// This should succeed under the contraints that the system clock works
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
		Duration::from_secs(timestamp) > now
	}
}

/// Callbacks for header submission rewards/penalties.
pub trait OnHeadersSubmitted<AccountId> {
	/// Called when valid headers have been submitted.
	///
	/// The submitter **must not** be rewarded for submitting valid headers, because greedy authority
	/// could produce and submit multiple valid headers (without relaying them to other peers) and
	/// get rewarded. Instead, the provider could track submitters and stop rewarding if too many
	/// headers have been submitted without finalization.
	fn on_valid_headers_submitted(submitter: AccountId, headers: Vec<CliqueHeader>);
	/// Called when invalid headers have been submitted.
	fn on_invalid_headers_submitted(submitter: AccountId);
	/// Called when earlier submitted headers have been finalized.
	///
	/// finalized is the finalized authority set
	fn on_valid_authority_finalized(submitter: AccountId, finalized: Vec<Address>);
}

impl<AccountId> OnHeadersSubmitted<AccountId> for () {
	fn on_valid_headers_submitted(_submitter: AccountId, _headers: Vec<CliqueHeader>) {}
	fn on_invalid_headers_submitted(_submitter: AccountId) {}
	fn on_valid_authority_finalized(_submitter: AccountId, _finalized: Vec<Address>) {}
}

/// The module configuration trait.
pub trait Config<I = DefaultInstance>: frame_system::Config {
	/// CliqueVariant configuration.
	type CliqueVariantConfiguration: Get<CliqueVariantConfiguration>;
	/// Header timestamp verification against current on-chain time.
	type ChainTime: ChainTime;
	/// Handler for headers submission result.
	type OnHeadersSubmitted: OnHeadersSubmitted<Self::AccountId>;
}

decl_error! {
	pub enum Error for Module<T: Config> {
		/// Block number isn't sensible.
		RidiculousNumber,
		/// The size of submitted headers is not N/2+1
		InvalidHeadersSize,
		/// This header is not checkpoint,
		NotCheckpoint,
		/// Invalid signer
		InvalidSigner,
		/// Submitted headers not enough
		HeadersNotEnough,
		/// Signed recently
		SignedRecently,
	}
}

decl_module! {
	pub struct Module<T: Config<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Verify unsigned relayed headers and finalize authority set
		#[weight = 0]
		pub fn verify_and_update_authority_set_unsigned(origin, headers: Vec<CliqueHeader>) {
			// ensure not signed
			frame_system::ensure_none(origin)?;
			// get finalized authority set from storage
			let last_authority_set = BridgeStorage::<T, I>::new().finalized_authority();
			// ensure valid length
			assert!(last_authority_set.len() / 2 + 1 <= headers.len(), "Invalid headers size");
			let last_checkpoint = BridgeStorage::<T, I>::new().finalized_checkpoint();
			let cfg = T::CliqueVariantConfiguration::get();
			let checkpoint = headers[0];
			// ensure valid header number
			// CHECKME it should be <= or == ?
			assert!(last_checkpoint.number + cfg.epoch_length == checkpoint.number, "Ridiculous checkpoint header number");
			// ensure first element is checkpoint block header
			assert!(checkpoint.number % cfg.epoch_length == 0, "First element is not checkpoint");

			// verify checkpoint
			verification::contextless_checks(cfg, checkpoint, &T::ChainTime::default());
			let signer = utils::recover_creator(&checkpoint)?;
			ensure!(BridgeStorage::<T, I>::new().contains(last_authority_set, signer), Error::<T>::InvalidSigner);

			let mut recently = BTreeSet::new()?;
			let new_authority_set = utils::extract_signers(checkpoint);
			for i in 1..headers.len() {
				verification::contextless_checks(cfg, headers[i], &T::ChainTime::default()).map_err(|e|e.msg())?;
				verification::contextual_checks(cfg, headers[i], headers[i-1]).map_err(|e|e.msg())?;
				let signer = utils::recover_creator(&headers[i])?;
				// signed by last authority set
				ensure!(BridgeStorage::<T, I>::new().contains(last_authority_set, signer), "Signer not authorized")?;
				// headers submitted must signed by different authority
				ensure!(!recently.contains(&signer), "Signer signed recently");
				if new_authority_set.len() == last_authority_set.len() {
					// finalize new authroity set
					BridgeStorage::<T, I>::new().save_finalized_authority(new_authority_set);
					BridgeStorage::<T, I>::new().save_checkpoint(checkpoint);
					return Ok(());
				}
			}

			Err(Error::<T>::HeadersNotEnough)
		}

		/// Verify signed relayed headers and finalize authority set
		#[weight = 0]
		pub fn verify_and_update_authority_set_signed(origin, headers: Vec<CliqueHeader>) {
			let submitter = frame_system::ensure_signed(origin)?;
			let last_authority_set = BridgeStorage::<T, I>::new().finalized_authority();
			// ensure valid length
			assert!(last_authority_set.len() / 2 + 1 <= headers.len(),  "Invalid headers size");
			let last_checkpoint = BridgeStorage::<T, I>::new().finalized_checkpoint();
			let cfg = T::CliqueVariantConfiguration::get();
			let checkpoint = headers[0];
			// ensure valid header number
			// CHECKME it should be <= or == ?
			assert!(last_checkpoint.number + cfg.epoch_length == checkpoint.number, "Ridiculous checkpoint header number");
			// ensure first element is checkpoint block header
			assert!(checkpoint.number % cfg.epoch_length == 0, "First element is not checkpoint");

			// verify checkpoint
			verification::contextless_checks(cfg, checkpoint, &T::ChainTime::default());
			let signer = utils::recover_creator(&checkpoint)?;
			ensure!(BridgeStorage::<T, I>::new().contains(last_authority_set, signer), "Signer not authorized");

			let mut recently = BTreeSet::new();
			let new_authority_set = utils::extract_signers(checkpoint);
			for i in 1..headers.len() {
				verification::contextless_checks(cfg, headers[i], &T::ChainTime::default()).map_err(|e|e.msg())?;
				verification::contextual_checks(cfg, headers[i], headers[i-1]).map_err(|e|e.msg())?;
				let signer = utils::recover_creator(&headers[i])?;
				// signed by last authority set
				ensure!(BridgeStorage::<T, I>::new().contains(last_authority_set, signer), "Signer not authorized")?;
				// headers submitted must signed by different authority
				ensure!(!recently.contains(signer), "Signer signed recently");
				if new_authority_set.len() == last_authority_set.len() {
					// finalize new authroity set
					BridgeStorage::<T, I>::new().save_finalized_authority(new_authority_set);
					BridgeStorage::<T, I>::new().save_checkpoint(checkpoint);
					T::OnHeadersSubmitted::on_valid_authority_finalized(submitter, new_authority_set);
					return Ok(());
				}
			}
			T::OnHeadersSubmitted::on_invalid_headers_submitted(submitter);

			Err(Error::<T>::HeadersNotEnough)
		}
	}
}

decl_storage! {
	trait Store for Pallet<T: Config<I>, I: Instance = DefaultInstance> as Bridge {
		/// Finalized authority set.
		FinalizedAuthority: Vec<Address>;
		FinalizedCheckpoint: CliqueHeader;
	}
	add_extra_genesis {
		config(initial_validators): Vec<Address>;
		build(|config| {
			assert!(
				!config.initial_validators.is_empty(),
				"Initial validators set can't be empty",
			);

			initialize_storage::<T, I>(
				&config.initial_validators,
			);
		})
	}
}

impl<T: Config<I>, I: Instance> Pallet<T, I> {
	/// Returns finalized authority set
	pub fn finalized_authority() -> Vec<Address> {
		BridgeStorage::<T, I>::new().finalized_authority()
	}
}

/// Runtime bridge storage.
#[derive(Default)]
pub struct BridgeStorage<T, I: Instance = DefaultInstance>(sp_std::marker::PhantomData<(T, I)>);

impl<T: Config<I>, I: Instance> BridgeStorage<T, I> {
	/// Create new BridgeStorage.
	pub fn new() -> Self {
		BridgeStorage(sp_std::marker::PhantomData::<(T, I)>::default())
	}
}

impl<T: Config<I>, I: Instance> Storage for BridgeStorage<T, I> {
	type Submitter = T::AccountId;

	fn finalized_authority(&self) -> Vec<Address> {
		FinalizedAuthority::<T, I>::get()
	}

	fn finalized_checkpoint(&self) -> CliqueHeader {
		FinalizedCheckpoint::<T, I>::get()
	}

	fn save_finalized_authority(&mut self, authority_set: Vec<Address>) {
		FinalizedAuthority::<I>::put(authority_set);
	}

	fn save_checkpoint(&mut self, checkpoint: CliqueHeader) {
		FinalizedCheckpoint::<T, I>::put(checkpoint);
	}

	fn contains(&self, signers: Vec<Address>, signer: Address) -> bool {
		match signers.binary_search(&signer) {
			// If the search succeeds, the caller is already a member, so just return
			Ok(_) => true,
			Err(_) => false,
		}
	}
}

/// Initialize storage.
#[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
pub(crate) fn initialize_storage<T: Config<I>, I: Instance>(initial_validators: &[Address]) {
	FinalizedAuthority::<I>::put(initial_validators);
}
