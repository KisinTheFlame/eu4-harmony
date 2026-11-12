//! Owned entry images. Validation runs before an extension is accepted; the
//! core serves these bytes and their native sprite definitions as framework
//! files, so a mod never has to arrange native resource registration.
use crate::{layout::Window, sandbox};
use std::{path::Path, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub sprite: String,
    pub path: String,
    pub data: Arc<[u8]>,
}

impl Image {
    pub fn definition(&self) -> String {
        format!(
            "spriteType = {{ name = \"{}\" texturefile = \"{}\" effectFile = \"gfx/FX/buttonstate.lua\" noOfFrames = 1 clicksound = click }}\n",
            self.sprite, self.path
        )
    }
}

/// Entry images match the game's native panel-button frame, 41 by 40. Keep
/// the supported formats bounded rather than accepting a header whose pixel
/// payload the native decoder will reject later.
fn validate(data: &[u8], extension: &str) -> Result<(), String> {
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    match extension {
        "dds" => {
            if data.get(..4) != Some(b"DDS ") || word(4) != Some(124) || word(76) != Some(32) {
                return Err("entry image has an invalid DDS header".into());
            }
            if word(16) != Some(41) || word(12) != Some(40) || word(24).unwrap_or(0) > 1 || word(112) != Some(0) {
                return Err("entry image must be a single 41 by 40 pixel texture".into());
            }
            let flags = word(80).unwrap_or(0);
            let block = if flags & 4 != 0 {
                match data.get(84..88) {
                    Some(b"DXT1") => 8,
                    Some(b"DXT3" | b"DXT5") => 16,
                    _ => return Err("entry DDS uses DXT1, DXT3, DXT5 or 32-bit BGRA".into()),
                }
            } else if flags & 0x40 != 0
                && word(88) == Some(32)
                && word(92) == Some(0x00ff0000)
                && word(96) == Some(0x0000ff00)
                && word(100) == Some(0x000000ff)
                && word(104) == Some(0xff000000)
            {
                0
            } else {
                return Err("entry DDS uses DXT1, DXT3, DXT5 or 32-bit BGRA".into());
            };
            let levels = word(28).unwrap_or(1).max(1);
            if levels > 6 {
                return Err("entry DDS has too many mip levels".into());
            }
            let (mut width, mut height, mut required) = (41usize, 40usize, 128usize);
            for _ in 0..levels {
                required += if block == 0 { width * height * 4 } else { width.div_ceil(4) * height.div_ceil(4) * block };
                width = (width / 2).max(1);
                height = (height / 2).max(1);
            }
            if data.len() < required {
                return Err("entry DDS pixel data is truncated".into());
            }
        }
        "tga" => {
            if data.len() < 18 || data[1] != 0 || data[2] != 2 || ![24, 32].contains(&data[16]) {
                return Err("entry TGA is an uncompressed 24- or 32-bit true-color image".into());
            }
            let size = |at| u16::from_le_bytes([data[at], data[at + 1]]);
            if size(12) != 41 || size(14) != 40 {
                return Err("entry image must be 41 by 40 pixels".into());
            }
            if data.len() < 18 + usize::from(data[0]) + 41 * 40 * usize::from(data[16] / 8) {
                return Err("entry TGA pixel data is truncated".into());
            }
        }
        _ => return Err("entry image is a .dds or .tga file".into()),
    }
    Ok(())
}

pub fn load(root: &Path, id: &str, windows: &mut [Window]) -> Result<Vec<Image>, String> {
    let mut images = Vec::new();
    for window in windows {
        let Some(entry) = window.entry.as_mut() else { continue };
        let Some(file) = &entry.image else { continue };
        let path = sandbox::within(root, file)?;
        let canonical_root = root.canonicalize().map_err(|e| format!("entry image root: {e}"))?;
        let canonical = path.canonicalize().map_err(|e| format!("entry image {file:?}: {e}"))?;
        if !canonical.starts_with(&canonical_root) {
            return Err(format!("entry image {file:?} is outside its mod"));
        }
        let metadata = std::fs::metadata(&canonical).map_err(|e| format!("entry image {file:?}: {e}"))?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err(format!("entry image {file:?} is a regular file of at most 1 MiB"));
        }
        let data = std::fs::read(&canonical).map_err(|e| format!("entry image {file:?}: {e}"))?;
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        validate(&data, &extension).map_err(|e| format!("entry image {file:?}: {e}"))?;
        let sprite = format!("GFX_eu4harmony_{id}_{}_entry", window.name);
        let path = format!("gfx/interface/eu4harmony/{id}/{}_entry.{extension}", window.name);
        entry.sprite.clone_from(&sprite);
        images.push(Image { sprite, path, data: data.into() });
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds() -> Vec<u8> {
        let mut data = vec![0; 128 + 11 * 10 * 16];
        data[..4].copy_from_slice(b"DDS ");
        for (at, value) in [(4, 124u32), (12, 40), (16, 41), (76, 32), (80, 4)] {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[84..88].copy_from_slice(b"DXT5");
        data
    }

    #[test]
    fn bad_format_dimensions_and_truncated_pixels_fail_before_native_loading() {
        let mut data = dds();
        assert!(validate(&data, "dds").is_ok());
        assert!(validate(&data[..128], "dds").unwrap_err().contains("truncated"));
        data[16..20].copy_from_slice(&410u32.to_le_bytes());
        assert!(validate(&data, "dds").unwrap_err().contains("41 by 40"));
        data[16..20].copy_from_slice(&41u32.to_le_bytes());
        data[84..88].copy_from_slice(b"DX10");
        assert!(validate(&data, "dds").is_err());
        assert!(validate(&[], "tga").is_err());
        assert!(validate(&dds(), "png").is_err());
    }

    #[test]
    fn an_image_cannot_escape_the_mod_or_silently_use_a_missing_resource() {
        let root = std::env::temp_dir().join(format!("eu4h-entry-image-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("entry.dds"), dds()).unwrap();
        let read = |file: &str| crate::layout::read(&serde_json::json!({"tools":{"size":[200,200],"entry":{"image":file}}}), &|_| true).unwrap();
        let mut windows = read("entry.dds");
        let images = load(&root, "tools", &mut windows).unwrap();
        assert_eq!(windows[0].entry.as_ref().unwrap().sprite, images[0].sprite);
        assert!(images[0].definition().contains("gfx/FX/buttonstate.lua"));
        assert!(load(&root, "tools", &mut read("missing.dds")).is_err());
        assert!(load(&root, "tools", &mut read("../entry.dds")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
