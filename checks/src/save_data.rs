//! Extension save data, which every save carries whole (ADR 0020).
use crate::{Checked, Refusal, Status};

/// How much JSON one extension's data may be.
pub const MAX_JSON: usize = 16 * 1024 * 1024;

/// Refuses data of more than [`MAX_JSON`] bytes as JSON.
pub fn size(json_len: usize) -> Checked {
    if json_len > MAX_JSON {
        return Err(Refusal::new(Status::InvalidArgument, format!("save data is {json_len} bytes of JSON; at most {MAX_JSON} are kept")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_up_to_16_mib_is_kept() {
        assert!(size(0).is_ok());
        assert!(size(MAX_JSON).is_ok());
        let refusal = size(MAX_JSON + 1).unwrap_err();
        assert_eq!((refusal.status, refusal.message.as_str()), (Status::InvalidArgument, "save data is 16777217 bytes of JSON; at most 16777216 are kept"));
    }
}
