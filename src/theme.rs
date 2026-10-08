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

pub fn to_egui(c: Color) -> Color32 {
    match c {
        Color::Reset => Color32::from_rgb(255, 243, 214),
        Color::Black => Color32::BLACK,
        Color::Red => Color32::RED,
        Color::Green => Color32::GREEN,
        Color::Yellow => Color32::YELLOW,
        Color::Blue => Color32::BLUE,
        Color::Magenta => Color32::from_rgb(255, 0, 255),
        Color::Cyan => Color32::from_rgb(0, 255, 255),
        Color::Gray => Color32::GRAY,
        Color::DarkGray => Color32::DARK_GRAY,
        Color::LightRed => Color32::LIGHT_RED,
        Color::LightGreen => Color32::LIGHT_GREEN,
        Color::LightYellow => Color32::LIGHT_YELLOW,
        Color::LightBlue => Color32::LIGHT_BLUE,
        Color::LightMagenta => Color32::from_rgb(255, 128, 255),
        Color::LightCyan => Color32::from_rgb(128, 255, 255),
        Color::White => Color32::WHITE,
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        _ => Color32::from_rgb(255, 243, 214),
    }
}
