//! The player's armies and the armies they can see. A move or a stop from a
//! remote client is the player's order (ADR remote-control-uses-capabilities):
//! the capability layer takes the army back from an extension moving it.
use crate::game::{armies as layer, units};
use eu4_harmony_remote::armies::args;
use eu4_harmony_remote::armies::{Army, ArmyAction, ArmyAnswer, ArmyRequest, Regiment};

layer! {
    run ArmyRequest => ArmyAnswer;
    Get => fn army -> Army = layer::get;
    List => fn armies -> Vec<Army> = layer::list;
    Regiments => fn regiments -> Vec<Regiment> = layer::regiments;
    At => fn armies_at -> Vec<Army> = layer::in_province;
    RouteTo => fn find_route -> Vec<u32> = layer::find_route;
    CanStep => fn can_step -> bool = layer::can_step;
    CanAttack => fn can_attack -> bool = layer::can_attack_seen;
    Move => fn move_to -> () = layer::move_to;
    Stop => fn stop -> () = layer::stop;
    Selected => fn selected -> u64 = layer::selected;
    Options => fn army_options -> Vec<ArmyAction> = units::options;
    Split => fn split -> u64 = units::split;
    Merge => fn merge -> () = units::merge;
    Disband => fn disband -> () = units::disband;
    Consolidate => fn consolidate -> () = units::consolidate;
    Drill => fn drill -> () = units::drill;
    StopDrill => fn stop_drill -> () = units::stop_drill;
    Mission => fn mission -> () = units::mission;
    StopMission => fn stop_mission -> () = units::stop_mission;
}
