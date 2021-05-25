// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of OpenEthereum.

// OpenEthereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// OpenEthereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with OpenEthereum.  If not, see <http://www.gnu.org/licenses/>.

use crate::error::Error;

use bp_bsc::{public_to_address, Address, BSCHeader, ADDRESS_LENGTH, H160, SIGNATURE_LENGTH, VANITY_LENGTH};
use sp_io::crypto::secp256k1_ecdsa_recover;

/// Recover block creator from signature
pub fn recover_creator(header: &BSCHeader) -> Result<Address, Error> {
	let data = &header.extra_data;
	if data.len() < VANITY_LENGTH {
		return Err(Error::MissingVanity);
	}

	if data.len() < VANITY_LENGTH + SIGNATURE_LENGTH {
		return Err(Error::MissingSignature);
	}

	// Split `signed_extra data` and `signature`
	let (signed_data_slice, signature_slice) = data.split_at(data.len() - SIGNATURE_LENGTH);

	// convert `&[u8]` to `[u8; 65]`
	let signature = {
		let mut s = [0; SIGNATURE_LENGTH];
		s.copy_from_slice(signature_slice);
		s
	};

	// modify header and hash it
	let unsigned_header = &mut header.clone();
	unsigned_header.extra_data = signed_data_slice.to_vec();
	let msg = unsigned_header.compute_hash();

	let pubkey = secp256k1_ecdsa_recover(&signature, msg.as_fixed_bytes()).map_err(|_| Error::RecoverPubkeyFail)?;
	let creator = public_to_address(&pubkey);

	Ok(creator)
}

/// Extract authority set from extra_data.
///
/// Layout of extra_data:
/// ----
/// VANITY: 32 bytes
/// Signers: N * 32 bytes as hex encoded (20 characters)
/// Signature: 65 bytes
/// --
pub fn extract_authorities(header: &BSCHeader) -> Result<Vec<Address>, Error> {
	let data = &header.extra_data;

	if data.len() <= VANITY_LENGTH + SIGNATURE_LENGTH {
		return Err(Error::CheckpointNoSigner);
	}

	// extract only the portion of extra_data which includes the signer list
	let signers_raw = &data[(VANITY_LENGTH)..data.len() - (SIGNATURE_LENGTH)];

	if signers_raw.len() % ADDRESS_LENGTH != 0 {
		return Err(Error::CheckpointInvalidSigners(signers_raw.len()));
	}

	let num_signers = signers_raw.len() / 20;

	let signers: Vec<Address> = (0..num_signers)
		.map(|i| {
			let start = i * ADDRESS_LENGTH;
			let end = start + ADDRESS_LENGTH;
			H160::from_slice(&signers_raw[start..end])
		})
		.collect();

	Ok(signers)
}
