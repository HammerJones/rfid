//! Colors, icons and the overall look of the app.

use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke};

// A dark navy theme with teal, violet and amber accents.
pub const BG: Color32 = Color32::from_rgb(13, 17, 28);
pub const PANEL: Color32 = Color32::from_rgb(20, 26, 41);
pub const CARD: Color32 = Color32::from_rgb(27, 35, 54);
pub const CARD_HOVER: Color32 = Color32::from_rgb(36, 46, 70);
pub const BORDER: Color32 = Color32::from_rgb(45, 56, 82);
pub const TEXT: Color32 = Color32::from_rgb(230, 235, 245);
pub const MUTED: Color32 = Color32::from_rgb(140, 152, 178);
pub const ACCENT: Color32 = Color32::from_rgb(45, 212, 191); // teal
pub const VIOLET: Color32 = Color32::from_rgb(139, 122, 255);
pub const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
pub const GOOD: Color32 = Color32::from_rgb(74, 222, 128);
pub const BAD: Color32 = Color32::from_rgb(248, 113, 113);
pub const SKY: Color32 = Color32::from_rgb(56, 189, 248);

/// Colors handed out to categories, in order.
const CATEGORY_COLORS: [Color32; 8] = [
    Color32::from_rgb(56, 189, 248),  // sky
    Color32::from_rgb(251, 146, 60),  // orange
    Color32::from_rgb(139, 122, 255), // violet
    Color32::from_rgb(244, 114, 182), // pink
    Color32::from_rgb(74, 222, 128),  // green
    Color32::from_rgb(251, 191, 36),  // amber
    Color32::from_rgb(45, 212, 191),  // teal
    Color32::from_rgb(248, 113, 113), // red
];

pub fn category_color(index: usize) -> Color32 {
    CATEGORY_COLORS[index % CATEGORY_COLORS.len()]
}

/// An icon picked from the category's name.
pub fn category_icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    let has = |ws: &[&str]| ws.iter().any(|w| n.contains(w));
    if has(&["transport", "car", "auto", "vehicle", "truck", "bike", "gas"]) {
        "🚗"
    } else if has(&["food", "grocer", "dining", "restaurant", "eat", "meal"]) {
        "🍔"
    } else if has(&["hous", "home", "rent", "utilit", "mortgage"]) {
        "🏠"
    } else if has(&["subscri", "stream", "entertain", "media"]) {
        "📺"
    } else if has(&["health", "medical", "pharm", "doctor", "insur"]) {
        "💊"
    } else if has(&["sav", "invest", "emergency"]) {
        "💰"
    } else if has(&["debt", "loan", "credit", "card"]) {
        "💳"
    } else {
        "🏷"
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(10, 13, 22); // text fields, progress bar track
    v.faint_bg_color = Color32::from_rgb(22, 29, 45); // striped rows
    v.code_bg_color = Color32::from_rgb(10, 13, 22);
    v.hyperlink_color = ACCENT;
    v.override_text_color = None;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = CornerRadius::same(16);
    v.menu_corner_radius = CornerRadius::same(12);
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.selection.bg_fill = Color32::from_rgb(20, 120, 110);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.warn_fg_color = AMBER;
    v.error_fg_color = BAD;

    let r = CornerRadius::same(10);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = CARD;
    w.inactive.weak_bg_fill = CARD;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = CARD_HOVER;
    w.hovered.weak_bg_fill = CARD_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
    w.hovered.corner_radius = r;
    w.active.bg_fill = Color32::from_rgb(20, 120, 110);
    w.active.weak_bg_fill = Color32::from_rgb(20, 120, 110);
    w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    w.active.corner_radius = r;
    w.open.bg_fill = CARD_HOVER;
    w.open.weak_bg_fill = CARD_HOVER;
    w.open.corner_radius = r;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.window_margin = Margin::same(16);
        s.spacing.menu_margin = Margin::same(10);
        use egui::{FontFamily::Proportional, FontId, TextStyle};
        s.text_styles.insert(TextStyle::Heading, FontId::new(22.0, Proportional));
        s.text_styles.insert(TextStyle::Body, FontId::new(15.0, Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(15.0, Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, Proportional));
    });
}

/// A rounded card.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(12))
}

/// A card with a colored stripe feel: tinted fill and colored border.
pub fn tinted_card(color: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(mix(CARD, color, 0.12))
        .stroke(Stroke::new(1.0_f32, mix(BORDER, color, 0.55)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(12))
}

/// Blend `a` toward `b` by `t` (0..1).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn section_title(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(format!("{icon}  {text}")).size(18.0).strong().color(TEXT));
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(MUTED)
}

/// A filled accent button.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(Color32::from_rgb(8, 30, 28)))
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(10)),
    )
}

/// A thin colored bar showing `frac` of the width.
pub fn bar(ui: &mut egui::Ui, frac: f32, color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let r = CornerRadius::same((height / 2.0).min(255.0) as u8);
    painter.rect_filled(rect, r, Color32::from_rgb(10, 13, 22));
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    if fill.width() > 0.5 {
        painter.rect_filled(fill, r, color);
    }
}
