//! Path N frozen contract schemas (`docs/architecture/LLD.md` §0).
//!
//! These types are the ONLY coupling between swarm units. A unit may import
//! `pathn_contracts` but never another unit's internals.
//!
//! Changing any type here requires a contract version bump plus a `#swarm`
//! announcement (see `docs/architecture/SWARM.md` swarm law 3).

pub mod cpu;
pub mod device;
pub mod machine;
