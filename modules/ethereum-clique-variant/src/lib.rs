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

use crate::{
	finality::{CachedFinalityVotes, FinalityVotes},
	snapshot::Snapshot,
};
use bp_eth_clique::{Address, CliqueHeader, HeaderId, RawTransaction};
use codec::{Decode, Encode};
use frame_support::{decl_module, decl_storage, traits::Get};
use primitive_types::{H256, U256};
use sp_runtime::{
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionPriority, TransactionSource, TransactionValidity,
		UnknownTransaction, ValidTransaction,
	},
	RuntimeDebug,
};
use sp_std::{cmp::Ord, collections::btree_map::BTreeMap, prelude::*};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time_utils::CheckedSystemTime;

mod error;
mod finality;
mod import;
mod snapshot;
mod utils;
mod verification;

/// Maximal number of blocks we're pruning in single import call.
/// CHECKME
const MAX_BLOCKS_TO_PRUNE_IN_SINGLE_IMPORT: u64 = 8;

/// CliqueVariant engine configuration parameters.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct CliqueVariantConfiguration {
	/// First block for which a 2/3 quorum (instead of 1/2) is required.
	pub two_thirds_majority_transition: u64,
	/// Minimum gas limit.
	pub min_gas_limit: U256,
	/// Maximum gas limit.
	pub max_gas_limit: U256,
	/// epoch length
	pub epoch_length: u32,
	/// HashLength is the expected length of the hash
	pub hash_length: u32,
	/// block period
	pub period: u32,
}

/// Transaction pool configuration.
///
/// This is used to limit number of unsigned headers transactions in
/// the pool. We never use it to verify signed transactions.
pub struct PoolConfiguration {
	/// Maximal difference between number of header from unsigned transaction
	/// and current best block. This must be selected with caution - the more
	/// is the difference, the more (potentially invalid) transactions could be
	/// accepted to the pool and mined later (filling blocks with spam).
	pub max_future_number_difference: u64,
}

/// Block header as it is stored in the runtime storage.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct StoredHeader<Submitter> {
	/// Submitter of this header. May be `None` if header has been submitted
	/// using unsigned transaction.
	pub submitter: Option<Submitter>,
	/// The block header itself.
	pub header: CliqueHeader,
	/// Total difficulty of the chain.
	pub total_difficulty: U256,
}

/// Header that we're importing.
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct HeaderToImport<Submitter> {
	/// Header import context,
	pub context: ImportContext<Submitter>,
	/// Should we consider this header as best?
	pub is_best: bool,
	/// The id of the header.
	pub id: HeaderId,
	/// The header itself.
	pub header: CliqueHeader,
	/// Total chain difficulty at the header.
	pub total_difficulty: U256,
}

/// Blocks range that we want to prune.
#[derive(Encode, Decode, Default, RuntimeDebug, Clone, PartialEq)]
struct PruningRange {
	/// Number of the oldest unpruned block(s). This might be the block that we do not
	/// want to prune now (then it is equal to `oldest_block_to_keep`), or block that we
	/// were unable to prune for whatever reason (i.e. if it isn't finalized yet and has
	/// scheduled validators set change).
	pub oldest_unpruned_block: u64,
	/// Number of oldest block(s) that we want to keep. We want to prune blocks in range
	/// [`oldest_unpruned_block`; `oldest_block_to_keep`).
	pub oldest_block_to_keep: u64,
}

/// Header import context.
///
/// The import context contains information needed by the header verification
/// pipeline which is not directly part of the header being imported. This includes
/// information relating to its parent, and the current validator set (which
/// provide _context_ for the current header).
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct ImportContext<Submitter> {
	submitter: Option<Submitter>,
	parent_hash: H256,
	parent_header: CliqueHeader,
	parent_total_difficulty: U256,
}

impl<Submitter> ImportContext<Submitter> {
	/// Returns reference to header submitter (if known).
	pub fn submitter(&self) -> Option<&Submitter> {
		self.submitter.as_ref()
	}

	/// Returns reference to parent header.
	pub fn parent_header(&self) -> &CliqueHeader {
		&self.parent_header
	}

	/// Returns total chain difficulty at parent block.
	pub fn total_difficulty(&self) -> &U256 {
		&self.parent_total_difficulty
	}

	/// Converts import context into header we're going to import.
	#[allow(clippy::too_many_arguments)]
	pub fn into_import_header(
		self,
		is_best: bool,
		id: HeaderId,
		header: CliqueHeader,
		total_difficulty: U256,
	) -> HeaderToImport<Submitter> {
		HeaderToImport {
			context: self,
			is_best,
			id,
			header,
			total_difficulty,
		}
	}
}

/// The storage that is used by the client.
///
/// Storage modification must be discarded if block import has failed.
pub trait Storage {
	/// Header submitter identifier.
	type Submitter: Clone + Ord;

	/// Get best known block and total chain difficulty.
	fn best_block(&self) -> (HeaderId, U256);
	/// Get last finalized block.
	fn finalized_block(&self) -> HeaderId;
	/// Get imported header by its hash.
	///
	/// Returns header and its submitter (if known).
	fn header(&self, hash: &H256) -> Option<(CliqueHeader, Option<Self::Submitter>)>;
	/// Get header import context by parent header hash.
	fn import_context(
		&self,
		submitter: Option<Self::Submitter>,
		parent_hash: &H256,
	) -> Option<ImportContext<Self::Submitter>>;
	/// Insert imported header.
	fn insert_header(&mut self, header: HeaderToImport<Self::Submitter>);
	/// Finalize given block and schedules pruning of all headers
	/// with number < prune_end.
	///
	/// The headers in the pruning range could be either finalized, or not.
	/// It is the storage duty to ensure that unfinalized headers that have
	/// scheduled changes won't be pruned until they or their competitors
	/// are finalized.
	fn finalize_and_prune_headers(&mut self, finalized: Option<HeaderId>, prune_end: u64);
}

/// Headers pruning strategy.
pub trait PruningStrategy: Default {
	/// Return upper bound (exclusive) of headers pruning range.
	///
	/// Every value that is returned from this function, must be greater or equal to the
	/// previous value. Otherwise it will be ignored (we can't revert pruning).
	///
	/// Pallet may prune both finalized and unfinalized blocks. But it can't give any
	/// guarantees on when it will happen. Example: if some unfinalized block at height N
	/// is checkpoint block, then the module won't prune any blocks with
	/// number >= N even if strategy allows that.
	///
	/// If your strategy allows pruning unfinalized blocks, this could lead to switch
	/// between finalized forks (only if authorities are misbehaving). But since V/2 + 1
	/// validators are able to do whatever they want with the chain, this isn't considered
	/// fatal. If your strategy only prunes finalized blocks, we'll never be able to finalize
	/// header that isn't descendant of current best finalized block.
	fn pruning_upper_bound(&mut self, best_number: u64, best_finalized_number: u64) -> u64;
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
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or_default(Duration::from_secs(0));

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
	fn on_valid_headers_submitted(submitter: AccountId, useful: u64, useless: u64);
	/// Called when invalid headers have been submitted.
	fn on_invalid_headers_submitted(submitter: AccountId);
	/// Called when earlier submitted headers have been finalized.
	///
	/// finalized is the number of headers that submitter has submitted and which
	/// have been finalized.
	fn on_valid_headers_finalized(submitter: AccountId, finalized: u64);
}

impl<AccountId> OnHeadersSubmitted<AccountId> for () {
	fn on_valid_headers_submitted(_submitter: AccountId, _useful: u64, _useless: u64) {}
	fn on_invalid_headers_submitted(_submitter: AccountId) {}
	fn on_valid_headers_finalized(_submitter: AccountId, _finalized: u64) {}
}

/// The module configuration trait.
pub trait Config<I = DefaultInstance>: frame_system::Config {
	/// CliqueVariant configuration.
	type CliqueVariantConfiguration: Get<CliqueVariantConfiguration>;
	/// Headers pruning strategy.
	type PruningStrategy: PruningStrategy;
	/// Header timestamp verification against current on-chain time.
	type ChainTime: ChainTime;
	/// Handler for headers submission result.
	type OnHeadersSubmitted: OnHeadersSubmitted<Self::AccountId>;
}

decl_module! {
	pub struct Module<T: Config<I>, I: Instance = DefaultInstance> for enum Call where origin: T::Origin {
		/// Import single CliqueVariant header. Requires transaction to be **UNSIGNED**.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn import_unsigned_header(origin, header: CliqueHeader) {
			frame_system::ensure_none(origin)?;

			import::import_header(
				&mut BridgeStorage::<T, I>::new(),
				&mut T::PruningStrategy::default(),
				&T::CliqueVariantConfiguration::get(),
				None,
				header,
				&T::ChainTime::default(),
			).map_err(|e| e.msg())?;
		}

		/// Import CliqueVariant chain headers in a single **SIGNED** transaction.
		/// Ignores non-fatal errors (like when known header is provided), rewards
		/// for successful headers import and penalizes for fatal errors as you wish.
		///
		/// This should be used with caution - passing too many headers could lead to
		/// enormous block production/import time.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn import_signed_headers(origin, headers: Vec<liqueHeader>) {
			let submitter = frame_system::ensure_signed(origin)?;
			let mut finalized_headers = BTreeMap::new();
			let import_result = import::import_headers(
				&mut BridgeStorage::<T, I>::new(),
				&mut T::PruningStrategy::default(),
				&T::CliqueVariantConfiguration::get(),
				Some(submitter.clone()),
				&T::ChainTime::default(),
				&mut finalized_headers,
			);

			// if we have finalized some headers, we will reward their submitters even
			// if current submitter has provided some invalid headers
			for (f_submitter, f_count) in finalized_headers {
				T::OnHeadersSubmitted::on_valid_headers_finalized(
					f_submitter,
					f_count,
				);
			}

			// now track/penalize current submitter for providing new headers
			match import_result {
				Ok((useful, useless)) =>
					T::OnHeadersSubmitted::on_valid_headers_submitted(submitter, useful, useless),
				Err(error) => {
					// even though we may have accept some headers, we do not want to reward someone
					// who provides invalid headers
					T::OnHeadersSubmitted::on_invalid_headers_submitted(submitter);
					return Err(error.msg().into());
				},
			}
		}
	}
}

decl_storage! {
	trait Store for Pallet<T: Config<I>, I: Instance = DefaultInstance> as Bridge {
		/// Best known block.
		BestBlock: (HeaderId, U256);
		/// Best finalized block.
		FinalizedBlock: HeaderId;
		/// Range of blocks that we want to prune.
		BlocksToPrune: PruningRange;
		/// Map of imported headers by hash.
		Headers: map hasher(identity) H256 => Option<StoredHeader<T::AccountId>>;
		/// Map of imported header hashes by number.
		HeadersByNumber: map hasher(blake2_128_concat) u64 => Option<Vec<H256>>;
	}
	add_extra_genesis {
		config(initial_header): CliqueHeader;
		config(initial_total_difficulty): U256;
		config(initial_validators): Vec<Address>;
		build(|config| {
			assert!(
				!config.initial_validators.is_empty(),
				"Initial validators set can't be empty",
			);

			initialize_storage::<T, I>(
				&config.initial_header,
				config.initial_total_difficulty, // CHECKME it should be total difficulity right?
				&config.initial_validators,
			);
		})
	}
}

impl<T: Config<I>, I: Instance> Pallet<T, I> {
	/// Returns number and hash of the best block known to the bridge module.
	/// The caller should only submit `import_header` transaction that makes
	/// (or leads to making) other header the best one.
	pub fn best_block() -> HeaderId {
		BridgeStorage::<T, I>::new().best_block().0
	}

	/// Returns number and hash of the best finalized block known to the bridge module.
	pub fn finalized_block() -> HeaderId {
		BridgeStorage::<T, I>::new().finalized_block()
	}

	/// Returns true if header is known to the runtime.
	pub fn is_known_block(hash: H256) -> bool {
		BridgeStorage::<T, I>::new().header(&hash).is_some()
	}

	/// Verify that transaction is included into given finalized block.
	pub fn verify_transaction_finalized(block: H256, tx_index: u64, proof: &[RawTransaction]) -> bool {
		crate::verify_transaction_finalized(&BridgeStorage::<T, I>::new(), block, tx_index)
	}
}

impl<T: Config<I>, I: Instance> frame_support::unsigned::ValidateUnsigned for Pallet<T, I> {
	type Call = Call<T, I>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		match *call {
			Self::Call::import_unsigned_header(ref header) => {
				let accept_result = verification::accept_clique_header_into_pool(
					&BridgeStorage::<T, I>::new(),
					&T::CliqueVariantConfiguration::get(),
					&pool_configuration(),
					header,
					&T::ChainTime::default(),
				);

				match accept_result {
					Ok((requires, provides)) => Ok(ValidTransaction {
						priority: TransactionPriority::max_value(),
						requires,
						provides,
						longevity: TransactionLongevity::max_value(),
						propagate: true,
					}),
					// UnsignedTooFarInTheFuture is the special error code used to limit
					// number of transactions in the pool - we do not want to ban transaction
					// in this case (see verification.rs for details)
					Err(error::Error::UnsignedTooFarInTheFuture) => {
						UnknownTransaction::Custom(error::Error::UnsignedTooFarInTheFuture.code()).into()
					}
					Err(error) => InvalidTransaction::Custom(error.code()).into(),
				}
			}
			_ => InvalidTransaction::Call.into(),
		}
	}
}

/// Runtime bridge storage.
#[derive(Default)]
pub struct BridgeStorage<T, I = DefaultInstance>(sp_std::marker::PhantomData<(T, I)>);

impl<T: Config<I>, I: Instance> BridgeStorage<T, I> {
	/// Create new BridgeStorage.
	pub fn new() -> Self {
		BridgeStorage(sp_std::marker::PhantomData::<(T, I)>::default())
	}

	/// Prune old blocks.
	fn prune_blocks(&self, mut max_blocks_to_prune: u64, finalized_number: u64, prune_end: u64) {
		let pruning_range = BlocksToPrune::<I>::get();
		let mut new_pruning_range = pruning_range.clone();

		// update oldest block we want to keep
		if prune_end > new_pruning_range.oldest_block_to_keep {
			new_pruning_range.oldest_block_to_keep = prune_end;
		}

		// start pruning blocks
		let begin = new_pruning_range.oldest_unpruned_block;
		let end = new_pruning_range.oldest_block_to_keep;
		log::trace!(target: "runtime", "Pruning blocks in range [{}..{})", begin, end);
		for number in begin..end {
			// if we can't prune anything => break
			if max_blocks_to_prune == 0 {
				break;
			}

			// read hashes of blocks with given number and try to prune these blocks
			// CHECKME why we have multiple blocks with same block number?
			let blocks_at_number = HeadersByNumber::<I>::take(number);
			if let Some(mut blocks_at_number) = blocks_at_number {
				self.prune_blocks_by_hashes(
					&mut max_blocks_to_prune,
					finalized_number,
					number,
					&mut blocks_at_number,
					&T::CliqueVariantConfiguration::get(),
				);

				// if we haven't pruned all blocks, remember unpruned
				if !blocks_at_number.is_empty() {
					HeadersByNumber::<I>::insert(number, blocks_at_number);
					break;
				}
			}

			// we have pruned all headers at number
			new_pruning_range.oldest_unpruned_block = number + 1;
			log::trace!(
				target: "runtime",
				"Oldest unpruned clique variant header now at: {}",
				new_pruning_range.oldest_unpruned_block,
			);
		}

		// update pruning range in storage
		if pruning_range != new_pruning_range {
			BlocksToPrune::<I>::put(new_pruning_range);
		}
	}

	/// Prune old blocks with given hashes.
	fn prune_blocks_by_hashes(
		&self,
		max_blocks_to_prune: &mut u64,
		finalized_number: u64,
		number: u64,
		blocks_at_number: &mut Vec<H256>,
		clique_variant_config: &CliqueVariantConfiguration,
	) {
		// ensure that unfinalized headers we want to prune do not have validator changes
		if number > finalized_number
			&& blocks_at_number.iter().any(|block| match self.header(&block) {
				Some((header, _)) => header.number % clique_variant_config.epoch_length, // CHECKME
				None => false,
			}) {
			return;
		}

		// physically remove headers and (probably) obsolete validators sets
		while let Some(hash) = blocks_at_number.pop() {
			let header = Headers::<T, I>::take(&hash);
			log::trace!(
				target: "runtime",
				"Pruning clique variants header: ({}, {})",
				number,
				hash,
			);
			// check if we have already pruned too much headers in this call
			*max_blocks_to_prune -= 1;
			if *max_blocks_to_prune == 0 {
				return;
			}
		}
	}
}

impl<T: Config<I>, I: Instance> Storage for BridgeStorage<T, I> {
	type Submitter = T::AccountId;

	fn best_block(&self) -> (HeaderId, U256) {
		BestBlock::<I>::get()
	}

	fn finalized_block(&self) -> HeaderId {
		FinalizedBlock::<I>::get()
	}

	fn header(&self, hash: &H256) -> Option<(CliqueHeader, Option<Self::Submitter>)> {
		Headers::<T, I>::get(hash).map(|stored_header| (stored_header.header, stored_header.submitter))
	}

	fn import_context(
		&self,
		submitter: Option<Self::Submitter>,
		parent_hash: &H256,
	) -> Option<ImportContext<Self::Submitter>> {
		Headers::<T, I>::get(parent_hash).map(|stored_header| ImportContext {
			submitter,
			parent_hash: *parent_hash,
			parent_header: stored_header.header,
			parent_total_difficulty: stored_header.total_difficulty,
		})
	}

	fn insert_header(&mut self, header: HeaderToImport<Self::Submitter>) {
		if header.is_best {
			BestBlock::<I>::put((header.id, header.total_difficulty));
		}

		log::trace!(
			target: "runtime",
			"Inserting Clique variants header: ({}, {})",
			header.header.number,
			header.id.hash,
		);

		HeadersByNumber::<I>::append(header.id.number, header.id.hash);
		Headers::<T, I>::insert(
			&header.id.hash,
			StoredHeader {
				submitter: header.context.submitter,
				header: header.header,
				total_difficulty: header.total_difficulty,
			},
		);
	}

	fn finalize_and_prune_headers(&mut self, finalized: Option<HeaderId>, prune_end: u64) {
		// remember just finalized block
		let finalized_number = finalized
			.as_ref()
			.map(|f| f.number)
			.unwrap_or_else(|| FinalizedBlock::<I>::get().number);
		if let Some(finalized) = finalized {
			log::trace!(
				target: "runtime",
				"Finalizing Clique variant header: ({}, {})",
				finalized.number,
				finalized.hash,
			);

			FinalizedBlock::<I>::put(finalized);
		}

		// and now prune headers if we need to
		self.prune_blocks(MAX_BLOCKS_TO_PRUNE_IN_SINGLE_IMPORT, finalized_number, prune_end);
	}
}

/// Initialize storage.
#[cfg(any(feature = "std", feature = "runtime-benchmarks"))]
pub(crate) fn initialize_storage<T: Config<I>, I: Instance>(
	initial_header: &CliqueHeader,
	initial_total_difficulty: U256,
	initial_validators: &[Address],
) {
	let initial_hash = initial_header.compute_hash();
	log::trace!(
		target: "runtime",
		"Initializing bridge with Clique variant header: ({}, {})",
		initial_header.number,
		initial_hash,
	);

	let initial_id = HeaderId {
		number: initial_header.number,
		hash: initial_hash,
	};
	BestBlock::<I>::put((initial_id, initial_total_difficulty));
	FinalizedBlock::<I>::put(initial_id);
	BlocksToPrune::<I>::put(PruningRange {
		oldest_unpruned_block: initial_header.number,
		oldest_block_to_keep: initial_header.number,
	});
	HeadersByNumber::<I>::insert(initial_header.number, vec![initial_hash]);
	Headers::<T, I>::insert(
		initial_hash,
		StoredHeader {
			submitter: None,
			header: initial_header.clone(),
			total_difficulty: initial_total_difficulty,
		},
	);
}

/// Verify that transaction is included into given finalized block.
pub fn verify_transaction_finalized<S: Storage>(
	storage: &S,
	block: H256,
	tx_index: u64,
	proof: &[(RawTransaction)],
) -> bool {
	if tx_index >= proof.len() as _ {
		log::trace!(
			target: "runtime",
			"Tx finality check failed: transaction index ({}) is larger than number of transactions ({})",
			tx_index,
			proof.len(),
		);

		return false;
	}

	let header = match storage.header(&block) {
		Some((header, _)) => header,
		None => {
			log::trace!(
				target: "runtime",
				"Tx finality check failed: can't find header in the storage: {}",
				block,
			);

			return false;
		}
	};
	let finalized = storage.finalized_block();

	// if header is not yet finalized => return
	if header.number > finalized.number {
		log::trace!(
			target: "runtime",
			"Tx finality check failed: header {}/{} is not finalized. Best finalized: {}",
			header.number,
			block,
			finalized.number,
		);

		return false;
	}

	// check if header is actually finalized
	let is_finalized = match header.number < finalized.number {
		true => ancestry(storage, finalized.hash)
			.skip_while(|(_, ancestor)| ancestor.number > header.number)
			.any(|(ancestor_hash, _)| ancestor_hash == block),
		false => block == finalized.hash,
	};
	if !is_finalized {
		log::trace!(
			target: "runtime",
			"Tx finality check failed: header {} is not finalized: no canonical path to best finalized block {}",
			block,
			finalized.hash,
		);
		return false;
	}

	// verify that transaction is included in the block
	if let Err(computed_root) = header.check_transactions_root(proof.iter().map(|(tx, _)| tx)) {
		log::trace!(
			target: "runtime",
			"Tx finality check failed: transactions root mismatch. Expected: {}, computed: {}",
			header.transactions_root,
			computed_root,
		);

		return false;
	}

	true
}

/// Transaction pool configuration.
fn pool_configuration() -> PoolConfiguration {
	PoolConfiguration {
		max_future_number_difference: 10,
	}
}

/// Return iterator of given header ancestors.
fn ancestry<S: Storage>(storage: &'_ S, mut parent_hash: H256) -> impl Iterator<Item = (H256, CliqueHeader)> + '_ {
	sp_std::iter::from_fn(move || {
		let (header, _) = storage.header(&parent_hash)?;
		if header.number == 0 {
			return None;
		}

		let hash = parent_hash;
		parent_hash = header.parent_hash;
		Some((hash, header))
	})
}
