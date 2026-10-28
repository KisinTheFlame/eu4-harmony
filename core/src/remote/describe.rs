//! The capabilities, described: what a remote client can ask.
use eu4_harmony_remote::{
    capabilities,
    describe::{DescribeAnswer, DescribeRequest},
};

pub fn run(request: DescribeRequest) -> DescribeAnswer {
    match request {
        DescribeRequest::All {} => DescribeAnswer::All(capabilities()),
    }
}
