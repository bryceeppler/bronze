use rune::color::srgb_decode;
use rune::{Color, Shadow};
use rune_kit::{Kit, PaletteFocus, PaletteSeed};

#[derive(Clone, Copy)]
pub struct Look {
    pub bg: Color,
    pub card: Color,
    pub raised: Color,
    pub card_border: Color,
    pub field: Color,
    pub field_border: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub label: Color,
    pub hairline: Color,
    pub ring: Color,
    pub accent: Color,
    pub danger: Color,
    pub on_accent: Color,
    pub selected: Color,
    pub code: Color,
    pub shadow: Color,
    pub lift: Color,
    pub scrim: Color,
    pub hair: f32,
}

impl Look {
    pub fn new(dark: bool, scale: f32) -> Look {
        let hair = 1.0 / scale.max(1.0);
        if dark {
            Look {
                bg: srgb(0x1c1c1e),
                card: srgb(0x2a2a2d),
                raised: srgb(0x353538),
                card_border: srgb(0xffffff).alpha(0.07),
                field: srgb(0xffffff).alpha(0.06),
                field_border: srgb(0xffffff).alpha(0.08),
                text: srgb(0xececee),
                muted: srgb(0xa1a1a6),
                faint: srgb(0x808086),
                label: srgb(0x8e8e93),
                hairline: srgb(0xffffff).alpha(0.1),
                ring: srgb(0x7a7a80),
                accent: srgb(0xcd7f32),
                danger: srgb(0xec8a74),
                on_accent: srgb(0xffffff),
                selected: srgb(0x44382e),
                code: srgb(0xffffff).alpha(0.09),
                shadow: srgb(0x000000).alpha(0.3),
                lift: srgb(0x000000).alpha(0.55),
                scrim: srgb(0x000000).alpha(0.32),
                hair,
            }
        } else {
            Look {
                bg: srgb(0xededf0),
                card: srgb(0xffffff),
                raised: srgb(0xffffff),
                card_border: srgb(0x000000).alpha(0.06),
                field: srgb(0xf8f8fa),
                field_border: srgb(0x000000).alpha(0.07),
                text: srgb(0x1c1c1e),
                muted: srgb(0x6b6b70),
                faint: srgb(0x8e8e93),
                label: srgb(0x68686d),
                hairline: srgb(0x000000).alpha(0.09),
                ring: srgb(0x8e8e93),
                accent: srgb(0xa3612a),
                danger: srgb(0xb23c2c),
                on_accent: srgb(0xffffff),
                selected: srgb(0xf6efea),
                code: srgb(0x000000).alpha(0.06),
                shadow: srgb(0x000000).alpha(0.05),
                lift: srgb(0x000000).alpha(0.16),
                scrim: srgb(0x000000).alpha(0.1),
                hair,
            }
        }
    }

    pub fn kit(&self) -> Kit {
        let mut kit = Kit::from_palette_focus(
            PaletteSeed { bg: self.card, fg: self.text, accent: self.accent },
            PaletteFocus::Accent,
        );
        kit.c.layer_01 = self.raised;
        kit.c.surface_raised = self.raised;
        kit
    }

    pub fn popover_shadow(&self) -> Shadow {
        Shadow { y: 8.0, blur: 28.0, color: self.lift, ..Shadow::default() }
    }
}

fn srgb(hex: u32) -> Color {
    let channel = |shift: u32| srgb_decode(((hex >> shift) & 0xff) as f32 / 255.0);
    Color { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contrast(a: Color, b: Color) -> f32 {
        let (a, b) = (a.luminance(), b.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_and_the_accent_hold_contrast_in_both_appearances() {
        for dark in [false, true] {
            let look = Look::new(dark, 2.0);
            assert!(contrast(look.text, look.card) >= 7.0, "body text, dark={dark}");
            assert!(contrast(look.muted, look.card) >= 4.5, "muted text, dark={dark}");
            assert!(contrast(look.label, look.bg) >= 4.5, "section labels, dark={dark}");
            assert!(contrast(look.accent, look.card) >= 4.5, "accent on a card, dark={dark}");
            assert!(contrast(look.danger, look.raised) >= 4.5, "a problem on a sheet, dark={dark}");
            assert!(contrast(look.on_accent, look.accent) >= 3.0, "check on the accent, dark={dark}");
            assert!(contrast(look.ring, look.card) >= 3.0, "checkbox ring, dark={dark}");
        }
    }
}
