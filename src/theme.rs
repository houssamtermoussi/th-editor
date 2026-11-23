use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Vec2};

pub const BG_CANVAS: Color32 = Color32::from_rgb(14, 15, 19);
pub const BG_PANEL: Color32 = Color32::from_rgb(22, 24, 30);
pub const BG_SURFACE: Color32 = Color32::from_rgb(30, 33, 41);
pub const BG_WIDGET: Color32 = Color32::from_rgb(40, 44, 55);
pub const BG_HOVER: Color32 = Color32::from_rgb(52, 57, 71);
pub const BORDER: Color32 = Color32::from_rgb(44, 48, 60);
pub const TEXT: Color32 = Color32::from_rgb(226, 229, 237);
pub const TEXT_DIM: Color32 = Color32::from_rgb(138, 145, 162);
pub const ACCENT: Color32 = Color32::from_rgb(104, 132, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(52, 64, 120);
pub const WARNING: Color32 = Color32::from_rgb(255, 184, 92);
pub const SUCCESS: Color32 = Color32::from_rgb(98, 210, 150);

pub const RADIUS: u8 = 6;
pub const RADIUS_LG: u8 = 10;

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(15.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();

        let spacing = &mut style.spacing;
        spacing.item_spacing = Vec2::new(8.0, 8.0);
        spacing.button_padding = Vec2::new(10.0, 5.0);
        spacing.interact_size.y = 28.0;
        spacing.slider_rail_height = 4.0;
        spacing.menu_margin = Margin::same(8);
        spacing.window_margin = Margin::same(12);

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG_PANEL;
        v.window_fill = BG_PANEL;
        v.extreme_bg_color = BG_CANVAS;
        v.faint_bg_color = BG_SURFACE;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(RADIUS_LG);
        v.menu_corner_radius = CornerRadius::same(RADIUS_LG);
        v.window_shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(110),
        };
        v.popup_shadow = v.window_shadow;
        v.selection.bg_fill = ACCENT_SOFT;
        v.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(214, 222, 255));
        v.hyperlink_color = ACCENT;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let radius = CornerRadius::same(RADIUS);
        let w = &mut v.widgets;

        w.noninteractive.bg_fill = BG_PANEL;
        w.noninteractive.weak_bg_fill = BG_PANEL;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
        w.noninteractive.corner_radius = radius;

        w.inactive.bg_fill = BG_WIDGET;
        w.inactive.weak_bg_fill = BG_WIDGET;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = radius;
        w.inactive.expansion = 0.0;

        w.hovered.bg_fill = BG_HOVER;
        w.hovered.weak_bg_fill = BG_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(70, 76, 94));
        w.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = ACCENT_SOFT;
        w.active.weak_bg_fill = ACCENT_SOFT;
        w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open = w.hovered;
    });
}

/// Rounded "card" used to group controls in the side panel.
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(BG_SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(RADIUS_LG)
        .inner_margin(Margin::same(12))
}

/// Primary call-to-action button (filled with the accent color).
pub fn primary_button<'a>(text: impl Into<egui::WidgetText>) -> egui::Button<'a> {
    egui::Button::new(text.into().color(Color32::WHITE).strong()).fill(ACCENT)
}

pub fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).heading().strong().color(TEXT));
    if !subtitle.is_empty() {
        ui.label(egui::RichText::new(subtitle).small().color(TEXT_DIM));
    }
    ui.add_space(4.0);
}
