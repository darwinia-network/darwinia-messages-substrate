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

//! Parachains finality pallet benchmarking.

use crate::{
	weights_ext::DEFAULT_PARACHAIN_HEAD_SIZE, Call, RelayBlockHash, RelayBlockHasher,
	RelayBlockNumber,
};

use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use bp_runtime::StorageProofSize;
use frame_benchmarking::{account, benchmarks_instance_pallet};
use frame_system::RawOrigin;
use sp_std::prelude::*;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config<I>, I: 'static>(crate::Pallet<T, I>);

/// Trait that must be implemented by runtime to benchmark the parachains finality pallet.
pub trait Config<I: 'static>: crate::Config<I> {
	/// Generate parachain heads proof and prepare environment for verifying this proof.
	fn prepare_parachain_heads_proof(
		parachains: &[ParaId],
		parachain_head_size: u32,
		proof_size: StorageProofSize,
	) -> (RelayBlockNumber, RelayBlockHash, ParaHeadsProof, Vec<(ParaId, ParaHash)>);
}

benchmarks_instance_pallet! {
	where_clause {
		where
			<T as pallet_bridge_grandpa::Config<T::BridgesGrandpaPalletInstance>>::BridgedChain:
				bp_runtime::Chain<
					BlockNumber = RelayBlockNumber,
					Hash = RelayBlockHash,
					Hasher = RelayBlockHasher,
				>,
	}

	// Benchmark `submit_parachain_heads` extrinsic with different number of parachains.
	submit_parachain_heads_with_n_parachains {
		let p in 1..1024;

		let sender = account("sender", 0, 0);
		let parachains = (1..=p).map(ParaId).collect::<Vec<_>>();
		let (relay_block_number, relay_block_hash, parachain_heads_proof, parachains_heads) = T::prepare_parachain_heads_proof(
			&parachains,
			DEFAULT_PARACHAIN_HEAD_SIZE,
			StorageProofSize::Minimal(0),
		);
		let at_relay_block = (relay_block_number, relay_block_hash);
	}: submit_parachain_heads(RawOrigin::Signed(sender), at_relay_block, parachains_heads, parachain_heads_proof)
	verify {
		for parachain in parachains {
			assert!(crate::Pallet::<T, I>::best_parachain_head(parachain).is_some());
		}
	}

	// Benchmark `submit_parachain_heads` extrinsic with 1kb proof size.
	submit_parachain_heads_with_1kb_proof {
		let sender = account("sender", 0, 0);
		let parachains = vec![ParaId(1)];
		let (relay_block_number, relay_block_hash, parachain_heads_proof, parachains_heads) = T::prepare_parachain_heads_proof(
			&parachains,
			DEFAULT_PARACHAIN_HEAD_SIZE,
			StorageProofSize::HasExtraNodes(1024),
		);
		let at_relay_block = (relay_block_number, relay_block_hash);
	}: submit_parachain_heads(RawOrigin::Signed(sender), at_relay_block, parachains_heads, parachain_heads_proof)
	verify {
		for parachain in parachains {
			assert!(crate::Pallet::<T, I>::best_parachain_head(parachain).is_some());
		}
	}

	// Benchmark `submit_parachain_heads` extrinsic with 16kb proof size.
	submit_parachain_heads_with_16kb_proof {
		let sender = account("sender", 0, 0);
		let parachains = vec![ParaId(1)];
		let (relay_block_number, relay_block_hash, parachain_heads_proof, parachains_heads) = T::prepare_parachain_heads_proof(
			&parachains,
			DEFAULT_PARACHAIN_HEAD_SIZE,
			StorageProofSize::HasExtraNodes(16 * 1024),
		);
		let at_relay_block = (relay_block_number, relay_block_hash);
	}: submit_parachain_heads(RawOrigin::Signed(sender), at_relay_block, parachains_heads, parachain_heads_proof)
	verify {
		for parachain in parachains {
			assert!(crate::Pallet::<T, I>::best_parachain_head(parachain).is_some());
		}
	}
}
