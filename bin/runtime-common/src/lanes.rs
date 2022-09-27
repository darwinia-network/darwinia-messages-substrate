use bp_messages::LaneId;

/// Identifier of bridge between Darwinia and Crab.
pub const DARWINIA_CRAB_LANE: LaneId = [0; 4];

// Identifier of bridge between Pangoro and Pangolin.
pub const PANGORO_PANGOLIN_LANE: LaneId = *b"roli";

/// Identifier of bridge between Pangolin and Pangolin Parachain.
pub const PANGOLIN_PANGOLIN_PARACHAIN_LANE: LaneId = *b"pali";

/// Identifier of bridge between Pangolin and Pangolin Parachain Alpha.
pub const PANGOLIN_PANGOLIN_PARACHAIN_ALPHA_LANE: LaneId = *b"phli";

/// Identifier of bridge between Crab and Crab Parachain.
pub const CRAB_CRAB_PARACHAIN_LANE: LaneId = *b"pacr";
