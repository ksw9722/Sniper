//! Nuclei templates: the community's detection templates, and the subset of
//! their HTTP language Sniper can run.
//!
//! Templates are data, not code. Sniper never runs the protocols that execute
//! on this machine or open sockets it cannot scope (code, javascript,
//! headless, network, dns and the rest), and a template that needs anything
//! outside the supported subset is listed as unsupported rather than run in
//! part.

pub mod dsl;
pub mod model;
