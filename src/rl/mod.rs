//! A co-pilot bot for two-player games: the observation it plays from, the
//! training environment server, and the policy that runs in the game.
//! Training lives in tools/rl (see its README).

pub mod env;
pub mod obs;
pub mod server;
pub mod policy;
