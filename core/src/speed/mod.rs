//! Speed-up implementations (loading, saving, AI checks, modifier sums,
//! POPS polling). The `SpeedUp` descriptor and switch queries live in
//! `native::speed_up`, while the complete `ALL` registry is assembled at the
//! top level so `speed` and `render` remain independent siblings.

pub(crate) mod army_checks;
pub(crate) mod loading;
pub(crate) mod loading_times;
pub(crate) mod modifier_sums;
pub(crate) mod pool;
pub(crate) mod pops;
pub(crate) mod regiment_upkeep;
pub(crate) mod saving;
pub(crate) mod settlement_growth;
pub(crate) mod support_checks;
