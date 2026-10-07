//! CleanYourMac core: discovery modules, scope and identity policy, reviewed actions and
//! analytics. Platform work goes through the replaceable traits in [`ports`]; default
//! implementations live in [`adapters`] and are bundled by [`services::Services`].
pub mod actions;
pub mod adapters;
pub mod analytics;
pub mod engine;
pub mod ffi;
pub mod git;
pub mod model;
pub mod modules;
pub mod orphans;
pub mod policy;
pub mod ports;
pub mod results;
pub mod services;
pub mod simulator;

pub use engine::Engine;
pub use services::Services;
