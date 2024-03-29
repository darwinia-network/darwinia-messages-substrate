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

//! Primitives that may be used at (bridges) runtime level.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod messages;

mod chain;
mod storage_proof;
mod storage_types;

pub use chain::{
	AccountIdOf, AccountPublicOf, BalanceOf, BlockNumberOf, Chain, EncodedOrDecodedCall, HashOf,
	HasherOf, HeaderOf, IndexOf, SignatureOf, TransactionEraOf,
};
pub use frame_support::storage::storage_prefix as storage_value_final_key;
#[cfg(feature = "std")]
pub use storage_proof::craft_valid_storage_proof;
pub use storage_proof::{
	record_all_keys as record_all_trie_keys, Error as StorageProofError,
	ProofSize as StorageProofSize, StorageProofChecker,
};
pub use storage_types::BoundedStorageValue;
// Re-export macro to avoid include paste dependency everywhere
pub use sp_runtime::paste;

// crates.io
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use num_traits::{CheckedSub, One};
use scale_info::TypeInfo;
// substrate
use frame_support::{
	log, pallet_prelude::DispatchResult, PalletError, RuntimeDebug, StorageHasher, StorageValue,
};
use frame_system::RawOrigin;
use sp_core::{storage::StorageKey, H256};
use sp_io::hashing::blake2_256;
use sp_runtime::{
	traits::{BadOrigin, Header as HeaderT},
	transaction_validity::TransactionValidity,
};
use sp_std::{fmt::Debug, prelude::*};

/// Use this when something must be shared among all instances.
pub const NO_INSTANCE_ID: ChainId = [0, 0, 0, 0];

/// Bridge-with-Polkadot instance id.
pub const POLKADOT_CHAIN_ID: ChainId = *b"pdot";

/// Bridge-with-Kusama instance id.
pub const KUSAMA_CHAIN_ID: ChainId = *b"ksma";

/// Bridge-with-Rococo instance id.
pub const ROCOCO_CHAIN_ID: ChainId = *b"roco";

/// Bridge-with-Darwinia instance id.
pub const DARWINIA_CHAIN_ID: ChainId = *b"darw";

/// Bridge-with-Crab instance id.
pub const CRAB_CHAIN_ID: ChainId = *b"crab";

/// Bridge-with-Pangoro instance id.
pub const PANGORO_CHAIN_ID: ChainId = *b"pagr";

/// Bridge-with-Pangolin instance id.
pub const PANGOLIN_CHAIN_ID: ChainId = *b"pagl";

/// Bridge-with-PangolinParachain instance id.
pub const PANGOLIN_PARACHAIN_CHAIN_ID: ChainId = *b"pglp";

/// Bridge-with-PangolinParachainAlpha instance id.
pub const PANGOLIN_PARACHAIN_ALPHA_CHAIN_ID: ChainId = *b"pgpa";

/// Bridge-with-CrabParachain instance id.
pub const CRAB_PARACHAIN_CHAIN_ID: ChainId = *b"crap";

/// Call-dispatch module prefix.
pub const CALL_DISPATCH_MODULE_PREFIX: &[u8] = b"pallet-bridge/dispatch";

/// A unique prefix for entropy when generating cross-chain account IDs.
pub const ACCOUNT_DERIVATION_PREFIX: &[u8] = b"pallet-bridge/account-derivation/account";

/// A unique prefix for entropy when generating a cross-chain account ID for the Root account.
pub const ROOT_ACCOUNT_DERIVATION_PREFIX: &[u8] = b"pallet-bridge/account-derivation/root";

/// Unique identifier of the chain.
///
/// In addition to its main function (identifying the chain), this type may also be used to
/// identify module instance. We have a bunch of pallets that may be used in different bridges. E.g.
/// messages pallet may be deployed twice in the same runtime to bridge ThisChain with Chain1 and
/// Chain2. Sometimes we need to be able to identify deployed instance dynamically. This type may be
/// used for that.
pub type ChainId = [u8; 4];

/// Generic header id provider.
pub trait HeaderIdProvider<Header: HeaderT> {
	// Get the header id.
	fn id(&self) -> HeaderId<Header::Hash, Header::Number>;

	// Get the header id for the parent block.
	fn parent_id(&self) -> Option<HeaderId<Header::Hash, Header::Number>>;
}
impl<Header: HeaderT> HeaderIdProvider<Header> for Header {
	fn id(&self) -> HeaderId<Header::Hash, Header::Number> {
		HeaderId(*self.number(), self.hash())
	}

	fn parent_id(&self) -> Option<HeaderId<Header::Hash, Header::Number>> {
		self.number()
			.checked_sub(&One::one())
			.map(|parent_number| HeaderId(parent_number, *self.parent_hash()))
	}
}

/// Anything that has size.
pub trait Size {
	/// Return size of this object (in bytes).
	fn size(&self) -> u32;
}
impl Size for () {
	fn size(&self) -> u32 {
		0
	}
}
impl Size for Vec<u8> {
	fn size(&self) -> u32 {
		self.len() as _
	}
}

/// Can be use to access the runtime storage key of a `StorageMap`.
pub trait StorageMapKeyProvider {
	/// The name of the variable that holds the `StorageMap`.
	const MAP_NAME: &'static str;

	/// The same as `StorageMap::Hasher1`.
	type Hasher: StorageHasher;
	/// The same as `StorageMap::Key1`.
	type Key: FullCodec;
	/// The same as `StorageMap::Value`.
	type Value: FullCodec;

	/// This is a copy of the
	/// `frame_support::storage::generator::StorageMap::storage_map_final_key`.
	///
	/// We're using it because to call `storage_map_final_key` directly, we need access
	/// to the runtime and pallet instance, which (sometimes) is impossible.
	fn final_key(pallet_prefix: &str, key: &Self::Key) -> StorageKey {
		storage_map_final_key::<Self::Hasher>(pallet_prefix, Self::MAP_NAME, &key.encode())
	}
}

/// Can be use to access the runtime storage key of a `StorageDoubleMap`.
pub trait StorageDoubleMapKeyProvider {
	/// The name of the variable that holds the `StorageDoubleMap`.
	const MAP_NAME: &'static str;

	/// The same as `StorageDoubleMap::Hasher1`.
	type Hasher1: StorageHasher;
	/// The same as `StorageDoubleMap::Key1`.
	type Key1: FullCodec;
	/// The same as `StorageDoubleMap::Hasher2`.
	type Hasher2: StorageHasher;
	/// The same as `StorageDoubleMap::Key2`.
	type Key2: FullCodec;
	/// The same as `StorageDoubleMap::Value`.
	type Value: FullCodec;

	/// This is a copy of the
	/// `frame_support::storage::generator::StorageDoubleMap::storage_double_map_final_key`.
	///
	/// We're using it because to call `storage_double_map_final_key` directly, we need access
	/// to the runtime and pallet instance, which (sometimes) is impossible.
	fn final_key(pallet_prefix: &str, key1: &Self::Key1, key2: &Self::Key2) -> StorageKey {
		let key1_hashed = Self::Hasher1::hash(&key1.encode());
		let key2_hashed = Self::Hasher2::hash(&key2.encode());
		let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
		let storage_prefix_hashed = frame_support::Twox128::hash(Self::MAP_NAME.as_bytes());

		let mut final_key = Vec::with_capacity(
			pallet_prefix_hashed.len()
				+ storage_prefix_hashed.len()
				+ key1_hashed.as_ref().len()
				+ key2_hashed.as_ref().len(),
		);

		final_key.extend_from_slice(&pallet_prefix_hashed[..]);
		final_key.extend_from_slice(&storage_prefix_hashed[..]);
		final_key.extend_from_slice(key1_hashed.as_ref());
		final_key.extend_from_slice(key2_hashed.as_ref());

		StorageKey(final_key)
	}
}

/// Operating mode for a bridge module.
pub trait OperatingMode: Send + Copy + Debug + FullCodec {
	// Returns true if the bridge module is halted.
	fn is_halted(&self) -> bool;
}

/// Bridge module that has owner and operating mode
pub trait OwnedBridgeModule<T: frame_system::Config> {
	/// The target that will be used when publishing logs related to this module.
	const LOG_TARGET: &'static str;

	type OwnerStorage: StorageValue<T::AccountId, Query = Option<T::AccountId>>;
	type OperatingMode: OperatingMode;
	type OperatingModeStorage: StorageValue<Self::OperatingMode, Query = Self::OperatingMode>;

	/// Check if the module is halted.
	fn is_halted() -> bool {
		Self::OperatingModeStorage::get().is_halted()
	}

	/// Ensure that the origin is either root, or `PalletOwner`.
	fn ensure_owner_or_root(origin: T::RuntimeOrigin) -> Result<(), BadOrigin> {
		match origin.into() {
			Ok(RawOrigin::Root) => Ok(()),
			Ok(RawOrigin::Signed(ref signer))
				if Self::OwnerStorage::get().as_ref() == Some(signer) =>
				Ok(()),
			_ => Err(BadOrigin),
		}
	}

	/// Ensure that the module is not halted.
	fn ensure_not_halted() -> Result<(), OwnedBridgeModuleError> {
		match Self::is_halted() {
			true => Err(OwnedBridgeModuleError::Halted),
			false => Ok(()),
		}
	}

	/// Change the owner of the module.
	fn set_owner(origin: T::RuntimeOrigin, maybe_owner: Option<T::AccountId>) -> DispatchResult {
		Self::ensure_owner_or_root(origin)?;
		match maybe_owner {
			Some(owner) => {
				Self::OwnerStorage::put(&owner);
				log::info!(target: Self::LOG_TARGET, "Setting pallet Owner to: {:?}", owner);
			},
			None => {
				Self::OwnerStorage::kill();
				log::info!(target: Self::LOG_TARGET, "Removed Owner of pallet.");
			},
		}

		Ok(())
	}

	/// Halt or resume all/some module operations.
	fn set_operating_mode(
		origin: T::RuntimeOrigin,
		operating_mode: Self::OperatingMode,
	) -> DispatchResult {
		Self::ensure_owner_or_root(origin)?;
		Self::OperatingModeStorage::put(operating_mode);
		log::info!(target: Self::LOG_TARGET, "Setting operating mode to {:?}.", operating_mode);
		Ok(())
	}
}

/// A trait for querying whether a runtime call is valid.
pub trait FilterCall<Call> {
	/// Checks if a runtime call is valid.
	fn validate(call: &Call) -> TransactionValidity;
}

/// Type of accounts on the source chain.
pub enum SourceAccount<T> {
	/// An account that belongs to Root (privileged origin).
	Root,
	/// A non-privileged account.
	///
	/// The embedded account ID may or may not have a private key depending on the "owner" of the
	/// account (private key, pallet, proxy, etc.).
	Account(T),
}

/// Era of specific transaction.
#[derive(Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum TransactionEra<BlockNumber, BlockHash> {
	/// Transaction is immortal.
	Immortal,
	/// Transaction is valid for a given number of blocks, starting from given block.
	Mortal(HeaderId<BlockHash, BlockNumber>, u32),
}
impl<BlockNumber: Copy + Into<u64>, BlockHash: Copy> TransactionEra<BlockNumber, BlockHash> {
	/// Prepare transaction era, based on mortality period and current best block number.
	pub fn new(
		best_block_id: HeaderId<BlockHash, BlockNumber>,
		mortality_period: Option<u32>,
	) -> Self {
		mortality_period
			.map(|mortality_period| TransactionEra::Mortal(best_block_id, mortality_period))
			.unwrap_or(TransactionEra::Immortal)
	}

	/// Create new immortal transaction era.
	pub fn immortal() -> Self {
		TransactionEra::Immortal
	}

	/// Returns mortality period if transaction is mortal.
	pub fn mortality_period(&self) -> Option<u32> {
		match *self {
			TransactionEra::Immortal => None,
			TransactionEra::Mortal(_, period) => Some(period),
		}
	}

	/// Returns era that is used by FRAME-based runtimes.
	pub fn frame_era(&self) -> sp_runtime::generic::Era {
		match *self {
			TransactionEra::Immortal => sp_runtime::generic::Era::immortal(),
			TransactionEra::Mortal(header_id, period) =>
				sp_runtime::generic::Era::mortal(period as _, header_id.0.into()),
		}
	}

	/// Returns header hash that needs to be included in the signature payload.
	pub fn signed_payload(&self, genesis_hash: BlockHash) -> BlockHash {
		match *self {
			TransactionEra::Immortal => genesis_hash,
			TransactionEra::Mortal(header_id, _) => header_id.1,
		}
	}
}

/// Error generated by the `OwnedBridgeModule` trait.
#[derive(Encode, Decode, TypeInfo, PalletError)]
pub enum OwnedBridgeModuleError {
	/// All pallet operations are halted.
	Halted,
}

/// Basic operating modes for a bridges module (Normal/Halted).
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, RuntimeDebug, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum BasicOperatingMode {
	/// Normal mode, when all operations are allowed.
	Normal,
	/// The pallet is halted. All operations (except operating mode change) are prohibited.
	Halted,
}
impl Default for BasicOperatingMode {
	fn default() -> Self {
		Self::Normal
	}
}
impl OperatingMode for BasicOperatingMode {
	fn is_halted(&self) -> bool {
		*self == BasicOperatingMode::Halted
	}
}

/// Generic header Id.
#[derive(
	Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, RuntimeDebug,
)]
pub struct HeaderId<Hash, Number>(pub Number, pub Hash);

/// Pre-computed size.
pub struct PreComputedSize(pub usize);
impl Size for PreComputedSize {
	fn size(&self) -> u32 {
		u32::try_from(self.0).unwrap_or(u32::MAX)
	}
}

/// Derive an account ID from a foreign account ID.
///
/// This function returns an encoded Blake2 hash. It is the responsibility of the caller to ensure
/// this can be successfully decoded into an AccountId.
///
/// The `bridge_id` is used to provide extra entropy when producing account IDs. This helps prevent
/// AccountId collisions between different bridges on a single target chain.
///
/// Note: If the same `bridge_id` is used across different chains (for example, if one source chain
/// is bridged to multiple target chains), then all the derived accounts would be the same across
/// the different chains. This could negatively impact users' privacy across chains.
pub fn derive_account_id<AccountId>(bridge_id: ChainId, id: SourceAccount<AccountId>) -> H256
where
	AccountId: Encode,
{
	match id {
		SourceAccount::Root =>
			(ROOT_ACCOUNT_DERIVATION_PREFIX, bridge_id).using_encoded(blake2_256),
		SourceAccount::Account(id) => {
			let to_darwinia_old_account_id = |address| -> H256 {
				let mut result = [0u8; 32];
				result[0..4].copy_from_slice(b"dvm:");
				result[11..31].copy_from_slice(address);
				result[31] = result[1..31].iter().fold(result[0], |sum, &byte| sum ^ byte);
				result.into()
			};

			// The aim is to keep the accounts derived from the evm account compatible with the
			// darwinia 1.0 account id.
			if id.encode().len() == 20 {
				let account_id = to_darwinia_old_account_id(&id.encode());
				(ACCOUNT_DERIVATION_PREFIX, bridge_id, account_id).using_encoded(blake2_256)
			} else {
				(ACCOUNT_DERIVATION_PREFIX, bridge_id, id).using_encoded(blake2_256)
			}
		},
	}
	.into()
}

/// Derive the account ID of the shared relayer fund account.
///
/// This account is used to collect fees for relayers that are passing messages across the bridge.
///
/// The account ID can be the same across different instances of `pallet-bridge-messages` if the
/// same `bridge_id` is used.
pub fn derive_relayer_fund_account_id(bridge_id: ChainId) -> H256 {
	("relayer-fund-account", bridge_id).using_encoded(blake2_256).into()
}

/// This is a copy of the
/// `frame_support::storage::generator::StorageMap::storage_map_final_key` for maps based
/// on selected hasher.
///
/// We're using it because to call `storage_map_final_key` directly, we need access to the runtime
/// and pallet instance, which (sometimes) is impossible.
pub fn storage_map_final_key<H: StorageHasher>(
	pallet_prefix: &str,
	map_name: &str,
	key: &[u8],
) -> StorageKey {
	let key_hashed = H::hash(key);
	let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
	let storage_prefix_hashed = frame_support::Twox128::hash(map_name.as_bytes());

	let mut final_key = Vec::with_capacity(
		pallet_prefix_hashed.len() + storage_prefix_hashed.len() + key_hashed.as_ref().len(),
	);

	final_key.extend_from_slice(&pallet_prefix_hashed[..]);
	final_key.extend_from_slice(&storage_prefix_hashed[..]);
	final_key.extend_from_slice(key_hashed.as_ref());

	StorageKey(final_key)
}

/// This is how a storage key of storage parameter (`parameter_types! { storage Param: bool = false;
/// }`) is computed.
///
/// Copied from `frame_support::parameter_types` macro.
pub fn storage_parameter_key(parameter_name: &str) -> StorageKey {
	let mut buffer = Vec::with_capacity(1 + parameter_name.len() + 1);
	buffer.push(b':');
	buffer.extend_from_slice(parameter_name.as_bytes());
	buffer.push(b':');
	StorageKey(sp_io::hashing::twox_128(&buffer).to_vec())
}

/// This is how a storage key of storage value is computed.
///
/// Copied from `frame_support::storage::storage_prefix`.
pub fn storage_value_key(pallet_prefix: &str, value_name: &str) -> StorageKey {
	let pallet_hash = sp_io::hashing::twox_128(pallet_prefix.as_bytes());
	let storage_hash = sp_io::hashing::twox_128(value_name.as_bytes());

	let mut final_key = vec![0u8; 32];
	final_key[..16].copy_from_slice(&pallet_hash);
	final_key[16..].copy_from_slice(&storage_hash);

	StorageKey(final_key)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn storage_parameter_key_works() {
		assert_eq!(
			storage_parameter_key("MillauToRialtoConversionRate"),
			StorageKey(array_bytes::hex2bytes_unchecked("58942375551bb0af1682f72786b59d04")),
		);
	}

	#[test]
	fn storage_value_key_works() {
		assert_eq!(
			storage_value_key("PalletTransactionPayment", "NextFeeMultiplier"),
			StorageKey(array_bytes::hex2bytes_unchecked(
				"f0e954dfcca51a255ab12c60c789256a3f2edf3bdf381debe331ab7446addfdc"
			)),
		);
	}
}
