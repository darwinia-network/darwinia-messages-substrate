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

// From construct_runtime macro
#![allow(clippy::from_over_into)]

pub use bp_bsc::signatures::secret_to_address;

use crate::{BSCConfiguration, Config, GenesisConfig as CrateGenesisConfig};
use bp_bsc::{Address, BSCHeader, H256, U256};
use frame_support::{parameter_types, traits::UnixTime, weights::Weight};
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub type AccountId = u64;

type Block = frame_system::mocking::MockBlock<TestRuntime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;

use crate as pallet_bsc;

frame_support::construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		BSC: pallet_bsc::{Pallet, Config, Call},
	}
}

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: Weight = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Config for TestRuntime {
	type Origin = Origin;
	type Index = u64;
	type Call = Call;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = SubstrateHeader;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = ();
	type SystemWeightInfo = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type SS58Prefix = ();
	type OnSetCode = ();
}

parameter_types! {
	pub TestBSCConfiguration: BSCConfiguration = test_bsc_config();
}

impl Config for TestRuntime {
	type BSCConfiguration = TestBSCConfiguration;
	type UnixTime = ConstChainTime;
	type OnHeadersSubmitted = ();
}

/// Test context.
pub struct TestContext {
	/// Initial (genesis) header.
	pub genesis: BSCHeader,
}

/// BSC configuration that is used in tests by default.
pub fn test_bsc_config() -> BSCConfiguration {
	BSCConfiguration {
		min_gas_limit: 0x1388.into(),
		max_gas_limit: U256::max_value(),
		period: 0x03,       // 3s
		epoch_length: 0xc8, // 200
	}
}

/// Genesis header that is used in tests by default.
pub fn genesis() -> BSCHeader {
	let j_h7705800 = r#"
	{
		"difficulty": "0x2",
		"extraData": "0xd883010100846765746888676f312e31352e35856c696e7578000000fc3ca6b72465176c461afb316ebc773c61faee85a6515daa295e26495cef6f69dfa69911d9d8e4f3bbadb89b29a97c6effb8a411dabc6adeefaa84f5067c8bbe2d4c407bbe49438ed859fe965b140dcf1aab71a93f349bbafec1551819b8be1efea2fc46ca749aa14430b3230294d12c6ab2aac5c2cd68e80b16b581685b1ded8013785d6623cc18d214320b6bb6475970f657164e5b75689b64b7fd1fa275f334f28e1872b61c6014342d914470ec7ac2975be345796c2b7ae2f5b9e386cd1b50a4550696d957cb4900f03a8b6c8fd93d6f4cea42bbb345dbc6f0dfdb5bec739bb832254baf4e8b4cc26bd2b52b31389b56e98b9f8ccdafcc39f3c7d6ebf637c9151673cbc36b88a6f79b60359f141df90a0c745125b131caaffd12b8f7166496996a7da21cf1f1b04d9b3e26a3d077be807dddb074639cd9fa61b47676c064fc50d62cce2fd7544e0b2cc94692d4a704debef7bcb61328e2d3a739effcd3a99387d015e260eefac72ebea1e9ae3261a475a27bb1028f140bc2a7c843318afdea0a6e3c511bbd10f4519ece37dc24887e11b55dee226379db83cffc681495730c11fdde79ba4c0c0670403d7dfc4c816a313885fe04b850f96f27b2e9fd88b147c882ad7caf9b964abfe6543625fcca73b56fe29d3046831574b0681d52bf5383d6f2187b6276c100",
		"gasLimit": "0x38ff37a",
		"gasUsed": "0x1364017",
		"logsBloom": "0x2c30123db854d838c878e978cd2117896aa092e4ce08f078424e9ec7f2312f1909b35e579fb2702d571a3be04a8f01328e51af205100a7c32e3dd8faf8222fcf03f3545655314abf91c4c0d80cea6aa46f122c2a9c596c6a99d5842786d40667eb195877bbbb128890a824506c81a9e5623d4355e08a16f384bf709bf4db598bbcb88150abcd4ceba89cc798000bdccf5cf4d58d50828d3b7dc2bc5d8a928a32d24b845857da0b5bcf2c5dec8230643d4bec452491ba1260806a9e68a4a530de612e5c2676955a17400ce1d4fd6ff458bc38a8b1826e1c1d24b9516ef84ea6d8721344502a6c732ed7f861bb0ea017d520bad5fa53cfc67c678a2e6f6693c8ee",
		"miner": "0xe9ae3261a475a27bb1028f140bc2a7c843318afd",
		"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
		"nonce": "0x0000000000000000",
		"number": "0x7594c8",
		"parentHash": "0x5cb4b6631001facd57be810d5d1383ee23a31257d2430f097291d25fc1446d4f",
		"receiptsRoot": "0x1bfba16a9e34a12ff7c4b88be484ccd8065b90abea026f6c1f97c257fdb4ad2b",
		"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
		"stateRoot": "0xa6cd7017374dfe102e82d2b3b8a43dbe1d41cc0e4569f3dc45db6c4e687949ae",
		"timestamp": "0x60ac7137",
		"transactionsRoot": "0x657f5876113ac9abe5cf0460aa8d6b3b53abfc336cea4ab3ee594586f8b584ca",
	  }"#;

	BSCHeader::from_str_unchecked(j_h7705800)
}

/// Run test with default genesis header.
pub fn run_test<T>(test: impl FnOnce(TestContext) -> T) -> T {
	run_test_with_genesis(genesis(), test)
}

/// Run test with default genesis header.
pub fn run_test_with_genesis<T>(genesis: BSCHeader, test: impl FnOnce(TestContext) -> T) -> T {
	sp_io::TestExternalities::new(
		CrateGenesisConfig {
			initial_header: genesis.clone(),
			..Default::default()
		}
		.build_storage::<TestRuntime>()
		.unwrap(),
	)
	.execute_with(|| test(TestContext { genesis }))
}

/// Constant chain time
#[derive(Default)]
pub struct ConstChainTime;

impl UnixTime for ConstChainTime {
	fn now() -> Duration {
		SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()
	}
}
