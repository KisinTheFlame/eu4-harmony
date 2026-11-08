//! What the game unloads while it is shown, which vanilla never meets, as it
//! makes the main menu only at startup: a sprite type shown sprites still
//! count, which the main menu, made again, would unload with the game's
//! graphics, stays loaded.
use crate::called_by_game;
use crate::{
    objects::gui::{classes as gui_classes, sprite_type},
    points::{Function, Point},
};

/// `(sprite type)`: unloads a sprite type with its group of graphics
/// (`0x14dc240`), letting its textures go; its [`sprite_type::UNLOAD`].
const SPRITE_TYPE_UNLOAD: usize = 0x156d730;

type SpriteTypeUnload = unsafe extern "C" fn(usize);
static ORIGINAL_SPRITE_TYPE_UNLOAD: Function<SpriteTypeUnload> = Function::new();

/// A sprite type is unloaded with its group of graphics, as the main menu,
/// being made, unloads the game's, whatever its shown sprites. A sprite type
/// loaded lazily that shown sprites still show stays loaded: so it does until
/// the last of them hides, which unloads it. Vanilla makes the main menu
/// once, at startup, when no sprite of the game shows; after an in-process
/// return, sprites the session left still count, and a sprite of the type
/// shown later, finding the count above 0, does not load the texture, and is
/// drawn from one let go (docs/research/main-menu.md).
#[called_by_game]
unsafe extern "C" fn sprite_type_unloading(kind: usize) {
    let stays = crate::guarded("sprite type unload", || {
        let m = crate::hooked_memory();
        let kept = (|| -> crate::Result<u32> {
            let shown = if m.u8(kind + sprite_type::LAZY)? != 0 { m.u32(kind + sprite_type::SHOWN)? } else { 0 };
            if shown != 0 {
                m.write(kind + sprite_type::LOADED, &[1])?;
            }
            Ok(shown)
        })();
        match kept {
            Ok(0) => false,
            Ok(shown) => {
                let name = crate::game_string::key(&m, kind + sprite_type::NAME).unwrap_or_default();
                log!("repair: the sprite type {name} stays loaded with its group: {shown} shown sprite(s) show it");
                true
            }
            Err(e) => {
                log!("repair: a sprite type was unloaded with its group unread: {e}");
                false
            }
        }
    });
    if !stays {
        unsafe { ORIGINAL_SPRITE_TYPE_UNLOAD.get()(kind) }
    }
}

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, ..super::Repair::NONE };

pub(super) fn points() -> Vec<Point> {
    vec![
        // A sprite type unloaded with its group of graphics, which the main
        // menu's interface rebuild does for the game's (group 3); a sprite
        // shown loads its lazily loaded type only when none showed it.
        Point::replace(
            SPRITE_TYPE_UNLOAD,
            "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57 48 83 ec 40 4c 8b f9",
            "sprite type unload",
            sprite_type_unloading as SpriteTypeUnload,
            &ORIGINAL_SPRITE_TYPE_UNLOAD,
        ),
        Point::method(gui_classes::SPRITE_TYPE, sprite_type::UNLOAD, SPRITE_TYPE_UNLOAD, "sprite type unload method"),
        Point::keep(0x14dc2a5usize, "80 79 44 00 74 0d 48 8b 01 48 8b d7 c6 41 44 00 ff 50 48", "graphics group unload")
            .uses(&[sprite_type::LOADED, sprite_type::UNLOAD]),
        Point::keep(0x10d5ca9usize, "ba 03 00 00 00 48 8b c8 e8 8a 65 40 00", "main menu unloads the game's graphics"),
        Point::keep(0x15bd47fusize, "8b 87 88 02 00 00 85 c0 75 29 80 bf 8c 02 00 00 01 75 20 83 bf 58 02 00 00 ff", "sprite shown loads its type").uses(&[
            sprite_type::SHOWN,
            sprite_type::LAZY,
            sprite_type::TEXTURE,
        ]),
    ]
}
