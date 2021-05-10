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

use bp_eth_clique::Address;
use primitive_types::H256;
use sp_runtime::RuntimeDebug;
use std::fmt;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Error indicating an expected value was not found.
pub struct Mismatch<T> {
	/// Value expected.
	pub expect: T,
	/// Value found.
	pub found: T,
}

impl<T: fmt::Display> fmt::Display for Mismatch<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_fmt(format_args!("Expected {}, found {}", self.expected, self.found))
	}
}

/// Header import error.
#[derive(Clone, Copy, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub enum Error {
	/// The header is beyond last finalized and can not be imported.
	AncientHeader,
	/// The header is already imported.
	KnownHeader,
	/// Seal has an incorrect format.
	InvalidSealArity,
	/// Block number isn't sensible.
	RidiculousNumber,
	/// Block has too much gas used.
	TooMuchGasUsed,
	/// Gas limit header field is invalid.
	InvalidGasLimit,
	/// Extra data is of an invalid length.
	ExtraDataOutOfBounds,
	/// Timestamp header overflowed.
	TimestampOverflow,
	/// The parent header is missing from the blockchain.
	MissingParentBlock,
	/// Validation proof insufficient.
	InsufficientProof,
	/// Difficulty header field is invalid.
	InvalidDifficulty,
	/// The received block is from an incorrect proposer.
	NotValidator,
	/// Missing transaction receipts for the operation.
	MissingTransactionsReceipts,
	/// Redundant transaction receipts are provided.
	RedundantTransactionsReceipts,
	/// Provided transactions receipts are not matching the header.
	TransactionsReceiptsMismatch,
	/// Can't accept unsigned header from the far future.
	UnsignedTooFarInTheFuture,
	/// Trying to finalize sibling of finalized block.
	TryingToFinalizeSibling,
	/// Header timestamp is ahead of on-chain timestamp
	HeaderTimestampIsAhead,
	/// extra-data 32 byte vanity prefix missing
	/// MissingVanity is returned if a block's extra-data section is shorter than
	/// 32 bytes, which is required to store the validator(signer) vanity.
	MissingVanity,
	/// extra-data 65 byte signature suffix missing
	/// MissingSignature is returned if a block's extra-data section doesn't seem
	/// to contain a 65 byte secp256k1 signature
	MissingSignature,
	/// non-checkpoint block contains extra validator list
	/// ExtraValidators is returned if non-checkpoint block contain validator data in
	/// their extra-data fields
	ExtraValidators,
	/// Invalid validator list on checkpoint block
	/// errInvalidCheckpointValidators is returned if a checkpoint block contains an
	/// invalid list of validators (i.e. non divisible by 20 bytes).
	InvalidCheckpointValidators,
	/// Non-zero mix digest
	/// InvalidMixDigest is returned if a block's mix digest is non-zero.
	InvalidMixDigest,
	/// Non empty uncle hash
	/// InvalidUncleHash is returned if a block contains an non-empty uncle list.
	InvalidUncleHash,
	/// Non empty nonce
	/// InvalidNonce is returned if a block header nonce is non-empty
	InvalidNonce,
	/// UnknownAncestor is returned when validating a block requires an ancestor that is unknown.
	UnknownAncestor,
	/// Header timestamp too close
	/// HeaderTimestampTooClose is returned when header timestamp is too close with parent's
	HeaderTimestampTooClose,
	/// Missing signers
	CheckpointNoSigner,
	/// Signature or author field does not belong to an authority.
	NotAuthorized(Address),
	/// The signer signed a block too recently
	TooRecentlySigned(Address),
	/// Parent given is unknown.
	UnknownParent(H256),
	/// Checkpoint is missing
	MissingCheckpoint(H256),
}

impl Error {
	pub fn msg(&self) -> &'static str {
		match *self {
			Error::AncientHeader => "Header is beyound last finalized and can not be imported",
			Error::KnownHeader => "Header is already imported",
			Error::InvalidSealArity => "Header has an incorrect seal",
			Error::RidiculousNumber => "Header has too large number",
			Error::TooMuchGasUsed => "Header has too much gas used",
			Error::InvalidGasLimit => "Header has invalid gas limit",
			Error::ExtraDataOutOfBounds => "Header has too large extra data",
			Error::TimestampOverflow => "Header has too large timestamp",
			Error::MissingParentBlock => "Header has unknown parent hash",
			Error::MissingStep => "Header is missing step seal",
			Error::MissingEmptySteps => "Header is missing empty steps seal",
			Error::DoubleVote => "Header has invalid step in seal",
			Error::InsufficientProof => "Header has insufficient proof",
			Error::InvalidDifficulty => "Header has invalid difficulty",
			Error::NotValidator => "Header is sealed by unexpected validator",
			Error::MissingTransactionsReceipts => "The import operation requires transactions receipts",
			Error::RedundantTransactionsReceipts => "Redundant transactions receipts are provided",
			Error::TransactionsReceiptsMismatch => "Invalid transactions receipts provided",
			Error::UnsignedTooFarInTheFuture => "The unsigned header is too far in future",
			Error::TryingToFinalizeSibling => "Trying to finalize sibling of finalized block",
			Error::HeaderTimestampIsAhead => "Header timestamp is ahead of on-chain timestamp",
			Error::MissingVanity => "Extra-data 32 byte vanity prefix missing",
			Error::MissingSignature => "Extra-data 65 byte signature suffix missing",
			Error::ExtraValidators => "Non-checkpoint block contains extra validator list",
			Error::InvalidCheckpointValidators => "Invalid validator list on checkpoint block",
			Error::InvalidMixDigest => "Non-zero mix digest",
			Error::InvalidUncleHash => "Non empty uncle hash",
			Error::InvalidNonce => "Non empty nonce",
			Error::UnknownAncestor => "Unknow ancestor",
			Error::HeaderTimestampTooClose => "Header timestamp too close",
			Error::CheckpointNoSigner => "Missing signers",
			Error::NotAuthorized(address) => format!("Address {} not authorized", address),
			Error::TooRecentlySigned(signer) => format!("The signer {} signed a block too recently", signer),
			Error::UnknownParent(parent) => format!("Unknown parent {}", parent),
			Error::MissingCheckpoint(hash) => format!("Missing checkpoint {}", hash),
			_ => "Unknown error.",
		}
	}

	/// Return unique error code.
	pub fn code(&self) -> u8 {
		*self as u8
	}
}
