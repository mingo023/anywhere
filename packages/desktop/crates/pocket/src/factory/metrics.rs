use agents::factory::Metrics;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const DASH: &str = "–";

/// A duration in its largest two units: `45s`, `12m`, `1h 5m`, `2d 4h`.
pub fn span(secs: i64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    match secs {
        ..=0 => DASH.into(),
        1..60 => format!("{secs}s"),
        60..3600 => format!("{m}m"),
        3600..86_400 if m == 0 => format!("{h}h"),
        3600..86_400 => format!("{h}h {m}m"),
        _ if h == 0 => format!("{d}d"),
        _ => format!("{d}d {h}h"),
    }
}

/// Each tile's value and label.
pub fn tiles(m: Option<&Metrics>) -> [(String, &'static str); 4] {
    let m = m.cloned().unwrap_or_default();
    let merged = if m.merged_per_day > 0. { format!("{:.1}", m.merged_per_day) } else { DASH.into() };
    let fixed = if m.fix_mean > 0. { format!("{:.0}% · {:.1}", m.fix_rate * 100., m.fix_mean) } else { DASH.into() };
    [
        (merged, "Merged per day"),
        (span(m.start_to_ready), "Start → PR ready"),
        (format!("{} · {} · {}", span(m.wait_plan), span(m.wait_pr), span(m.wait_ask)), "Wait at Plan · PR · Ask"),
        (fixed, "Auto-fix success · tries"),
    ]
}

/// The four tiles and the line under them.
pub fn metrics_strip(m: Option<&Metrics>) -> Div {
    let cells = tiles(m).into_iter().enumerate().map(|(i, (value, label))| {
        div()
            .flex_1()
            .min_w_0()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .when(i > 0, |d| d.border_l(px(0.5)).border_color(SEPARATOR))
            .child(div().truncate().text_size(px(15.)).line_height(px(19.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(value))
            .child(div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(label))
    });
    let strip = div().flex().rounded(px(9.)).bg(SURFACE).shadow(ui::row_shadow()).overflow_hidden().children(cells);
    div().flex().flex_col().gap(px(6.)).child(strip).child(div().ml(px(2.)).text_size(px(11.5)).text_color(TEXT_4).child("Last 14 days · computed on this Mac"))
}

#[cfg(test)]
mod tests {
    use super::{span, tiles};
    use agents::factory::Metrics;

    #[test]
    fn a_span_reads_in_its_largest_two_units_and_none_reads_as_a_dash() {
        assert_eq!([span(0), span(45), span(720), span(3600), span(3900), span(11_400), span(187_200)], ["–", "45s", "12m", "1h", "1h 5m", "3h 10m", "2d 4h"]);
    }

    #[test]
    fn the_four_tiles_read_as_the_design_has_them() {
        let m = Metrics { merged_per_day: 2.4, start_to_ready: 11_400, wait_plan: 720, wait_pr: 3900, wait_ask: 240, fix_rate: 0.78, fix_mean: 1.3, days: 14 };
        let values: Vec<String> = tiles(Some(&m)).into_iter().map(|(v, _)| v).collect();
        assert_eq!(values, ["2.4", "3h 10m", "12m · 1h 5m · 4m", "78% · 1.3"]);
        let labels: Vec<&str> = tiles(Some(&m)).into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, ["Merged per day", "Start → PR ready", "Wait at Plan · PR · Ask", "Auto-fix success · tries"]);
    }

    #[test]
    fn a_project_with_no_data_reads_as_dashes() {
        let none: Vec<String> = tiles(None).into_iter().map(|(v, _)| v).collect();
        assert_eq!(none, ["–", "–", "– · – · –", "–"]);
        let empty: Vec<String> = tiles(Some(&Metrics { days: 14, ..Default::default() })).into_iter().map(|(v, _)| v).collect();
        assert_eq!(empty, none);
        let never_failed = Metrics { merged_per_day: 1., wait_plan: 60, days: 14, ..Default::default() };
        assert_eq!(tiles(Some(&never_failed))[3].0, "–", "no fix ended, so no share");
    }
}
