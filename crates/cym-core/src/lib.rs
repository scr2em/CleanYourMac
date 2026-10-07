//! CleanYourMac core: discovery modules, scope and identity policy, reviewed actions and
//! analytics, split into layered crates so a change rebuilds only what depends on it. This
//! crate re-exports them under one path and adds the C ABI and the `cym` CLI.
pub use cym_engine::{
    actions, adapters, analytics, brand, engine, git, model, modules, orphans, policy, ports,
    results, services, simulator, Engine,
};
pub use services::Services;
pub mod ffi;
