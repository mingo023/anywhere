use crate::themes::color;

/// Pixel art that swims across the chat behind its messages, `count` copies of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    /// Rows of cells, left to right, each `0xRRGGBBAA` or clear.
    pub cells: Vec<Vec<Option<u32>>>,
    pub pixel: f32,
    pub count: usize,
    /// Points a second.
    pub speed: f32,
}

/// Where a copy of a sprite sits: its top-left corner, and whether it faces left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub faces_left: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub sprites: Vec<Sprite>,
}

impl Sprite {
    pub fn size(&self) -> (f32, f32) {
        let columns = self.cells.iter().map(Vec::len).max().unwrap_or(0);
        (columns as f32 * self.pixel, self.cells.len() as f32 * self.pixel)
    }

    /// Where copy `i` sits at `t` seconds in a `width` by `height` area.
    pub fn place(&self, i: usize, t: f32, width: f32, height: f32) -> Placement {
        let (w, h) = self.size();
        let spread = (i as f32 * 0.618_034).fract();
        let pace = self.speed * (0.75 + 0.5 * (i as f32 * 0.381_966).fract());
        let span = width + w;
        let run = (spread * span + pace * t).rem_euclid(span);
        let left = i % 2 == 1;
        let x = if left { width - run } else { run - w };
        let room = (height - h).max(0.);
        let y = ((0.1 + 0.8 * spread) * room + (t * 1.3 + i as f32).sin() * 2. * self.pixel).clamp(0., room);
        let snap = |v: f32| (v / self.pixel).floor() * self.pixel;
        Placement { x: snap(x), y: snap(y), faces_left: left }
    }
}

/// A scene's most sprites, and a sprite's most rows and columns, which bound what one frame paints.
const MOST_SPRITES: usize = 8;
const MOST_CELLS: usize = 32;

pub(crate) fn parse(value: &serde_json::Value) -> Result<Option<Scene>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let sprites = value["sprites"].as_array().filter(|s| s.len() <= MOST_SPRITES).ok_or(format!("\"scene.sprites\" must be a list of at most {MOST_SPRITES}"))?;
    let sprites = sprites.iter().enumerate().map(|(i, s)| sprite(&format!("scene.sprites[{i}]"), s)).collect::<Result<_, _>>()?;
    Ok(Some(Scene { sprites }))
}

fn sprite(at: &str, s: &serde_json::Value) -> Result<Sprite, String> {
    let number = |key: &str, range: std::ops::RangeInclusive<f64>, default: f64| match &s[key] {
        serde_json::Value::Null => Ok(default),
        v => v.as_f64().filter(|n| range.contains(n)).ok_or(format!("\"{at}.{key}\" must be a number from {} to {}", range.start(), range.end())),
    };
    let colors = s["colors"].as_object().ok_or(format!("\"{at}.colors\" must map each art character to a color"))?;
    let rows = s["art"].as_array().filter(|rows| (1..=MOST_CELLS).contains(&rows.len())).ok_or(format!("\"{at}.art\" must be a list of 1 to {MOST_CELLS} rows"))?;
    let cells = rows
        .iter()
        .map(|row| {
            let row = row.as_str().filter(|row| row.chars().count() <= MOST_CELLS).ok_or(format!("\"{at}.art\" rows must be strings of at most {MOST_CELLS} characters"))?;
            row.chars()
                .map(|c| match c {
                    '.' | ' ' => Ok(None),
                    c => colors.get(&c.to_string()).map(|v| color(&format!("{at}.colors.{c}"), v).map(Some)).unwrap_or(Err(format!("\"{at}.art\" uses '{c}', which \"colors\" doesn't name"))),
                })
                .collect()
        })
        .collect::<Result<_, String>>()?;
    Ok(Sprite { cells, pixel: number("pixel", 1. ..=16., 4.)? as f32, count: number("count", 0. ..=24., 1.)? as usize, speed: number("speed", 0. ..=400., 30.)? as f32 })
}

#[cfg(test)]
mod tests {
    use super::{MOST_CELLS, Sprite, parse};

    fn fish() -> Sprite {
        parse(&serde_json::json!({ "sprites": [{ "art": [".ab", "abb"], "colors": { "a": "#ff0000", "b": "#00ff0080" }, "pixel": 4, "count": 3, "speed": 40 }] })).unwrap().unwrap().sprites.remove(0)
    }

    #[test]
    fn a_sprite_s_art_reads_as_cells_of_its_colors() {
        let fish = fish();
        assert_eq!(fish.cells, [vec![None, Some(0xff0000ff), Some(0x00ff0080)], vec![Some(0xff0000ff), Some(0x00ff0080), Some(0x00ff0080)]]);
        assert_eq!((fish.size(), fish.count), ((12., 8.), 3));
    }

    #[test]
    fn a_scene_that_can_t_be_drawn_says_what_to_fix() {
        let bad = |s: serde_json::Value| parse(&serde_json::json!({ "sprites": [s] })).unwrap_err();
        assert!(bad(serde_json::json!({ "art": ["ax"], "colors": { "a": "#ff0000" } })).contains("'x'"));
        assert!(bad(serde_json::json!({ "art": ["a"], "colors": { "a": "red" } })).contains("colors.a"));
        assert!(bad(serde_json::json!({ "art": ["a"], "colors": { "a": "#ff0000" }, "pixel": 0 })).contains("pixel"));
        assert!(bad(serde_json::json!({ "art": [], "colors": {} })).contains("art"));
        assert!(bad(serde_json::json!({ "art": ["a".repeat(MOST_CELLS + 1)], "colors": { "a": "#ff0000" } })).contains("art"));
        assert!(parse(&serde_json::json!({ "sprites": vec![serde_json::json!({ "art": ["a"], "colors": { "a": "#ff0000" } }); 9] })).unwrap_err().contains("at most 8"));
        assert_eq!(parse(&serde_json::Value::Null), Ok(None));
    }

    #[test]
    fn copies_swim_their_own_way_inside_the_area_on_the_pixel_grid() {
        let fish = fish();
        let (width, height) = (800., 600.);
        for i in 0..fish.count {
            for step in 0..400 {
                let t = step as f32 * 0.25;
                let super::Placement { x, y, faces_left: left } = fish.place(i, t, width, height);
                assert!((-12. ..=width).contains(&x) && (0. ..=height - 8.).contains(&y), "copy {i} at {t}s is at {x},{y}");
                assert_eq!((x % 4., y % 4., left), (0., 0., i % 2 == 1));
            }
        }
        let x = |i, t| fish.place(i, t, width, height).x;
        let (a, b) = ((x(0, 1.), x(0, 1.5)), (x(1, 1.), x(1, 1.5)));
        assert!(a.1 > a.0 && b.1 < b.0, "rightwards {a:?}, leftwards {b:?}");
        assert_ne!(fish.place(0, 0., width, height).y, fish.place(2, 0., width, height).y, "copies keep their own lanes");
    }
}
