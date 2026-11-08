//! Mercenary companies: what the mercenary view lists, and hiring one.
use eu4_harmony_remote::mercenaries::{Company, MercenaryAnswer, MercenaryRequest};
use eu4_harmony_remote::mercenaries::args;

layer! {
    run MercenaryRequest => MercenaryAnswer;
    List => fn companies -> Vec<Company> = crate::game::mercenaries::list;
    Hire => fn hire -> () = crate::game::mercenaries::hire;
}
