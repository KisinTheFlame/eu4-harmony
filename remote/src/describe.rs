//! The capabilities, described.
use crate::Capability;
use serde::Serialize;

group! {
    /// The capabilities, described.
    "describe": DescribeRequest, DescribeAnswer;

    /// The version of the capability catalogue the game serves, and every
    /// capability remote control has, with its arguments.
    remote_anywhere All "" {} -> Description, "api_version, and the capabilities: name, what it does, whether it needs a game going on, acts as a country, extensions have it and a request asks for it only as the game, arguments, result";
}

/// The catalogue, as remote control has it.
#[derive(Clone, Debug, PartialEq, Serialize, crate::shape::Shaped)]
pub struct Description {
    /// The catalogue's version (ADR one-capability-catalogue): it changes when
    /// a capability that exists changes what it means or how it is shaped.
    pub api_version: &'static str,
    pub capabilities: Vec<Capability>,
}
