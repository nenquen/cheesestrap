use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme::*;

#[derive(Clone, Copy)]
pub enum HotAction {
    Menu(usize),
    Action(usize),
    Setting(usize),
    Notice(usize),
}

pub struct HotZone {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub action: HotAction,
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Plain)
        .border_style(Style::default().fg(CRUST))
        .title_style(Style::default().fg(CHEESE).add_modifier(Modifier::BOLD))
        .title(Span::styled(format!(" {title} "), Style::default().fg(CHEESE)))
        .style(Style::default().bg(PANEL))
}

/// Log lines are coloured by how they start, which is enough to tell progress
/// from a result without tagging every call site.
fn log_style(line: &str) -> Style {
    if line.contains("fail") {
        Style::default().fg(BAD)
    } else if line.starts_with("download") || line.starts_with("installing") {
        Style::default().fg(MELT)
    } else if line.starts_with("ready")
        || line.starts_with("roblox launched")
        || line.starts_with("webview2")
    {
        Style::default().fg(GOOD)
    } else if line.starts_with("version")
        || line.starts_with("latest")
        || line.starts_with("checking")
        || line.starts_with("updating")
    {
        Style::default().fg(CHEESE)
    } else {
        Style::default().fg(CREAM)
    }
}

fn status_lines(app: &App) -> Vec<Line<'_>> {
    let installed = app.installed.as_deref().unwrap_or("none");
    let latest = app.latest.as_deref().unwrap_or("...");
    vec![
        Line::from(vec![
            Span::styled("installed: ", Style::default().fg(DIM)),
            Span::styled(installed, Style::default().fg(GOOD)),
        ]),
        Line::from(vec![
            Span::styled("latest:    ", Style::default().fg(DIM)),
            Span::styled(latest, Style::default().fg(CHEESE)),
        ]),
    ]
}

pub fn draw(f: &mut Frame, app: &mut App, hot: &mut Vec<HotZone>) {
    hot.clear();
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(BG)),
        area,
    );

    // while the update box is up it owns the whole window. drawing the panels
    // underneath would peek out around it and look like a half open dialog.
    if app.updating || app.update.is_some() {
        draw_notice(f, app, area, hot);
        return;
    }

    if app.using_keyboard && !app.editing_args && app.show_hints {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        draw_main(f, app, rows[0], hot);

        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!("  v{} tab: panel   up/down: move   enter: go   esc: back", env!("CARGO_PKG_VERSION")),
                Style::default().fg(DIM),
            )])),
            rows[1],
        );
    } else {
        draw_main(f, app, area, hot);
    }
}

/// The update screen. While it is up nothing else is drawn, so it gets the
/// whole window. It also doubles as the progress display once update is
/// pressed. Button row is update / later, update is the default.
fn draw_notice(
    f: &mut Frame,
    app: &mut App,
    area: ratatui::layout::Rect,
    hot: &mut Vec<HotZone>,
) {
    app.notice_rect = None;

    let w = (area.width as usize - 10).clamp(40, 56) as u16;
    let h: u16 = if app.updating { 7 } else { 8 };
    let rect = ratatui::layout::Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    app.notice_rect = Some((rect.x, rect.y, rect.width, rect.height));

    let inner_w = w.saturating_sub(2) as usize;
    let mut lines: Vec<Line<'_>> = vec![Line::from("")];

    if app.updating {
        let pct = app.update_pct;
        // egui paints the logo over the left of this row, keep it clear
        lines.push(Line::from(vec![
            Span::styled("        ", Style::default().fg(DIM)),
            Span::styled(
                "updating",
                Style::default().fg(CHEESE).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
        let bar_w = inner_w.saturating_sub(10).max(4);
        let filled = (bar_w * pct as usize / 100).min(bar_w);
        let bar: String = (0..bar_w)
            .map(|i| if i < filled { '#' } else { '.' })
            .collect();
        lines.push(Line::from(vec![
            Span::styled(format!("  {bar}  "), Style::default().fg(CHEESE)),
            Span::styled(format!("{pct:>3}%"), Style::default().fg(GOOD)),
        ]));
        f.render_widget(Paragraph::new(lines).block(panel("updating")), rect);
        return;
    }

    let rel = app.update.as_ref().expect("checked above");
    lines.push(Line::from(vec![Span::styled(
        format!("  cheesestrap {} is ready.", rel.version),
        Style::default().fg(CHEESE).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        format!("  you are running {}.", crate::update::current()),
        Style::default().fg(CREAM),
    )]));
    lines.push(Line::from(""));

    // one button, there is no way past an update
    let label = "update";
    let x = inner_w / 2 - label.len() / 2;
    lines.push(Line::from(vec![
        Span::styled(" ".repeat(x), Style::default().fg(CREAM)),
        Span::styled(
            format!(" {label} "),
            Style::default().fg(BG).bg(SELECT).add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(Paragraph::new(lines).block(panel("update available")), rect);

    hot.push(HotZone {
        x: rect.x + 1 + x as u16,
        y: rect.y + h - 2,
        w: label.len() as u16 + 2,
        action: HotAction::Notice(0),
    });
}

fn draw_main(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    hot: &mut Vec<HotZone>,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(24), Constraint::Percentage(76)])
        .split(area);

    let items = ["play roblox", "settings", "quit"];
    let inner_h = cols[0].height.saturating_sub(2) as usize;
    let pad_top = 1.min(inner_h.saturating_sub(items.len()));
    let mut list_items: Vec<ListItem> = Vec::new();
    for _ in 0..pad_top {
        list_items.push(ListItem::new(Line::from("")));
    }
    for (i, name) in items.iter().enumerate() {
        let on = (app.using_keyboard && i == app.menu_idx)
            || app.hover == Some((0, i));
        let style = if on {
            Style::default().fg(BG).bg(SELECT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(CREAM)
        };
        list_items.push(ListItem::new(Line::from(vec![Span::styled(
            format!("  {name}  "),
            style,
        )])));
    }
    let menu = List::new(list_items).block(panel("menu"));
    f.render_widget(menu, cols[0]);
    for (i, _) in items.iter().enumerate() {
        hot.push(HotZone {
            x: cols[0].x + 1,
            y: cols[0].y + 1 + pad_top as u16 + i as u16,
            w: cols[0].width.saturating_sub(2),
            action: HotAction::Menu(i),
        });
    }

    if app.show_settings() {
        draw_settings(f, app, cols[1], hot);
        return;
    }

    let mut cons = vec![
        Constraint::Length(4),
        Constraint::Min(0),
    ];
    cons.push(Constraint::Length(3));
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints(cons)
        .split(cols[1]);
    let launch_idx = right.len() - 1;

    f.render_widget(
        Paragraph::new(status_lines(app)).block(panel("status")),
        right[0],
    );

    let btn = app.play_label();
    let inner_w = right[2].width.saturating_sub(2) as usize;
    let label = format!("open folder | {btn}");
    let pad = inner_w.saturating_sub(label.len() + 2);
    let play_on = (app.using_keyboard
        && app.focus == crate::app::Focus::Content
        && app.action_idx == 1)
        || app.hover == Some((1, 1));
    let folder_on = (app.using_keyboard
        && app.focus == crate::app::Focus::Content
        && app.action_idx == 0)
        || app.hover == Some((1, 0));
    let line = Line::from(vec![
        Span::styled(
            " ".repeat(pad),
            Style::default().fg(CREAM),
        ),
        Span::styled(
            "open folder",
            if folder_on {
                Style::default().fg(BG).bg(SELECT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(CREAM)
            },
        ),
        Span::styled(" | ", Style::default().fg(DIM)),
        Span::styled(
            btn,
            if play_on {
                Style::default().fg(BG).bg(SELECT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(CHEESE).add_modifier(Modifier::BOLD)
            },
        ),
        Span::styled("  ", Style::default().fg(CREAM)),
    ]);
    f.render_widget(Paragraph::new(line).block(panel("launch")), right[launch_idx]);
    let base_x = right[launch_idx].x + 1 + pad as u16;
    let y = right[launch_idx].y + 1;
    hot.push(HotZone {
        x: base_x,
        y,
        w: 11,
        action: HotAction::Action(0),
    });
    hot.push(HotZone {
        x: base_x + 14,
        y,
        w: btn.len() as u16,
        action: HotAction::Action(1),
    });

    let log_lines: Vec<Line> = {
        let w = right[1].width.saturating_sub(2).max(1) as usize;
        let inner = right[1].height.saturating_sub(2).max(1) as usize;
        let len = app.log.len();
        let end = app.log_view_end.unwrap_or(len).min(len);
        // wrap every visible line, keep only as many wrapped rows as the box
        // is tall from the bottom, so the newest output is what stays up
        let mut rows: Vec<Line> = Vec::new();
        for l in &app.log[..end] {
            let style = log_style(l);
            let chars: Vec<char> = l.chars().collect();
            for chunk in chars.chunks(w) {
                rows.push(Line::from(vec![Span::styled(
                    chunk.iter().collect::<String>(),
                    style,
                )]));
            }
        }
        rows.split_off(rows.len().saturating_sub(inner))
    };
    f.render_widget(Paragraph::new(log_lines).block(panel("log")), right[1]);
}

fn draw_settings(
    f: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    hot: &mut Vec<HotZone>,
) {
    let rows = [
        (
            "save logs to file",
            if app.logs_to_file { "[x]" } else { "[ ]" },
            true,
        ),
        (
            "show hints bar",
            if app.show_hints { "[x]" } else { "[ ]" },
            true,
        ),
        (
            "repair webview2",
            if app.webview_ok { "" } else { "needed" },
            !app.webview_ok,
        ),
        ("uninstall roblox", "", app.can_uninstall),
    ];
    let label_w = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(0);
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, (label, value, enabled))| {
            let on = *enabled
                && ((app.using_keyboard && i == app.settings_idx)
                    || app.hover == Some((2, i)));
            let style = if on {
                Style::default().fg(BG).bg(SELECT).add_modifier(Modifier::BOLD)
            } else if !enabled {
                Style::default().fg(DIM)
            } else {
                Style::default().fg(CREAM)
            };
            let marker = " ";
            ListItem::new(Line::from(vec![Span::styled(
                format!("{marker} {label:<label_w$}  {value} "),
                style,
            )]))
        })
        .collect();
    f.render_widget(List::new(items).block(panel("settings")), area);
    for (i, (_, _, enabled)) in rows.iter().enumerate() {
        if !enabled {
            continue;
        }
        hot.push(HotZone {
            x: area.x + 1,
            y: area.y + 1 + i as u16,
            w: area.width.saturating_sub(2),
            action: HotAction::Setting(i),
        });
    }
    if let Some((2, i)) = app.hover {
        let desc = match i {
            0 => "write every log line to a file.",
            1 => "show the key hints at the bottom.",
            2 => "download and install the webview2 runtime roblox needs.",
            _ => "delete everything roblox from this pc.",
        };
        let dy = area.y + 1 + rows.len() as u16 + 1;
        if dy < area.y + area.height {
            let r = ratatui::layout::Rect::new(
                area.x + 1,
                dy,
                area.width.saturating_sub(2),
                1,
            );
            f.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(
                    desc,
                    Style::default().fg(DIM),
                )])),
                r,
            );
        }
    }
}
