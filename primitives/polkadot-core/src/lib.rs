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

pub mod parachains;

/// Re-export `time_units` to make usage easier.
pub use time_units::*;
/// Human readable time units defined in terms of number of blocks.
pub mod time_units {
	use super::BlockNumber;

	pub const MILLISECS_PER_BLOCK: u64 = 6000;
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;
}

pub use frame_support::{weights::constants::ExtrinsicBaseWeight, Parameter};
pub use sp_runtime::{traits::Convert, Perbill};

// crates.io
use codec::Compact;
use scale_info::{StaticTypeInfo, TypeInfo};
// darwinia-network
use bp_messages::MessageNonce;
use bp_runtime::{Chain, EncodedOrDecodedCall};
// substrate
use frame_support::{
	dispatch::{DispatchClass, Dispatchable},
	parameter_types,
	weights::{
		constants::{BlockExecutionWeight, WEIGHT_REF_TIME_PER_SECOND},
		Weight,
	},
	RuntimeDebug,
};
use frame_system::limits;
use sp_core::Hasher as HasherT;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, DispatchInfoOf, IdentifyAccount, Verify},
	transaction_validity::TransactionValidityError,
	MultiAddress, MultiSignature, OpaqueExtrinsic,
};

/// Block number type used in Polkadot-like chains.
pub type BlockNumber = u32;

/// Hash type used in Polkadot-like chains.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// Account Index (a.k.a. nonce).
pub type Index = u32;

/// Hashing type.
pub type Hashing = BlakeTwo256;

/// The type of object that can produce hashes on Polkadot-like chains.
pub type Hasher = BlakeTwo256;

/// The header type used by Polkadot-like chains.
pub type Header = generic::Header<BlockNumber, Hasher>;

/// Signature type used by Polkadot-like chains.
pub type Signature = MultiSignature;

/// Public key of account on Polkadot-like chains.
pub type AccountPublic = <Signature as Verify>::Signer;

/// Id of account on Polkadot-like chains.
pub type AccountId = <AccountPublic as IdentifyAccount>::AccountId;

/// Address of account on Polkadot-like chains.
pub type AccountAddress = MultiAddress<AccountId, ()>;

/// Index of a transaction on the Polkadot-like chains.
pub type Nonce = u32;

/// Block type of Polkadot-like chains.
pub type Block = generic::Block<Header, OpaqueExtrinsic>;

/// Polkadot-like block signed with a Justification.
pub type SignedBlock = generic::SignedBlock<Block>;

/// The balance of an account on Polkadot-like chain.
pub type Balance = u128;

/// Unchecked Extrinsic type.
pub type UncheckedExtrinsic<Call> = generic::UncheckedExtrinsic<
	AccountAddress,
	EncodedOrDecodedCall<Call>,
	Signature,
	SignedExtensions<Call>,
>;

/// Account address, used by the Polkadot-like chain.
pub type Address = MultiAddress<AccountId, ()>;

/// A type of the data encoded as part of the transaction.
pub type SignedExtra =
	((), (), (), (), sp_runtime::generic::Era, Compact<Nonce>, (), Compact<Balance>);

/// Parameters which are part of the payload used to produce transaction signature,
/// but don't end up in the transaction itself (i.e. inherent part of the runtime).
pub type AdditionalSigned = ((), u32, u32, Hash, Hash, (), (), ());

/// We assume that an on-initialize consumes 1% of the weight on average, hence a single extrinsic
/// will not be allowed to consume more than `AvailableBlockRatio - 1%`.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(1);
/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used
/// by  Operational  extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
/// We allow for 2 seconds of compute with a 6 second average block time.
/// The storage proof size is not limited so far.
pub const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_mul(2), u64::MAX);

parameter_types! {
	/// All Polkadot-like chains have maximal block size set to 5MB.
	///
	/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
	pub BlockLength: limits::BlockLength = limits::BlockLength::max_with_normal_ratio(
		5 * 1024 * 1024,
		NORMAL_DISPATCH_RATIO,
	);
	/// All Polkadot-like chains have the same block weights.
	///
	/// This is a copy-paste from the Polkadot repo's `polkadot-runtime-common` crate.
	pub BlockWeights: limits::BlockWeights = limits::BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have an extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT,
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
}

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
	_data: sp_std::marker::PhantomData<Call>,
}
impl<Call> codec::Encode for SignedExtensions<Call> {
	fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
		self.encode_payload.using_encoded(f)
	}
}
impl<Call> codec::Decode for SignedExtensions<Call> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		SignedExtra::decode(input).map(|encode_payload| SignedExtensions {
			encode_payload,
			additional_signed: None,
			_data: Default::default(),
		})
	}
}
impl<Call> SignedExtensions<Call> {
	pub fn new(
		spec_version: u32,
		transaction_version: u32,
		era: bp_runtime::TransactionEraOf<PolkadotLike>,
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
}
impl<Call> SignedExtensions<Call> {
	/// Return signer nonce, used to craft transaction.
	pub fn nonce(&self) -> Nonce {
		self.encode_payload.5.into()
	}

	/// Return transaction tip.
	pub fn tip(&self) -> Balance {
		self.encode_payload.7.into()
	}
}
impl<Call> sp_runtime::traits::SignedExtension for SignedExtensions<Call>
where
	Call: codec::Codec + sp_std::fmt::Debug + Sync + Send + Clone + Eq + PartialEq + StaticTypeInfo,
	Call: Dispatchable,
{
	type AccountId = AccountId;
	type AdditionalSigned = AdditionalSigned;
	type Call = Call;
	type Pre = ();

	const IDENTIFIER: &'static str = "Not needed.";

	fn additional_signed(
		&self,
	) -> Result<Self::AdditionalSigned, frame_support::unsigned::TransactionValidityError> {
		// we shall not ever see this error in relay, because we are never signing decoded
		// transactions. Instead we're constructing and signing new transactions. So the error code
		// is kinda random here
		self.additional_signed.ok_or(frame_support::unsigned::TransactionValidityError::Unknown(
			frame_support::unsigned::UnknownTransaction::Custom(0xFF),
		))
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

/// Polkadot-like chain.
#[derive(RuntimeDebug)]
pub struct PolkadotLike;
impl Chain for PolkadotLike {
	type AccountId = AccountId;
	type Balance = Balance;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;
	type Index = Index;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeights::get().get(DispatchClass::Normal).max_extrinsic.unwrap_or(Weight::MAX)
	}
}
