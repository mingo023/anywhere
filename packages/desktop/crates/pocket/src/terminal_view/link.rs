use term::Cell;

/// The link under cell (`row`, `col`) of a `cols`-wide grid, read across the rows a long line wraps onto.
pub fn link_at(cells: &[Cell], cols: u16, row: u16, col: u16) -> Option<String> {
    let rows: Vec<&[Cell]> = cells.chunks(cols.max(1) as usize).collect();
    let row = row as usize;
    rows.get(row)?;
    // Wrapping fills a row to its last cell, so a row ending blank ends its line.
    let runs_on = |r: usize| rows[r].last().is_some_and(|c| c.ch() != ' ');
    let first = (0..row).rev().take_while(|&r| runs_on(r)).last().unwrap_or(row);
    let last = (row..rows.len()).find(|&r| !runs_on(r)).unwrap_or(rows.len() - 1);
    let line: String = rows[first..=last].iter().flat_map(|r| r.iter()).map(Cell::ch).collect();
    web::link_at(&line, (row - first) * cols as usize + col as usize)
}

#[cfg(test)]
mod tests {
    use super::link_at;
    use term::Term;

    fn link(output: &str, row: u16, col: u16) -> Option<String> {
        let mut t = Term::new(20, 4);
        t.write(output.as_bytes());
        let (f, cells) = t.frame();
        link_at(cells, f.cols, row, col)
    }

    #[test]
    fn a_url_wrapped_onto_the_next_row_is_one_link() {
        let out = "go: https://example.com/abc/def";
        assert_eq!(link(out, 0, 6).as_deref(), Some("https://example.com/abc/def"));
        assert_eq!(link(out, 1, 3).as_deref(), Some("https://example.com/abc/def"));
    }

    #[test]
    fn a_row_ending_blank_does_not_run_into_the_next() {
        assert_eq!(link("http://a.io\r\nmore", 1, 1), None);
        assert_eq!(link("http://a.io\r\nmore", 0, 2).as_deref(), Some("http://a.io"));
    }

    #[test]
    fn wide_characters_keep_the_link_under_the_pointer() {
        assert_eq!(link("世界 localhost:3000", 0, 9).as_deref(), Some("http://localhost:3000"));
    }
}
