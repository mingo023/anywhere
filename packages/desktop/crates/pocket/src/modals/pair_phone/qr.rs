use gpui_kit::*;
use std::sync::Arc;

pub(super) const SIDE: f32 = 240.;

/// A horizontal run of dark modules, painted as one quad.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Run {
    row: usize,
    col: usize,
    len: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Qr {
    size: usize,
    runs: Arc<[Run]>,
}

impl Qr {
    pub(super) fn encode(url: &str) -> Option<Self> {
        let code = qrcode::QrCode::new(url).ok()?;
        let size = code.width();
        let dark: Vec<bool> = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
        let mut runs = Vec::new();
        for (row, modules) in dark.chunks(size).enumerate() {
            let mut x = 0;
            while x < size {
                let col = x;
                while x < size && modules[x] {
                    x += 1;
                }
                if x > col {
                    runs.push(Run { row, col, len: x - col });
                }
                x += 1;
            }
        }
        Some(Self { size, runs: runs.into() })
    }
}

pub(super) fn view(qr: &Qr) -> impl IntoElement {
    let (modules, runs) = (qr.size, qr.runs.clone());
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let m = f32::from(bounds.size.width) / modules as f32;
            for r in runs.iter() {
                let origin = bounds.origin + point(px(r.col as f32 * m), px(r.row as f32 * m));
                window.paint_quad(fill(Bounds::new(origin, size(px(r.len as f32 * m), px(m))), rgb(0x000000)));
            }
        },
    )
    .size(px(SIDE))
}

#[cfg(test)]
mod tests {
    use super::Qr;

    #[test]
    fn qr_runs_cover_exactly_the_dark_modules() {
        let url = "anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv";
        let qr = Qr::encode(url).unwrap();
        let mut painted = vec![false; qr.size * qr.size];
        for r in qr.runs.iter() {
            for dx in 0..r.len {
                painted[r.row * qr.size + r.col + dx] = true;
            }
        }
        let dark: Vec<bool> = qrcode::QrCode::new(url).unwrap().to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
        assert_eq!(painted, dark);
    }
}
