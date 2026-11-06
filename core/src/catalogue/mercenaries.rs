//! Mercenary companies: what the mercenary view lists, and hiring one.
use eu4_harmony_remote::mercenaries::{Company, MercenaryAnswer, MercenaryRequest};

layer! {
    run MercenaryRequest => MercenaryAnswer;
    List => fn companies() -> Vec<Company> = caller crate::game::mercenaries::list;
    Hire => fn hire(company: String, province: u32) -> () = caller crate::game::mercenaries::hire;
}
