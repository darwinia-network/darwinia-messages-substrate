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

use crate::error::Error;
use crate::finality::finalize_blocks;
use crate::snapshot::Snapshot;
use crate::verification::{is_importable_header, verify_clique_variant_header};
use crate::{ChainTime, CliqueVariantConfiguration, PruningStrategy, Storage};
use bp_eth_clique::{CliqueHeader, HeaderId};
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

/// Imports bunch of headers and updates blocks finality.
///
/// Transactions receipts are useless for verifying block in clique consensus
/// If successful, returns tuple where first element is the number of useful headers
/// we have imported and the second element is the number of useless headers (duplicate)
/// we have NOT imported.
/// Returns error if fatal error has occured during import. Some valid headers may be
/// imported in this case.
/// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/415)
#[allow(clippy::too_many_arguments)]
pub fn import_headers<S: Storage, PS: PruningStrategy, CT: ChainTime>(
	storage: &mut S,
	pruning_strategy: &mut PS,
	clique_variant_config: &CliqueVariantConfiguration,
	submitter: Option<S::Submitter>,
	headers: Vec<CliqueHeader>,
	chain_time: &CT,
	finalized_headers: &mut BTreeMap<S::Submitter, u64>,
) -> Result<(u64, u64), Error> {
	let mut useful = 0;
	let mut useless = 0;
	for header in headers {
		let import_result = import_header(
			storage,
			pruning_strategy,
			clique_variant_config,
			submitter.clone(),
			header,
			chain_time,
		);

		match import_result {
			Ok((_, finalized)) => {
				for (_, submitter) in finalized {
					if let Some(submitter) = submitter {
						*finalized_headers.entry(submitter).or_default() += 1;
					}
				}
				useful += 1;
			}
			Err(Error::AncientHeader) | Err(Error::KnownHeader) => useless += 1,
			Err(error) => return Err(error),
		}
	}

	Ok((useful, useless))
}

/// A vector of finalized headers and their submitters.
pub type FinalizedHeaders<S> = Vec<(HeaderId, Option<<S as Storage>::Submitter>)>;

/// Imports given header and updates blocks finality (if required).
///
/// Transactions receipts are useless here
///
/// Returns imported block id and list of all finalized headers.
/// TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/415)
#[allow(clippy::too_many_arguments)]
pub fn import_header<S: Storage, PS: PruningStrategy, CT: ChainTime>(
	storage: &mut S,
	pruning_strategy: &mut PS,
	clique_variant_config: &CliqueVariantConfiguration,
	submitter: Option<S::Submitter>,
	header: CliqueHeader,
	chain_time: &CT,
) -> Result<(HeaderId, FinalizedHeaders<S>), Error> {
	// first check that we are able to import this header at all
	let (header_id, finalized_id) = is_importable_header(storage, &header)?;

	// verify header
	let import_context = verify_clique_variant_header(storage, clique_variant_config, submitter, &header, chain_time)?;

	// verify validator
	// Retrieve the parent state
	// TODO how to init snapshot?
	let parent_state = Snapshot::<CT>::new().retrieve(storage, &header.parent_hash, clique_variant_config)?;
	// Try to apply current state, apply() will further check signer and recent signer.
	let mut new_state = parent_state.clone();
	new_state.apply(header, header.number % clique_variant_config.epoch_length == 0)?;
	new_state.calc_next_timestamp(header.timestamp, clique_variant_config.period)?;
	new_state.verify(header)?;

	let finalized_blocks = finalize_blocks(
		storage,
		finalized_id,
		header_id,
		import_context.submitter(),
		&header,
		clique_variant_config.two_thirds_majority_transition,
	)?;

	// NOTE: we can't return Err() from anywhere below this line
	// (because otherwise we'll have inconsistent storage if transaction will fail)

	// and finally insert the block
	let (best_id, best_total_difficulty) = storage.best_block();
	let total_difficulty = import_context.total_difficulty() + header.difficulty;
	let is_best = total_difficulty > best_total_difficulty;
	storage.insert_header(import_context.into_import_header(is_best, header_id, header, total_difficulty));

	// compute upper border of updated pruning range
	let new_best_block_id = if is_best { header_id } else { best_id };
	let new_best_finalized_block_id = finalized_blocks.finalized_headers.last().map(|(id, _)| *id);
	let pruning_upper_bound = pruning_strategy.pruning_upper_bound(
		new_best_block_id.number,
		new_best_finalized_block_id
			.map(|id| id.number)
			.unwrap_or(finalized_id.number),
	);

	// now mark finalized headers && prune old headers
	storage.finalize_and_prune_headers(new_best_finalized_block_id, pruning_upper_bound);

	Ok((header_id, finalized_blocks.finalized_headers))
}
