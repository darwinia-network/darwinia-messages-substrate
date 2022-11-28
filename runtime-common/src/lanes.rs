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

// darwinia-network
use bp_messages::LaneId;

/// Identifier of bridge between Darwinia and Crab.
pub const DARWINIA_CRAB_LANE: LaneId = [0; 4];

// Identifier of bridge between Pangoro and Pangolin.
pub const PANGORO_PANGOLIN_LANE: LaneId = *b"roli";

/// Identifier of bridge between Pangolin and Pangolin Parachain.
pub const PANGOLIN_PANGOLIN_PARACHAIN_LANE: LaneId = *b"pali";

/// Identifier of bridge between Pangolin and Pangolin Parachain Alpha.
pub const PANGOLIN_PANGOLIN_PARACHAIN_ALPHA_LANE: LaneId = *b"plpa";

/// Identifier of bridge between Crab and Crab Parachain.
pub const CRAB_CRAB_PARACHAIN_LANE: LaneId = *b"pacr";