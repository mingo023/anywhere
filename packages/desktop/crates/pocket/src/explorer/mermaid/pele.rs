//! Mermaid laid out by Pele, pluk-inc/markdown-preview's renderer, run in QuickJS, in its colours and type. Its SVG
//! reads colours from CSS variables, which resvg can't, so they're resolved into it here.

use resvg::usvg::fontdb::{Family, Query, Style, Weight};
use rquickjs::{Context, Function, Object, Runtime};
use std::cell::RefCell;

const PELE: &str = include_str!("../../../assets/pele/pele.global.js");
/// The system font, as markdown-preview draws labels. resvg draws nothing in the system's own variable SF, so this
/// is SF Pro where it's installed, else the closest the system has.
const SANS: [&str; 3] = ["SF Pro Text", "SF Pro", "Helvetica Neue"];
const MONO: [&str; 2] = ["SF Mono", "Menlo"];
/// markdown-preview's body size: Pele draws flowchart labels at it, other text a little smaller.
const FONT_SIZE: f64 = 15.;
const RADIUS: &str = "6";

/// Pele measures text on a canvas when it finds one; this one measures in the fonts resvg draws with. Its own
/// wrapper turns Pele's errors into markdown-preview's words.
const SHIM: &str = r#"
globalThis.OffscreenCanvas = class {
    getContext() {
        return { font: '', measureText(text) { return { width: __measure(this.font, text) }; } };
    }
};
globalThis.__render = (source, fontFamily, fontSize) => {
    try {
        return { svg: pele.render(source, { autoDirection: false, fontFamily, fontSize }).svg };
    } catch (e) {
        const summary = {
            'unsupported-diagram': 'This diagram type is not supported.',
            syntax: 'The diagram has a syntax error.',
            semantic: 'The diagram is not valid.',
            limit: 'The diagram is too large to draw.',
        }[e && e.code] || 'The diagram could not be drawn.';
        return { error: e && e.line > 0 ? `${summary} Line ${e.line}, column ${e.column}` : summary };
    }
};
"#;

thread_local! {
    /// Loading Pele takes ~60ms, so each thread keeps its engine.
    static ENGINE: RefCell<Option<(Runtime, Context)>> = const { RefCell::new(None) };
}

fn js(e: rquickjs::Error) -> String {
    e.to_string()
}

fn start() -> Result<(Runtime, Context), String> {
    let runtime = Runtime::new().map_err(js)?;
    let context = Context::full(&runtime).map_err(js)?;
    context
        .with(|cx| -> rquickjs::Result<()> {
            cx.globals().set("__measure", Function::new(cx.clone(), |font: String, text: String| measure(&font, &text))?)?;
            cx.eval::<(), _>(SHIM)?;
            cx.eval::<(), _>(PELE)
        })
        .map_err(js)?;
    Ok((runtime, context))
}

/// Pele's SVG for mermaid `source`, or why it can't draw it.
pub(super) fn render(source: &str) -> Result<String, String> {
    if source.trim().is_empty() {
        return Err("This diagram is empty.".into());
    }
    ENGINE.with_borrow_mut(|engine| {
        if engine.is_none() {
            *engine = Some(start()?);
        }
        let (_, context) = engine.as_ref().unwrap();
        context.with(|cx| {
            let render: Function = cx.globals().get("__render").map_err(js)?;
            let out: Object = render.call((source, family(&SANS, "sans-serif"), FONT_SIZE)).map_err(js)?;
            match out.get::<_, Option<String>>("svg").map_err(js)? {
                Some(svg) => Ok(svg),
                None => Err(out.get::<_, String>("error").map_err(js)?),
            }
        })
    })
}

/// A CSS font family list of `names`, then `generic`.
fn family(names: &[&str], generic: &str) -> String {
    names.iter().map(|n| format!("'{n}', ")).collect::<String>() + generic
}

/// Width of `text` in a canvas `font` such as `bold 15px 'SF Pro Text', …`, from the advances of the face resvg
/// draws it in.
fn measure(font: &str, text: &str) -> f64 {
    let size = font.split_whitespace().find_map(|w| w.strip_suffix("px")?.parse::<f64>().ok()).unwrap_or(FONT_SIZE);
    let mono = font.contains("monospace");
    let mut families: Vec<Family> = if mono { &MONO[..] } else { &SANS[..] }.iter().map(|n| Family::Name(n)).collect();
    families.push(if mono { Family::Monospace } else { Family::SansSerif });
    let query = Query { families: &families, weight: if font.contains("bold") { Weight::BOLD } else { Weight::NORMAL }, style: if font.contains("italic") { Style::Italic } else { Style::Normal }, ..Query::default() };
    let db = &super::USVG.fontdb;
    let width = db.query(&query).and_then(|id| {
        db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let em = f64::from(face.units_per_em());
            Some(text.chars().map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).map_or(0.55, |a| f64::from(a) / em)).sum::<f64>())
        })?
    });
    width.unwrap_or(text.chars().count() as f64 * 0.55) * size
}

fn rgb(c: u32) -> [f32; 3] {
    [(c >> 24) as f32, (c >> 16 & 0xff) as f32, (c >> 8 & 0xff) as f32]
}

/// `a` mixed into `b` at weight `t`, as CSS `color-mix(in srgb, a t, b)`.
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] * t + b[i] * (1. - t))
}

fn css(c: [f32; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0].round() as u8, c[1].round() as u8, c[2].round() as u8)
}

/// Pele's inputs as markdown-preview sets them on `canvas`, an opaque `0xRRGGBBAA`: each tone the text mixed into
/// the canvas at a fixed weight, chart series from its code palette.
fn variables(canvas: u32, dark: bool) -> Vec<(&'static str, String)> {
    let bg = rgb(canvas);
    // Dark text is the system label colour, white at 85%.
    let text = if dark { mix([255.; 3], bg, 0.85) } else { [0.; 3] };
    let (link, [keyword, string, comment, kind, variable, preprocessor, attribute]) = match dark {
        true => (0x419cffff, [0xfc5fa3ff, 0xfc6a5dff, 0x6c7986ff, 0xd0a8ffff, 0x67b7a4ff, 0xfd8f3fff, 0xbf8555ff]),
        false => (0x0068daff, [0x9b2393ff, 0xc41a16ff, 0x5d6c79ff, 0x3900a0ff, 0x326d74ff, 0x643820ff, 0x815f03ff]),
    };
    let tone = |t: f32| css(mix(text, bg, t));
    let hex = |c: u32| css(rgb(c));
    vec![
        ("--pele-bg", css(bg)),
        ("--pele-fg", css(text)),
        ("--pele-muted", tone(0.6)),
        ("--pele-line", tone(0.5)),
        ("--pele-surface", tone(0.03)),
        ("--pele-surface-alt", tone(0.05)),
        ("--pele-border", tone(0.2)),
        ("--pele-accent", hex(link)),
        ("--pele-critical", hex(string)),
        ("--pele-series-1", hex(link)),
        ("--pele-series-2", hex(keyword)),
        ("--pele-series-3", hex(variable)),
        ("--pele-series-4", hex(attribute)),
        ("--pele-series-5", hex(string)),
        ("--pele-series-6", hex(kind)),
        ("--pele-series-7", hex(preprocessor)),
        ("--pele-series-8", hex(comment)),
        ("--pele-radius", format!("{RADIUS}px")),
        ("--pele-title-weight", "600".into()),
        ("--pele-heading-weight", "600".into()),
        ("--pele-font", family(&SANS, "sans-serif")),
        ("--pele-font-mono", family(&MONO, "monospace")),
    ]
}

/// `s` with each `var(--name)` or `var(--name,fallback)` that `value` knows replaced, the rest by their fallback if
/// they have one.
fn substitute(s: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("var(--") {
        let Some(len) = rest[start..].find(')') else { break };
        let (name, fallback) = rest[start + 4..start + len].split_once(',').map_or((&rest[start + 4..start + len], None), |(n, f)| (n, Some(f)));
        out.push_str(&rest[..start]);
        out.push_str(&value(name).or(fallback.map(str::to_string)).unwrap_or_else(|| rest[start..=start + len].to_string()));
        rest = &rest[start + len + 1..];
    }
    out + rest
}

/// Pele's `svg` with its variables resolved for `canvas`, an opaque `0xRRGGBBAA`.
pub(super) fn resolve(svg: &str, canvas: u32, dark: bool) -> String {
    let vars = variables(canvas, dark);
    // resvg reads neither a corner radius from CSS nor the root's responsive sizing.
    let svg = svg.replace("style=\"rx:var(--_r)\"", &format!("rx=\"{RADIUS}\"")).replacen("max-width:100%;height:auto;", "", 1);
    let svg = substitute(&svg, |name| vars.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone()));
    // The root's style aliases each input as `--_x`, which the drawing reads.
    let root = svg.split_once("style=\"").and_then(|(_, s)| s.split_once('"')).map_or("", |(s, _)| s);
    let aliases: Vec<(&str, &str)> = root.split(';').filter_map(|d| d.split_once(':')).filter(|(n, _)| n.starts_with("--_")).collect();
    let svg = substitute(&svg, |name| aliases.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()));
    // resvg misreads the root's font, falling back to serif; nothing else in that style is read once resolved.
    let style = svg.split_once("style=\"").and_then(|(_, s)| s.split_once('"')).map_or("", |(s, _)| s);
    let svg = svg.replacen(&format!("style=\"{style}\""), &format!("font-family=\"{}\" font-size=\"{FONT_SIZE}\"", family(&SANS, "sans-serif")), 1);
    // Pele leaves the background to the page; the raster carries it.
    let body = svg.find('>').map_or(0, |i| i + 1);
    format!("{}<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>{}", &svg[..body], css(rgb(canvas)), &svg[body..])
}

#[cfg(test)]
mod tests {
    use super::{measure, render, resolve, substitute, variables};

    #[test]
    fn substitutes_known_variables_and_falls_back_for_the_rest() {
        let known = |n: &str| (n == "--a").then(|| "#111".to_string());
        assert_eq!(substitute("fill=\"var(--a)\" stroke=\"var(--b,#fff)\" x", known), "fill=\"#111\" stroke=\"#fff\" x");
        assert_eq!(substitute("fill=\"var(--c)\"", known), "fill=\"var(--c)\"");
    }

    #[test]
    fn tones_mix_the_text_into_the_canvas_as_markdown_preview_does() {
        let vars = variables(0x262626ff, true);
        let get = |n: &str| vars.iter().find(|(k, _)| *k == n).unwrap().1.clone();
        assert_eq!((get("--pele-bg"), get("--pele-fg"), get("--pele-border")), ("#262626".into(), "#dedede".into(), "#4b4b4b".into()));
        assert_eq!(variables(0xf9f9f9ff, false)[1].1, "#000000");
    }

    #[test]
    fn draws_a_flowchart_with_its_labels_and_every_variable_resolved() {
        let svg = resolve(&render("flowchart LR\n  A[Open] --> B[Close]\n").unwrap(), 0x262626ff, true);
        assert!(svg.starts_with("<svg") && svg.contains(">Open<") && svg.contains("rx=\"6\""));
        assert!(!svg.contains("var(") && !svg.contains("=\"\"") && !svg.contains("<foreignObject"));
    }

    #[test]
    fn reports_syntax_errors_with_their_place() {
        assert_eq!(render("flowchart LR\n  A -->\n").unwrap_err(), "The diagram has a syntax error. Line 2, column 8");
        assert_eq!(render("nonsense").unwrap_err(), "This diagram type is not supported.");
        assert_eq!(render("  \n").unwrap_err(), "This diagram is empty.");
    }

    #[test]
    fn measures_wider_text_wider_and_bold_wider_still() {
        let (short, long) = (measure("15px 'SF Pro Text'", "ab"), measure("15px 'SF Pro Text'", "abcd"));
        assert!(short > 0. && long > short * 1.5);
        assert!(measure("bold 15px 'SF Pro Text'", "abcd") > long);
    }
}
