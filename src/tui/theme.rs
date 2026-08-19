//! Palette. Brass, not gold. Oxidation, not damage. An instrument, not a costume.
//!
//! Truecolour values throughout. Backgrounds are deliberately not set: the terminal's
//! own ground is warmer than anything the 256 cube can offer, and a neutral grey panel
//! behind brass reads as dirt.

use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub seam: Color,
    pub ink: Color,
    pub ink_dim: Color,
    pub ink_faint: Color,
    pub copper: Color,
    pub brass: Color,
    pub brass_hi: Color,
    pub verdigris: Color,
    pub ember: Color,
    pub rust: Color,
    /// The one opaque ground, for a panel drawn over other content. Nothing does yet.
    #[allow(dead_code)]
    pub void: Color,
}

pub const T: Theme = Theme {
    seam:      Color::Rgb( 77,  66,  49), // #4d4231  frames
    ink:       Color::Rgb(232, 223, 205), // #e8dfcd  body
    ink_dim:   Color::Rgb(162, 147, 122), // #a2937a  secondary
    ink_faint: Color::Rgb(157, 143, 117), // #9d8f75  labels
    copper:    Color::Rgb(205, 124,  72), // #cd7c48  DATA
    brass:     Color::Rgb(201, 162,  39), // #c9a227  STRUCTURE
    brass_hi:  Color::Rgb(240, 216, 145), // #f0d891  ATTENTION, sparingly
    verdigris: Color::Rgb( 95, 158, 125), // #5f9e7d  healthy
    ember:     Color::Rgb(232, 146,  42), // #e8922a  working
    rust:      Color::Rgb(232, 105,  81), // #e86951  failed
    void:      Color::Rgb( 20,  17,  13), // #14110d  the one opaque ground
};

impl Theme {
    // The full set is kept whether or not every one is currently drawn: the palette is
    // the contract, and reaching for a literal colour later is how a theme unravels.
    #[allow(dead_code)]
    pub fn body(&self) -> Style { Style::new().fg(self.ink) }
    pub fn label(&self) -> Style { Style::new().fg(self.ink_faint).add_modifier(Modifier::BOLD) }
    pub fn dim(&self) -> Style { Style::new().fg(self.ink_dim).add_modifier(Modifier::DIM) }
    pub fn frame(&self) -> Style { Style::new().fg(self.seam) }
    #[allow(dead_code)]
    pub fn data(&self) -> Style { Style::new().fg(self.copper) }
}

/// Each voice on the line gets a stable colour, drawn from the signal metals and the
/// oxides. Not decoration: at a glance the colour is how you tell who is speaking
/// before you have read the name.
pub fn voice(name: &str) -> Color {
    match name {
        "Jenny" => T.brass_hi,
        "Mark" => T.copper,
        "Bernard" => T.ember,
        "Gemma" => T.verdigris,
        "Chief" => T.brass,
        _ => T.ink_dim,
    }
}
