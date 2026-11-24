use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};

pub fn draw_plus_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8_f32, color);
    painter.line_segment([Pos2::new(c.x - 6.2, c.y), Pos2::new(c.x + 6.2, c.y)], s);
    painter.line_segment([Pos2::new(c.x, c.y - 6.2), Pos2::new(c.x, c.y + 6.2)], s);
}

pub fn draw_grid_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.5_f32, color);
    for dx in [-3.8_f32, 3.8_f32] {
        for dy in [-3.8_f32, 3.8_f32] {
            let r = Rect::from_center_size(Pos2::new(c.x + dx, c.y + dy), Vec2::splat(4.8));
            painter.rect_stroke(r, CornerRadius::same(1), s, egui::StrokeKind::Middle);
        }
    }
}

pub fn draw_copy_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.5_f32, color);
    let back = Rect::from_center_size(c + Vec2::new(2.3, -2.3), Vec2::new(8.8, 9.8));
    let front = Rect::from_center_size(c + Vec2::new(-2.3, 2.3), Vec2::new(8.8, 9.8));
    painter.rect_stroke(back, CornerRadius::same(1), s, egui::StrokeKind::Middle);
    painter.rect_filled(front, CornerRadius::same(1), color.gamma_multiply(0.18));
    painter.rect_stroke(front, CornerRadius::same(1), s, egui::StrokeKind::Middle);
}

pub fn draw_pin_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    painter.circle_stroke(Pos2::new(c.x, c.y - 3.0), 3.6, s);
    painter.line_segment([Pos2::new(c.x, c.y + 1.0), Pos2::new(c.x, c.y + 6.4)], s);
    painter.line_segment([Pos2::new(c.x - 4.2, c.y + 1.0), Pos2::new(c.x + 4.2, c.y + 1.0)], s);
}

pub fn draw_gear_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.55_f32, color);
    painter.circle_stroke(c, 4.0, s);
    for i in 0..6 {
        let ang = (i as f32) * std::f32::consts::TAU / 6.0;
        let dir = Vec2::new(ang.cos(), ang.sin());
        painter.line_segment([c + dir * 4.4, c + dir * 6.6], s);
    }
}

pub fn draw_hide_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.85_f32, color);
    painter.line_segment([Pos2::new(c.x - 5.8, c.y + 2.0), Pos2::new(c.x + 5.8, c.y + 2.0)], s);
}

pub fn draw_chevron_left_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8_f32, color);
    painter.line_segment([Pos2::new(c.x + 2.8, c.y - 5.2), Pos2::new(c.x - 3.0, c.y)], s);
    painter.line_segment([Pos2::new(c.x - 3.0, c.y), Pos2::new(c.x + 2.8, c.y + 5.2)], s);
}

pub fn draw_chevron_right_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8_f32, color);
    painter.line_segment([Pos2::new(c.x - 2.8, c.y - 5.2), Pos2::new(c.x + 3.0, c.y)], s);
    painter.line_segment([Pos2::new(c.x + 3.0, c.y), Pos2::new(c.x - 2.8, c.y + 5.2)], s);
}

pub fn draw_trash_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.5_f32, color);
    painter.line_segment([Pos2::new(c.x - 5.6, c.y - 3.6), Pos2::new(c.x + 5.6, c.y - 3.6)], s);
    painter.line_segment([Pos2::new(c.x - 2.2, c.y - 5.8), Pos2::new(c.x + 2.2, c.y - 5.8)], s);
    let body = Rect::from_min_max(
        Pos2::new(c.x - 4.2, c.y - 3.2),
        Pos2::new(c.x + 4.2, c.y + 5.8),
    );
    painter.rect_stroke(body, CornerRadius::same(1), s, egui::StrokeKind::Middle);
}

pub fn draw_search_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    painter.circle_stroke(c + Vec2::new(-1.6, -1.6), 4.4, s);
    painter.line_segment([c + Vec2::new(1.7, 1.7), c + Vec2::new(5.8, 5.8)], s);
}

pub fn draw_undo_icon(painter: &Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    painter.line_segment([Pos2::new(c.x - 5.2, c.y - 1.4), Pos2::new(c.x + 3.6, c.y - 1.4)], s);
    painter.line_segment([Pos2::new(c.x + 3.6, c.y - 1.4), Pos2::new(c.x + 3.6, c.y + 4.4)], s);
    painter.line_segment([Pos2::new(c.x - 5.2, c.y - 1.4), Pos2::new(c.x - 2.0, c.y - 4.6)], s);
    painter.line_segment([Pos2::new(c.x - 5.2, c.y - 1.4), Pos2::new(c.x - 2.0, c.y + 1.8)], s);
}

pub fn draw_mini_app_badge(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(46, 118, 232));
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
        egui::StrokeKind::Inside,
    );
    let line_color = Color32::from_rgb(248, 251, 255);
    let s = Stroke::new(1.4_f32, line_color);
    let x0 = rect.min.x + rect.width() * 0.26;
    let x1 = rect.max.x - rect.width() * 0.26;
    let y0 = rect.min.y + rect.height() * 0.34;
    let y1 = rect.min.y + rect.height() * 0.52;
    let y2 = rect.min.y + rect.height() * 0.70;
    painter.line_segment([Pos2::new(x0, y0), Pos2::new(x1, y0)], s);
    painter.line_segment([Pos2::new(x0, y1), Pos2::new(x1 - 2.0, y1)], s);
    painter.line_segment([Pos2::new(x0, y2), Pos2::new(x1 - 4.0, y2)], s);
}
