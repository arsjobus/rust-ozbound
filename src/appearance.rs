//! What characters look like, as plain data, and the dialogue portrait drawn from it.
//!
//! Pure Rust (no Bevy). An [`Appearance`] is four colours and a hat shape. The 3D models
//! (`models.rs`) and the 16x16 dialogue portraits are both built from the same value, so a
//! character always looks like their portrait. Level files choose an appearance by name
//! (`[npcs]` in `assets/maps/bluebrook.map`); [`Appearance::from_key`] is the only lookup,
//! and [`Appearance::KEYS`] lists every name the game knows.

/// An sRGB colour, 0-255 per channel.
pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appearance {
    pub skin: Rgb,
    pub tunic: Rgb,
    pub legs: Rgb,
    pub hat: Rgb,
    /// A wide, flat brim (travellers' hats) instead of a plain pointed cap.
    pub wide_brim: bool,
}

impl Appearance {
    /// Every appearance name a level file may use. `traveller` is the player's own look.
    pub const KEYS: [&'static str; 6] = [
        "traveller",
        "guide",
        "apothecary",
        "woodcutter",
        "ferrywoman",
        "surveyor",
    ];

    pub fn from_key(key: &str) -> Option<Self> {
        let skin_light = [237, 189, 148];
        let skin_tan = [214, 160, 118];
        let skin_deep = [150, 100, 72];
        let skin_rosy = [232, 170, 150];
        Some(match key {
            "traveller" => Self {
                skin: skin_light,
                tunic: [61, 128, 209],
                legs: [77, 61, 51],
                hat: [179, 77, 56],
                wide_brim: false,
            },
            "guide" => Self {
                skin: skin_light,
                tunic: [204, 158, 56],
                legs: [71, 77, 87],
                hat: [87, 66, 117],
                wide_brim: true,
            },
            "apothecary" => Self {
                skin: skin_rosy,
                tunic: [92, 150, 112],
                legs: [64, 60, 72],
                hat: [226, 220, 196],
                wide_brim: false,
            },
            "woodcutter" => Self {
                skin: skin_tan,
                tunic: [158, 58, 52],
                legs: [60, 52, 44],
                hat: [58, 70, 52],
                wide_brim: false,
            },
            "ferrywoman" => Self {
                skin: skin_deep,
                tunic: [70, 120, 160],
                legs: [50, 56, 70],
                hat: [214, 190, 120],
                wide_brim: true,
            },
            "surveyor" => Self {
                skin: skin_tan,
                tunic: [120, 96, 150],
                legs: [88, 78, 66],
                hat: [60, 56, 64],
                wide_brim: true,
            },
            _ => return None,
        })
    }

    /// The key this appearance is registered under, if it is one of the named ones.
    pub fn key(&self) -> Option<&'static str> {
        Self::KEYS
            .into_iter()
            .find(|key| Self::from_key(key).as_ref() == Some(self))
    }
}

/// Portraits are `PORTRAIT_SIZE` x `PORTRAIT_SIZE` pixels, four bytes (RGBA) each.
pub const PORTRAIT_SIZE: usize = 16;

const EYE: Rgb = [28, 24, 30];
const MOUTH: Rgb = [120, 52, 52];

fn shade(color: Rgb, percent: u32) -> Rgb {
    color.map(|c| ((c as u32 * percent) / 100).min(255) as u8)
}

/// A small pixel-art bust of `look`: hat, face, shoulders, on a backdrop tinted from the
/// tunic. Row 0 is the top. Every pixel is opaque, so the portrait never shows holes.
pub fn portrait_rgba(look: &Appearance) -> Vec<u8> {
    let backdrop = shade(look.tunic, 38);
    let shadow = shade(look.skin, 82);
    let mut pixels = vec![backdrop; PORTRAIT_SIZE * PORTRAIT_SIZE];
    let mut paint = |x: usize, y: usize, color: Rgb| {
        debug_assert!(x < PORTRAIT_SIZE && y < PORTRAIT_SIZE);
        pixels[y * PORTRAIT_SIZE + x] = color;
    };

    // Shoulders and tunic.
    for y in 11..PORTRAIT_SIZE {
        let left = if y == 11 { 4 } else { 2 };
        let right = if y == 11 { 11 } else { 13 };
        for x in left..=right {
            paint(x, y, look.tunic);
        }
    }
    // Neck.
    for x in 7..=8 {
        paint(x, 11, shadow);
    }
    // Face: a 6 x 6 block with a shaded jaw line.
    for y in 5..=10 {
        for x in 5..=10 {
            paint(x, y, if y == 10 { shadow } else { look.skin });
        }
    }
    // Eyes and mouth.
    paint(6, 7, EYE);
    paint(9, 7, EYE);
    paint(7, 9, MOUTH);
    paint(8, 9, MOUTH);
    // Hat: a crown over the forehead, plus a brim for wide hats.
    for y in 2..=4 {
        for x in 5..=10 {
            paint(x, y, look.hat);
        }
    }
    if look.wide_brim {
        for x in 2..=13 {
            paint(x, 4, shade(look.hat, 82));
        }
    } else {
        for x in 6..=9 {
            paint(x, 1, look.hat);
        }
        paint(7, 0, look.hat);
        paint(8, 0, look.hat);
    }

    pixels.into_iter().flat_map(|[r, g, b]| [r, g, b, 255]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
        let at = (y * PORTRAIT_SIZE + x) * 4;
        [data[at], data[at + 1], data[at + 2], data[at + 3]]
    }

    fn with_alpha(color: Rgb) -> [u8; 4] {
        [color[0], color[1], color[2], 255]
    }

    #[test]
    fn every_listed_key_is_known_and_round_trips() {
        for key in Appearance::KEYS {
            let look = Appearance::from_key(key).unwrap_or_else(|| panic!("{key} is listed but unknown"));
            assert_eq!(look.key(), Some(key));
        }
        assert_eq!(Appearance::from_key("wizard"), None);
        assert_eq!(Appearance::from_key("Guide"), None);
        assert_eq!(Appearance::from_key(""), None);
    }

    #[test]
    fn every_appearance_is_distinct() {
        for (i, a) in Appearance::KEYS.iter().enumerate() {
            for b in &Appearance::KEYS[i + 1..] {
                assert_ne!(Appearance::from_key(a), Appearance::from_key(b), "{a} and {b} look the same");
            }
        }
    }

    #[test]
    fn portraits_have_the_right_size_and_no_transparent_pixels() {
        for key in Appearance::KEYS {
            let data = portrait_rgba(&Appearance::from_key(key).unwrap());
            assert_eq!(data.len(), PORTRAIT_SIZE * PORTRAIT_SIZE * 4, "{key}");
            assert!(data.chunks(4).all(|p| p[3] == 255), "{key} has a transparent pixel");
        }
    }

    #[test]
    fn portraits_are_deterministic() {
        let look = Appearance::from_key("guide").unwrap();
        assert_eq!(portrait_rgba(&look), portrait_rgba(&look));
    }

    #[test]
    fn the_portrait_shows_the_characters_own_colours() {
        for key in Appearance::KEYS {
            let look = Appearance::from_key(key).unwrap();
            let data = portrait_rgba(&look);
            assert_eq!(pixel(&data, 5, 6), with_alpha(look.skin), "{key} face");
            assert_eq!(pixel(&data, 7, 3), with_alpha(look.hat), "{key} hat crown");
            assert_eq!(pixel(&data, 7, 13), with_alpha(look.tunic), "{key} tunic");
            assert_eq!(pixel(&data, 6, 7), with_alpha(EYE), "{key} left eye");
            assert_eq!(pixel(&data, 9, 7), with_alpha(EYE), "{key} right eye");
            assert_eq!(pixel(&data, 7, 9), with_alpha(MOUTH), "{key} mouth");
        }
    }

    #[test]
    fn wide_brims_and_pointed_caps_have_different_silhouettes() {
        let mut look = Appearance::from_key("guide").unwrap();
        look.wide_brim = true;
        let wide = portrait_rgba(&look);
        look.wide_brim = false;
        let pointed = portrait_rgba(&look);
        assert_ne!(wide, pointed);
        // A brim reaches past the face; a pointed cap does not, but it has a tip.
        assert_ne!(pixel(&wide, 2, 4), pixel(&pointed, 2, 4));
        assert_eq!(pixel(&pointed, 7, 0), with_alpha(look.hat));
        assert_ne!(pixel(&wide, 7, 0), with_alpha(look.hat));
    }

    #[test]
    fn different_appearances_give_different_portraits() {
        let all: Vec<Vec<u8>> = Appearance::KEYS
            .iter()
            .map(|key| portrait_rgba(&Appearance::from_key(key).unwrap()))
            .collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn shading_never_overflows() {
        assert_eq!(shade([255, 255, 255], 100), [255, 255, 255]);
        assert_eq!(shade([255, 128, 0], 50), [127, 64, 0]);
        assert_eq!(shade([200, 200, 200], 150), [255, 255, 255]);
    }
}
