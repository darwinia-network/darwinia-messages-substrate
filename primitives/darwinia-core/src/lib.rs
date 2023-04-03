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

#![cfg_attr(not(feature = "std"), no_std)]

mod copy_paste_from_darwinia {
	// moonbeam
	use account::EthereumSignature;
	// paritytech
	use frame_support::{
		dispatch::DispatchClass,
		weights::{
			constants::{BlockExecutionWeight, ExtrinsicBaseWeight, WEIGHT_REF_TIME_PER_MILLIS},
			Weight,
		},
	};
	use frame_system::limits::{BlockLength, BlockWeights};
	use sp_core::H256;
	use sp_runtime::{
		generic,
		traits::{BlakeTwo256, IdentifyAccount, Verify},
		OpaqueExtrinsic, Perbill,
	};

	pub type BlockNumber = u32;
	pub type Hashing = BlakeTwo256;
	pub type Hash = H256;
	pub type Signature = EthereumSignature;
	pub type AccountPublic = <Signature as Verify>::Signer;
	pub type AccountId = <AccountPublic as IdentifyAccount>::AccountId;
	pub type Address = AccountId;
	pub type Nonce = u32;
	pub type Balance = u128;
	pub type Header = generic::Header<BlockNumber, Hashing>;
	pub type OpaqueBlock = generic::Block<Header, OpaqueExtrinsic>;

	/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
	/// used to limit the maximal weight of a single extrinsic.
	pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);
	/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
	/// `Operational` extrinsics.
	pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
	/// We allow for 0.5 of a second of compute with a 12 second average block time.
	pub const WEIGHT_MILLISECS_PER_BLOCK: u64 = 500;
	/// Maximum PoV size we support right now.(Copied from the polkadot repo)
	pub const MAX_POV_SIZE: u32 = 5 * 1024 * 1024;
	pub const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
		WEIGHT_REF_TIME_PER_MILLIS * WEIGHT_MILLISECS_PER_BLOCK,
		MAX_POV_SIZE as u64,
	);

	frame_support::parameter_types! {
		pub RuntimeBlockLength: BlockLength =
		BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have some extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
	}

	/// This determines the average expected block time that we are targeting.
	/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
	/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
	/// up by `pallet_aura` to implement `fn slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECS_PER_BLOCK: u64 = 12_000;

	// NOTE: Currently it is not possible to change the slot duration after the chain has started.
	//       Attempting to do so will brick block production.
	/// Slot duration.
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	// Time is measured by number of blocks.
	/// 10 blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	/// 600 blocks.
	pub const HOURS: BlockNumber = MINUTES * 60;
	/// 14,400 blocks.
	pub const DAYS: BlockNumber = HOURS * 24;

	// Unit = the base number of indivisible units for balances
	/// 1e18 wei — 1,000,000,000,000,000,000
	pub const UNIT: Balance = 1_000 * MILLIUNIT;
	/// 1e15 wei — 1,000,000,000,000,000
	pub const MILLIUNIT: Balance = 1_000 * MICROUNIT;
	/// 1e12 wei — 1,000,000,000,000
	pub const MICROUNIT: Balance = 1_000 * GWEI;
	/// 1e9 wei — 1,000,000,000
	pub const GWEI: Balance = 1_000 * MWEI;
	/// 1e6 wei — 1,000,000
	pub const MWEI: Balance = 1_000 * KWEI;
	/// 1e3 wei — 1,000
	pub const KWEI: Balance = 1_000 * WEI;
	/// 1 wei — 1
	pub const WEI: Balance = 1;
}
pub use copy_paste_from_darwinia::*;

// core
use core::{fmt::Debug, marker::PhantomData};
// crates.io
use codec::{Codec, Compact, Decode, Encode, Error as CodecError, Input};
use scale_info::{StaticTypeInfo, TypeInfo};
// darwinia-network
use bp_messages::MessageNonce;
use bp_runtime::{Chain, EncodedOrDecodedCall, TransactionEraOf};
// paritytech
use frame_support::{
	dispatch::DispatchClass,
	unsigned::{TransactionValidityError, UnknownTransaction},
	weights::Weight,
};
use sp_core::{H160, H256};
use sp_runtime::{
	generic,
	generic::Era,
	traits::{Convert, DispatchInfoOf, Dispatchable, SignedExtension as SignedExtensionT},
	RuntimeDebug,
};
use sp_std::prelude::*;

/// Unchecked Extrinsic type.
pub type UncheckedExtrinsic<Call> = generic::UncheckedExtrinsic<
	Address,
	EncodedOrDecodedCall<Call>,
	Signature,
	SignedExtensions<Call>,
>;

/// Parameters which are part of the payload used to produce transaction signature,
/// but don't end up in the transaction itself (i.e. inherent part of the runtime).
pub type AdditionalSigned = ((), u32, u32, Hash, Hash, (), (), ());

/// A type of the data encoded as part of the transaction.
pub type SignedExtra = ((), (), (), (), Era, Compact<Nonce>, (), Compact<Balance>);

/// Maximal number of unrewarded relayer entries at inbound lane.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 128;

// TODO [#438] should be selected keeping in mind:
// finality delay on both chains + reward payout cost + messages throughput.
/// Maximal number of unconfirmed messages at inbound lane.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 8192;

/// A simplified version of signed extensions meant for producing signed transactions
/// and signed payload in the client code.
#[derive(Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct SignedExtensions<Call> {
	encode_payload: SignedExtra,
	// It may be set to `None` if extensions are decoded. We are never reconstructing transactions
	// (and it makes no sense to do that) => decoded version of `SignedExtensions` is only used to
	// read fields of `encode_payload`. And when resigning transaction, we're reconstructing
	// `SignedExtensions` from the scratch.
	additional_signed: Option<AdditionalSigned>,
	_data: PhantomData<Call>,
}
impl<Call> SignedExtensions<Call> {
	pub fn new(
		spec_version: u32,
		transaction_version: u32,
		era: TransactionEraOf<DarwiniaLike>,
		genesis_hash: Hash,
		nonce: Nonce,
		tip: Balance,
	) -> Self {
		Self {
			encode_payload: (
				(),              // non-zero sender
				(),              // spec version
				(),              // tx version
				(),              // genesis
				era.frame_era(), // era
				nonce.into(),    // nonce (compact encoding)
				(),              // Check weight
				tip.into(),      // transaction payment / tip (compact encoding)
			),
			additional_signed: Some((
				(),
				spec_version,
				transaction_version,
				genesis_hash,
				era.signed_payload(genesis_hash),
				(),
				(),
				(),
			)),
			_data: Default::default(),
		}
	}

	/// Return signer nonce, used to craft transaction.
	pub fn nonce(&self) -> Nonce {
		self.encode_payload.5.into()
	}

	/// Return transaction tip.
	pub fn tip(&self) -> Balance {
		self.encode_payload.7.into()
	}
}
impl<Call> Encode for SignedExtensions<Call> {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		self.encode_payload.using_encoded(f)
	}
}
impl<Call> Decode for SignedExtensions<Call> {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		SignedExtra::decode(input).map(|encode_payload| SignedExtensions {
			encode_payload,
			additional_signed: None,
			_data: Default::default(),
		})
	}
}
impl<Call> SignedExtensionT for SignedExtensions<Call>
where
	Call: Clone + Debug + Eq + PartialEq + Sync + Send + Codec + StaticTypeInfo + Dispatchable,
{
	type AccountId = AccountId;
	type AdditionalSigned = AdditionalSigned;
	type Call = Call;
	type Pre = ();

	const IDENTIFIER: &'static str = "Not needed.";

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		// we shall not ever see this error in relay, because we are never signing decoded
		// transactions. Instead we're constructing and signing new transactions. So the error code
		// is kinda random here
		self.additional_signed
			.ok_or(TransactionValidityError::Unknown(UnknownTransaction::Custom(0xFF)))
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}
}

/// Darwinia-like chain.
#[derive(RuntimeDebug)]
pub struct DarwiniaLike;
impl Chain for DarwiniaLike {
	type AccountId = AccountId;
	type Balance = Balance;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hashing;
	type Header = Header;
	type Index = Nonce;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		*RuntimeBlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		RuntimeBlockWeights::get().get(DispatchClass::Normal).max_extrinsic.unwrap_or(Weight::MAX)
	}
}

/// Convert a 256-bit hash into an AccountId.
pub struct AccountIdConverter;
impl Convert<H256, AccountId> for AccountIdConverter {
	fn convert(hash: H256) -> AccountId {
		// This way keep compatible with darwinia 1.0 substrate to evm account rule.
		let evm_address = H160::from_slice(&hash.as_bytes()[0..20]);
		evm_address.into()
	}
}
