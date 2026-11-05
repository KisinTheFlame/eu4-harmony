//! Mercenary companies: what the mercenary view lists, and hiring one.
use eu4_harmony_remote::{
    mercenaries::{Company, MercenaryAnswer, MercenaryRequest},
    Error,
};

layer! {
    fn companies() -> Vec<Company> = crate::game::mercenaries::list;
    fn hire(company: String, province: u32) -> () = crate::game::mercenaries::hire;
}

pub fn run(request: MercenaryRequest, game: &mut (impl Layer + ?Sized)) -> Result<MercenaryAnswer, Error> {
    Ok(match request {
        MercenaryRequest::List {} => MercenaryAnswer::List(game.companies()?),
        MercenaryRequest::Hire { company, province } => MercenaryAnswer::Hire(game.hire(company, province.0)?),
    })
}
