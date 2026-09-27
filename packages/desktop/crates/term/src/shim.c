#include <ghostty/vt.h>
#include <stdlib.h>

typedef struct {
  uint32_t cp;
  uint8_t fg[3];
  uint8_t bg[3];
  uint8_t has_fg;
  uint8_t has_bg;
  uint8_t bold;
  uint8_t italic;
  uint8_t underline;
  uint8_t inverse;
  uint8_t wide;
} PCell;

typedef struct {
  uint16_t cols, rows, cursor_x, cursor_y;
  uint8_t cursor_visible;
  uint8_t fg[3], bg[3];
} PFrame;

typedef struct {
  GhosttyTerminal term;
  GhosttyRenderState rs;
  GhosttyRenderStateRowIterator rows;
  GhosttyRenderStateRowCells cells;
} PTerm;

void pt_free(PTerm* p);

PTerm* pt_new(uint16_t cols, uint16_t rows) {
  PTerm* p = calloc(1, sizeof(PTerm));
  if (!p) return NULL;
  if (ghostty_terminal_new(NULL, &p->term, cols, rows) != GHOSTTY_SUCCESS ||
      ghostty_render_state_new(NULL, &p->rs) != GHOSTTY_SUCCESS ||
      ghostty_render_state_row_iterator_new(NULL, &p->rows) != GHOSTTY_SUCCESS ||
      ghostty_render_state_row_cells_new(NULL, &p->cells) != GHOSTTY_SUCCESS) {
    pt_free(p);
    return NULL;
  }
  return p;
}

void pt_write(PTerm* p, const uint8_t* data, size_t len) { ghostty_terminal_vt_write(p->term, data, len); }

uint8_t pt_app_cursor(PTerm* p) {
  GhosttyTerminalModeConfig m = {.mode = GHOSTTY_MODE_DECCKM};
  return ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_MODE, &m) == GHOSTTY_SUCCESS && m.value;
}

void pt_resize(PTerm* p, uint16_t cols, uint16_t rows) { ghostty_terminal_resize(p->term, cols, rows, 0, 0); }

static void rgb(uint8_t out[3], GhosttyColorRgb c) { out[0] = c.r; out[1] = c.g; out[2] = c.b; }

size_t pt_frame(PTerm* p, PFrame* f, PCell* out, size_t cap) {
  ghostty_render_state_update(p->rs, p->term);
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_COLS, &f->cols);
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_ROWS, &f->rows);
  GhosttyRenderStateColors colors = GHOSTTY_INIT_SIZED(GhosttyRenderStateColors);
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_COLORS, &colors);
  rgb(f->fg, colors.foreground);
  rgb(f->bg, colors.background);
  GhosttyRenderStateCursor cur = GHOSTTY_INIT_SIZED(GhosttyRenderStateCursor);
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_CURSOR, &cur);
  f->cursor_visible = cur.visible && cur.viewport_has_value;
  f->cursor_x = cur.viewport_x;
  f->cursor_y = cur.viewport_y;

  size_t n = 0;
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &p->rows);
  while (ghostty_render_state_row_iterator_next(p->rows)) {
    ghostty_render_state_row_get(p->rows, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &p->cells);
    while (ghostty_render_state_row_cells_next(p->cells) && n < cap) {
      PCell* c = &out[n++];
      *c = (PCell){0};
      uint32_t glen = 0;
      ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN, &glen);
      if (glen > 16) {
        c->cp = 0xFFFD;
      } else if (glen > 0) {
        uint32_t cps[16];
        ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, cps);
        c->cp = cps[0];
      }
      GhosttyCell raw;
      ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, &raw);
      GhosttyCellWide wide = GHOSTTY_CELL_WIDE_NARROW;
      ghostty_cell_get(raw, GHOSTTY_CELL_DATA_WIDE, &wide);
      c->wide = (uint8_t)wide;
      GhosttyColorRgb col;
      if (ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, &col) == GHOSTTY_SUCCESS) {
        rgb(c->fg, col);
        c->has_fg = 1;
      }
      if (ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, &col) == GHOSTTY_SUCCESS) {
        rgb(c->bg, col);
        c->has_bg = 1;
      }
      GhosttyStyle st = GHOSTTY_INIT_SIZED(GhosttyStyle);
      ghostty_render_state_row_cells_get(p->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &st);
      c->bold = st.bold;
      c->italic = st.italic;
      c->underline = st.underline != 0;
      c->inverse = st.inverse;
    }
  }
  ghostty_render_state_clean(p->rs);
  return n;
}

void pt_free(PTerm* p) {
  if (p->cells) ghostty_render_state_row_cells_free(p->cells);
  if (p->rows) ghostty_render_state_row_iterator_free(p->rows);
  if (p->rs) ghostty_render_state_free(p->rs);
  if (p->term) ghostty_terminal_free(p->term);
  free(p);
}
