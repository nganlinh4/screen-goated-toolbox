use super::View;
use windows::Win32::Foundation::{POINT, RECT};

pub(super) struct Layout {
    pub window: RECT,
    pub footer: RECT,
    pub quit: RECT,
    pub handles: [RECT; 8],
    pub edges: [RECT; 4],
}

pub(super) fn contains(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

impl Layout {
    pub fn new(view: &View, scale: f32) -> Self {
        let px = |v: f32| (v * scale).round() as i32;
        let r = view.rect;
        let b = view.bounds;
        let radius = px(5.0).max(3);
        let handles = crate::overlay::selection::region_handles(r, b, radius).map(|p| RECT {
            left: p.x - radius,
            top: p.y - radius,
            right: p.x + radius,
            bottom: p.y + radius,
        });
        let line = px(1.0).max(1);
        let edges = [
            RECT {
                bottom: r.top + line,
                ..r
            },
            RECT {
                top: r.bottom - line,
                ..r
            },
            RECT {
                right: r.left + line,
                ..r
            },
            RECT {
                left: r.right - line,
                ..r
            },
        ];
        let width = px(440.0).min(b.right - b.left);
        let height = px(52.0).min(b.bottom - b.top);
        let left = ((r.left + r.right - width) / 2).clamp(b.left, b.right - width);
        let below = r.bottom + px(10.0);
        let top = if below + height <= b.bottom {
            below
        } else {
            (r.top - px(10.0) - height).max(b.top)
        };
        let footer = RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
        let quit = RECT {
            left: footer.right - px(64.0),
            ..footer
        };
        let mut window = footer;
        for piece in handles.iter().chain(edges.iter()) {
            window.left = window.left.min(piece.left);
            window.top = window.top.min(piece.top);
            window.right = window.right.max(piece.right);
            window.bottom = window.bottom.max(piece.bottom);
        }
        Self {
            window,
            footer,
            quit,
            handles,
            edges,
        }
    }

    pub fn hit(&self, point: POINT) -> Option<usize> {
        if contains(self.quit, point) {
            return Some(9);
        }
        if let Some(index) = self.handles.iter().position(|r| contains(*r, point)) {
            return Some(index);
        }
        (contains(self.footer, point) || self.edges.iter().any(|r| contains(*r, point)))
            .then_some(8)
    }

    pub fn pieces(&self) -> impl Iterator<Item = RECT> + '_ {
        self.handles
            .iter()
            .chain(self.edges.iter())
            .copied()
            .chain([self.footer])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_excludes_interior_and_all_controls_stay_on_negative_origin_monitor() {
        let view = View {
            rect: RECT {
                left: -1800,
                top: 800,
                right: -100,
                bottom: 1080,
            },
            bounds: RECT {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
            editing: true,
            error: String::new(),
            epoch: 1,
            language: "en".into(),
            hotkey: "F10".into(),
        };
        for scale in [1.0, 1.5, 2.0] {
            let layout = Layout::new(&view, scale);
            assert_eq!(layout.hit(POINT { x: -900, y: 900 }), None);
            assert!(
                layout
                    .pieces()
                    .all(|r| r.left >= -1920 && r.right <= 0 && r.top >= 0 && r.bottom <= 1080)
            );
            for (i, r) in layout.handles.iter().enumerate() {
                assert_eq!(
                    layout.hit(POINT {
                        x: (r.left + r.right) / 2,
                        y: (r.top + r.bottom) / 2
                    }),
                    Some(i)
                );
            }
            assert_eq!(
                layout.hit(POINT {
                    x: layout.quit.right - 5,
                    y: layout.quit.top + 5
                }),
                Some(9)
            );
        }
    }
}
