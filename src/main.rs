#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod roblox;
mod net;
mod theme;
mod ui;
mod update;

use std::sync::mpsc;

use crossterm::event::KeyCode;
use ratatui::{backend::TestBackend, Terminal};

use app::{App, Focus};
use ui::{HotAction, HotZone};

const COLS: u16 = 100;
const ROWS: u16 = 30;
const TITLE_H: f32 = 32.0;

struct CheeseApp {
    app: App,
    term: Terminal<TestBackend>,
    hot: Vec<HotZone>,
    logo: Option<egui::TextureHandle>,
}

impl CheeseApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new(tx, rx);
        app.protocol_url = roblox::protocol_url_from_args();
        app.refresh_versions();
        app.check_update();
        let backend = TestBackend::new(COLS, ROWS);
        let term = Terminal::new(backend).expect("test backend");
        Self {
            app,
            term,
            hot: Vec::new(),
            logo: None,
        }
    }

    fn activate_action(&mut self) {
        match self.app.action_idx {
            1 => self.app.play(),
            _ => match roblox::open_folder() {
                Ok(()) => self.app.push_log("opened roblox folder.".to_string()),
                Err(e) => self.app.push_log(format!("folder failed: {e}")),
            },
        }
    }

    fn handle_key(&mut self, code: KeyCode) {
        // an update is not optional, so the screen swallows everything and
        // only accepts the one button
        if !self.app.updating && self.app.update.is_some() {
            if matches!(code, KeyCode::Enter | KeyCode::Char(' ')) {
                self.app.using_keyboard = true;
                self.app.apply_update();
            }
            return;
        }
        if self.app.editing_args {
            match code {
                KeyCode::Esc | KeyCode::Enter => self.app.editing_args = false,
                KeyCode::Backspace => {
                    self.app.extra_args.pop();
                }
                KeyCode::Char(c) => self.app.extra_args.push(c),
                _ => {}
            }
            return;
        }
        if matches!(code, KeyCode::Tab) {
            self.app.focus = match self.app.focus {
                Focus::Menu => Focus::Content,
                Focus::Content => Focus::Menu,
            };
            return;
        }
        if matches!(code, KeyCode::Esc) {
            self.app.focus = Focus::Menu;
            return;
        }
        let mut activate = false;
        {
            let app = &mut self.app;
            match app.focus {
                Focus::Menu => match code {
                    KeyCode::Char('q') => {
                        app.ctx_close_requested = true;
                    }
                    KeyCode::Char('s') => {
                        app.menu_idx = 1;
                        app.focus = Focus::Content;
                    }
                    KeyCode::Up => app.menu_idx = app.menu_idx.saturating_sub(1),
                    KeyCode::Down => app.menu_idx = (app.menu_idx + 1).min(2),
                    KeyCode::Enter => match app.menu_idx {
                        0 | 1 => app.focus = Focus::Content,
                        _ => app.ctx_close_requested = true,
                    },
                    _ => {}
                },
                Focus::Content => {
                    if app.show_settings() {
                        match code {
                            KeyCode::Up => {
                                app.settings_idx = app.settings_idx.saturating_sub(1)
                            }
                            KeyCode::Down => app.settings_idx = (app.settings_idx + 1).min(3),
                            KeyCode::Enter | KeyCode::Char(' ') => match app.settings_idx {
                                0 => {
                                    app.logs_to_file = !app.logs_to_file;
                                    if app.logs_to_file {
                                        app.push_log(
                                            "file logging on. check the logs folder next to the app.".to_string(),
                                        );
                                    }
                                    app.save();
                                }
                                1 => {
                                    app.show_hints = !app.show_hints;
                                    app.save();
                                }
                                2 if !app.webview_ok => app.repair_webview2(),
                                3 if app.can_uninstall => app.uninstall_roblox(),
                                _ => {}
                            },
                            _ => {}
                        }
                    } else {
                        match code {
                            KeyCode::Up => {
                                app.action_idx = app.action_idx.saturating_sub(1)
                            }
                            KeyCode::Down => app.action_idx = (app.action_idx + 1).min(1),
                            KeyCode::Enter => activate = true,
                            _ => {}
                        }
                    }
                }
            }
        }
        if activate {
            self.activate_action();
        }
    }

    fn click(&mut self, col: u16, row: u16) {
        let zone = self.hot.iter().find(|z| {
            row == z.y && col >= z.x && col < z.x + z.w
        });
        match zone.map(|z| z.action) {
            Some(HotAction::Menu(i)) => {
                let i = i.min(2);
                self.app.menu_idx = i;
                self.app.focus = Focus::Menu;
                self.handle_key(KeyCode::Enter);
            }
            Some(HotAction::Action(i)) => {
                self.app.action_idx = i.min(1);
                self.app.focus = Focus::Content;
                self.activate_action();
            }
            Some(HotAction::Setting(i)) => {
                let i = i.min(3);
                self.app.settings_idx = i;
                self.app.focus = Focus::Content;
                if i == 2 {
                    if !self.app.webview_ok {
                        self.app.repair_webview2();
                    }
                } else if i == 3 {
                    if self.app.can_uninstall {
                        self.app.uninstall_roblox();
                    }
                } else {
                    self.handle_key(KeyCode::Enter);
                }
            }
            Some(HotAction::Notice(_)) => {
                self.app.apply_update();
            }
            None => {}
        }
    }
}

fn rat_to_egui(c: ratatui::style::Color) -> egui::Color32 {
    theme::to_egui(c)
}

/// Relaunches the app elevated if it is not already.
/// Returns immediately when already admin or on any failure.
#[cfg(windows)]
fn ensure_admin() {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == FALSE {
            return;
        }
        let mut elev: TOKEN_ELEVATION = std::mem::zeroed();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        CloseHandle(token);
        if ok == FALSE || elev.TokenIsElevated == 0 {
            if let Ok(exe) = std::env::current_exe() {
                let exe_w: Vec<u16> =
                    exe.to_string_lossy().encode_utf16().chain([0]).collect();
                let op: Vec<u16> = "runas\0".encode_utf16().collect();
                let r = ShellExecuteW(
                    std::ptr::null_mut(),
                    op.as_ptr(),
                    exe_w.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                );
                if r as usize > 32 {
                    std::process::exit(0);
                }
            }
        }
    }
}

impl eframe::App for CheeseApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut codes: Vec<KeyCode> = Vec::new();
        let mut clicked: Option<egui::Pos2> = None;
        ui.ctx().input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => {
                        self.app.using_keyboard = true;
                        match key {
                            egui::Key::ArrowUp => codes.push(KeyCode::Up),
                            egui::Key::ArrowDown => codes.push(KeyCode::Down),
                            egui::Key::ArrowLeft => codes.push(KeyCode::Left),
                            egui::Key::ArrowRight => codes.push(KeyCode::Right),
                            egui::Key::Enter => codes.push(KeyCode::Enter),
                            egui::Key::Escape => codes.push(KeyCode::Esc),
                            egui::Key::Backspace => codes.push(KeyCode::Backspace),
                            egui::Key::Tab => codes.push(KeyCode::Tab),
                            _ => {}
                        }
                    }
                    egui::Event::Text(t) => {
                        self.app.using_keyboard = true;
                        for ch in t.chars() {
                            codes.push(KeyCode::Char(ch));
                        }
                    }
                    egui::Event::PointerButton {
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => {
                        self.app.using_keyboard = false;
                        clicked = i.pointer.latest_pos();
                    }
                    egui::Event::MouseWheel { unit, delta, .. } => {
                        use egui::MouseWheelUnit as U;
                        let lines = match unit {
                            U::Line => delta.y * 3.0,
                            U::Page => delta.y * 10.0,
                            _ => delta.y / 40.0,
                        };
                        self.app.scroll_log(lines.round() as i32);
                    }
                    _ => {}
                }
            }
        });
        for c in codes {
            self.handle_key(c);
        }
        if self.app.ctx_close_requested {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.app.drain();
        if !self.app.busy && self.app.latest.is_some() {
            if let Some(url) = self.app.protocol_url.take() {
                self.app.push_log("website launch detected.".to_string());
                self.app.play_with_url(url);
            }
        }
        if let Some(t) = self.app.close_at {
            if std::time::Instant::now() >= t {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.term
            .draw(|f| ui::draw(f, &mut self.app, &mut self.hot))
            .ok();
        let buf = self.term.backend().buffer().clone();
        let w = buf.area.width as usize;

        // Fixed font. The window is resized to fit the grid exactly,
        // so nothing ever clips and there are no gaps.
        let font_id = egui::FontId::monospace(13.0);
        // Authoritative metrics straight from the font: advance for
        // width, full row height (with leading) for height.
        let (cell_w, cell_h) = ui.fonts_mut(|f| {
            let g = f.layout_no_wrap(
                "MMMMMMMMMM".to_string(),
                font_id.clone(),
                egui::Color32::WHITE,
            );
            (g.size().x / 10.0, f.row_height(&font_id))
        });

        // Keep the window locked to the grid size so no black
        // strips ever appear around the content.
        let avail = ui.max_rect();
        let want = egui::vec2(
            (COLS as f32 * cell_w).ceil() + 1.0,
            TITLE_H + (ROWS as f32 * cell_h).ceil() + 1.0,
        );
        let have = avail.size();
        if (want.x - have.x).abs() > 2.0 || (want.y - have.y).abs() > 2.0 {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(want));
        }

        let painter = ui.painter().clone();
        // Panel-colored backdrop so gaps never look like dark bands.
        let bg = egui::Color32::from_rgb(42, 28, 14);
        painter.rect_filled(avail, egui::CornerRadius::ZERO, bg);

        // Custom title bar (must be after the backdrop fill).
        let bar = egui::Rect::from_min_size(
            avail.min,
            egui::vec2(avail.width(), TITLE_H),
        );
        painter.rect_filled(
            bar,
            egui::CornerRadius::ZERO,
            egui::Color32::from_rgb(42, 28, 14),
        );
        let drag = ui.allocate_response(
            egui::vec2((avail.width() - 100.0).max(50.0), TITLE_H),
            egui::Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        // Animated title: darkened yellow base with a shine sweep.
        let title_font = egui::FontId::monospace(14.0);
        let adv = ui.fonts_mut(|f| {
            f.layout_no_wrap(
                "M".to_string(),
                title_font.clone(),
                egui::Color32::WHITE,
            )
            .size()
            .x
        });
        let title = "cheesestrap";
        let t = ui.input(|i| i.time) as f32;
        let base = egui::Color32::from_rgb(176, 134, 20);
        let shine = egui::Color32::from_rgb(255, 216, 100);
        let total = title.chars().count() as f32 * adv;
        let sweep = (t * 130.0) % (total + 90.0) - 45.0;
        let ty = avail.min.y + (TITLE_H - 17.0) / 2.0;
        for (i, ch) in title.chars().enumerate() {
            let cx = avail.min.x + 32.0 + i as f32 * adv;
            let d = ((cx - (avail.min.x + 32.0 + sweep)) / 36.0).abs();
            let k = (1.0 - d).clamp(0.0, 1.0);
            let k = k * k;
            let col = egui::Color32::from_rgb(
                (base.r() as f32 + (shine.r() as f32 - base.r() as f32) * k) as u8,
                (base.g() as f32 + (shine.g() as f32 - base.g() as f32) * k) as u8,
                (base.b() as f32 + (shine.b() as f32 - base.b() as f32) * k) as u8,
            );
            painter.text(
                egui::pos2(cx, ty),
                egui::Align2::LEFT_TOP,
                ch.to_string(),
                title_font.clone(),
                col,
            );
        }
        if self.logo.is_none() {
            if let Ok(img) = image::load_from_memory(include_bytes!(
                "../assets/branding/macncheese-256.png"
            )) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &rgba.into_raw(),
                );
                self.logo = Some(ui.ctx().load_texture(
                    "logo",
                    ci,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        if let Some(tex) = &self.logo {
            painter.image(
                tex.id(),
                egui::Rect::from_min_size(
                    egui::pos2(avail.min.x + 7.0, avail.min.y + (TITLE_H - 18.0) / 2.0),
                    egui::vec2(18.0, 18.0),
                ),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        let mut bx = avail.max.x;
        for (kind, cmd) in [
            (0u8, egui::ViewportCommand::Close),
            (1u8, egui::ViewportCommand::Minimized(true)),
        ] {
            bx -= 46.0;
            let r = egui::Rect::from_min_size(
                egui::pos2(bx, avail.min.y),
                egui::vec2(46.0, TITLE_H),
            );
            let resp = ui.allocate_rect(r, egui::Sense::click());
            let hovered = resp.hovered();
            if kind == 0 && hovered {
                painter.rect_filled(
                    r,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgb(232, 17, 35),
                );
            } else if hovered {
                painter.rect_filled(
                    r,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                );
            }
            let c = r.center();
            let ink = egui::Color32::from_rgb(255, 243, 214);
            if kind == 0 {
                let s = 5.0;
                let st = egui::Stroke::new(1.5, ink);
                painter.line_segment(
                    [
                        egui::pos2(c.x - s, c.y - s),
                        egui::pos2(c.x + s, c.y + s),
                    ],
                    st,
                );
                painter.line_segment(
                    [
                        egui::pos2(c.x - s, c.y + s),
                        egui::pos2(c.x + s, c.y - s),
                    ],
                    st,
                );
            } else {
                painter.line_segment(
                    [
                        egui::pos2(c.x - 5.0, c.y + 3.0),
                        egui::pos2(c.x + 5.0, c.y + 3.0),
                    ],
                    egui::Stroke::new(1.5, ink),
                );
            }
            if resp.clicked() {
                ui.ctx().send_viewport_cmd(cmd);
            }
        }

        // Grid content below the title bar.
        let origin = egui::pos2(avail.min.x.round(), (avail.min.y + TITLE_H).round());
        for (idx, cell) in buf.content.iter().enumerate() {
            let x = (idx % w) as f32;
            let y = (idx / w) as f32;
            let min = egui::pos2(
                (origin.x + x * cell_w).round(),
                (origin.y + y * cell_h).round(),
            );
            let rect = egui::Rect::from_min_size(min, egui::vec2(cell_w + 1.0, cell_h + 1.0));
            painter.rect_filled(rect, egui::CornerRadius::ZERO, rat_to_egui(cell.bg));
        }
        let h = buf.area.height as usize;
        for row in 0..h {
            let mut job = egui::text::LayoutJob::default();
            for col in 0..w {
                let cell = &buf.content[row * w + col];
                job.append(
                    cell.symbol(),
                    0.0,
                    egui::text::TextFormat {
                        font_id: font_id.clone(),
                        color: rat_to_egui(cell.fg),
                        ..Default::default()
                    },
                );
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let pos = egui::pos2(origin.x, (origin.y + row as f32 * cell_h).round());
            painter.galley(pos, galley, egui::Color32::WHITE);
        }

        if let Some(pos) = clicked {
            let col = ((pos.x - origin.x) / cell_w).floor() as i32;
            let row = ((pos.y - origin.y) / cell_h).floor() as i32;
            if col >= 0 && row >= 0 {
                self.click(col as u16, row as u16);
            }
        }

        // The updating box gets the real app logo. The tui can only paint text
        // rows, so the image has to go in here, over the space the popup left.
        if self.app.updating {
            if let Some((nx, ny, nw, _nh)) = self.app.notice_rect {
                let size = 16.0;
                let c = egui::pos2(
                    origin.x + (nx as f32 + 3.0) * cell_w,
                    origin.y + (ny as f32 + 2.0) * cell_h,
                );
                let _ = nw;
                if let Some(tex) = &self.logo {
                    painter.image(
                        tex.id(),
                        egui::Rect::from_min_size(c, egui::vec2(size, size)),
                        egui::Rect::from_min_max(
                            egui::pos2(0.0, 0.0),
                            egui::pos2(1.0, 1.0),
                        ),
                        egui::Color32::WHITE,
                    );
                }
            }
        }

        self.app.hover = None;
        if let Some(pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
            let col = ((pos.x - origin.x) / cell_w).floor() as i32;
            let row = ((pos.y - origin.y) / cell_h).floor() as i32;
            if col >= 0 && row >= 0 {
                let (c, r) = (col as u16, row as u16);
                for z in &self.hot {
                    if r == z.y && c >= z.x && c < z.x + z.w {
                        let kind = match z.action {
                            HotAction::Menu(_) => 0,
                            HotAction::Action(_) => 1,
                            HotAction::Setting(_) => 2,
                            HotAction::Notice(_) => 3,
                        };
                        let idx = match z.action {
                            HotAction::Menu(i) => i,
                            HotAction::Action(i) => i,
                            HotAction::Setting(i) => i,
                            HotAction::Notice(i) => i,
                        };
                        self.app.hover = Some((kind, idx));
                        break;
                    }
                }
            }
        }

        ui.ctx().request_repaint();
    }
}

fn load_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!(
        "../assets/branding/macncheese-256.png"
    ))
    .ok()?
    .to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

fn main() -> eframe::Result<()> {
    #[cfg(windows)]
    ensure_admin();
    roblox::register_protocols();
    let mut vp = egui::ViewportBuilder::default()
        .with_title("cheesestrap")
        .with_inner_size([980.0, 640.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_decorations(false);
    if let Some(icon) = load_icon() {
        vp = vp.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "cheesestrap",
        options,
        Box::new(|_cc| Ok(Box::new(CheeseApp::new()))),
    )
}
