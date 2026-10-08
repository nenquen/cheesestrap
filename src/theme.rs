use egui::Color32;
use ratatui::style::Color;

// Cheesy palette. Warm yellows on dark chocolate.
pub const BG: Color = Color::Rgb(29, 19, 10);
pub const PANEL: Color = Color::Rgb(42, 28, 14);
pub const CHEESE: Color = Color::Rgb(255, 201, 60);
pub const MELT: Color = Color::Rgb(255, 159, 28);
pub const CREAM: Color = Color::Rgb(255, 243, 214);
pub const CRUST: Color = Color::Rgb(138, 90, 43);
pub const DIM: Color = Color::Rgb(160, 130, 90);
pub const GOOD: Color = Color::Rgb(163, 214, 92);
pub const BAD: Color = Color::Rgb(255, 107, 94);
pub const SELECT: Color = Color::Rgb(172, 120, 20);

/// Every colour above is `Rgb`, so anything else can only be a default cell
/// ratatui handed back. Cream is the closest match to an untouched cell.
pub fn to_egui(c: Color) -> Color32 {
    match c {
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        _ => Color32::from_rgb(255, 243, 214),
    }
}
