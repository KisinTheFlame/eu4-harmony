//! The capabilities, described.
use crate::Capability;

group! {
    /// The capabilities, described.
    "describe": DescribeRequest, DescribeAnswer;

    /// Every capability remote control has, with its arguments.
    anywhere All "" {} -> Vec<Capability>, "the capabilities: name, what it does, whether it needs a game going on, arguments, result";
}
