//! Vector icon enum and 1.6px stroke primitive painter.
//!
//! Entry points: `Icon`, `draw_icon`.

use egui::{Color32, Painter, Pos2, Rect, Stroke};

/// Icons drawn via vector primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Icon {
    Plus,
    Pencil,
    Trash,
    Fullscreen,
    ExitFullscreen,
    Search,
    Check,
    More,
}

/// Draw a vector icon centered within a rectangle using 1.6px strokes.
#[allow(dead_code)]
pub fn draw_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let pt = |dx: f32, dy: f32| Pos2::new(c.x + dx, c.y + dy);
    let stroke = Stroke::new(1.6, color);

    match icon {
        Icon::Plus => {
            painter.line_segment([pt(-6.0, 0.0), pt(6.0, 0.0)], stroke);
            painter.line_segment([pt(0.0, -6.0), pt(0.0, 6.0)], stroke);
        }
        Icon::Pencil => {
            let pts = vec![
                pt(-5.0, 5.0),
                pt(-4.0, 1.5),
                pt(2.5, -5.0),
                pt(5.0, -2.5),
                pt(-1.5, 4.0),
                pt(-5.0, 5.0),
            ];
            painter.add(egui::Shape::line(pts, stroke));
            painter.line_segment([pt(-4.0, 1.5), pt(-1.5, 4.0)], stroke);
        }
        Icon::Trash => {
            painter.line_segment([pt(-5.0, -3.5), pt(5.0, -3.5)], stroke);
            painter.add(egui::Shape::line(
                vec![pt(-1.5, -3.5), pt(-1.5, -5.0), pt(1.5, -5.0), pt(1.5, -3.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-3.5, -3.5), pt(-2.5, 5.5), pt(2.5, 5.5), pt(3.5, -3.5)],
                stroke,
            ));
            painter.line_segment([pt(-1.0, -1.5), pt(-0.8, 3.5)], stroke);
            painter.line_segment([pt(1.0, -1.5), pt(0.8, 3.5)], stroke);
        }
        Icon::Fullscreen => {
            painter.add(egui::Shape::line(
                vec![pt(-6.0, -2.5), pt(-6.0, -6.0), pt(-2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(2.5, -6.0), pt(6.0, -6.0), pt(6.0, -2.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, 2.5), pt(6.0, 6.0), pt(2.5, 6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-2.5, 6.0), pt(-6.0, 6.0), pt(-6.0, 2.5)],
                stroke,
            ));
        }
        Icon::ExitFullscreen => {
            painter.add(egui::Shape::line(
                vec![pt(-6.0, -2.5), pt(-2.5, -2.5), pt(-2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, -2.5), pt(2.5, -2.5), pt(2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, 2.5), pt(2.5, 2.5), pt(2.5, 6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-6.0, 2.5), pt(-2.5, 2.5), pt(-2.5, 6.0)],
                stroke,
            ));
        }
        Icon::Search => {
            painter.circle_stroke(pt(-1.5, -1.5), 4.5, stroke);
            painter.line_segment([pt(1.8, 1.8), pt(5.5, 5.5)], stroke);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(
                vec![pt(-5.0, 0.0), pt(-1.5, 4.0), pt(5.5, -3.5)],
                stroke,
            ));
        }
        Icon::More => {
            painter.circle_filled(pt(-4.5, 0.0), 1.5, color);
            painter.circle_filled(pt(0.0, 0.0), 1.5, color);
            painter.circle_filled(pt(4.5, 0.0), 1.5, color);
        }
    }
}
