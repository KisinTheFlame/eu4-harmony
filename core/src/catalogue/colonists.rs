//! Colonists: the province view's colonize buttons.
use eu4_harmony_remote::{
    colonists::{ColonistAnswer, ColonistOptions, ColonistRequest},
    Error,
};

layer! {
    fn colonist_options(province: u32) -> ColonistOptions = crate::game::colonists::options;
    fn send_colonist(province: u32) -> () = crate::game::colonists::send;
    fn recall_colonist(province: u32) -> () = crate::game::colonists::recall;
}

pub fn run(request: ColonistRequest, game: &mut (impl Layer + ?Sized)) -> Result<ColonistAnswer, Error> {
    Ok(match request {
        ColonistRequest::Options { province } => ColonistAnswer::Options(game.colonist_options(province.0)?),
        ColonistRequest::Send { province } => ColonistAnswer::Send(game.send_colonist(province.0)?),
        ColonistRequest::Recall { province } => ColonistAnswer::Recall(game.recall_colonist(province.0)?),
    })
}
