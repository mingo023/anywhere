#include <ghostty/vt.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

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
  uint8_t selected;
} PCell;

typedef struct {
  uint16_t cols, rows, cursor_x, cursor_y;
  uint8_t cursor_visible;
  uint8_t fg[3], bg[3];
  uint8_t at_bottom;
  uint8_t cursor_style, cursor_blink;
} PFrame;

typedef struct {
  GhosttyTerminal term;
  GhosttyRenderState rs;
  GhosttyRenderStateRowIterator rows;
  GhosttyRenderStateRowCells cells;
  GhosttySelectionGesture gesture;
  GhosttySelectionGestureEvent press, drag, tick, release;
  GhosttyMouseEncoder mouse;
  GhosttyMouseEvent mouse_event;
  GhosttyKeyEncoder keys;
  GhosttyKeyEvent key_event;
  bool reported;
  uint16_t reported_col, reported_row;
  bool bell;
} PTerm;

typedef struct {
  uint16_t col, row;
  double x, y;
  uint32_t cell_width, height;
} PPointer;

void pt_free(PTerm* p);

static void rgb(uint8_t out[3], GhosttyColorRgb c) { out[0] = c.r; out[1] = c.g; out[2] = c.b; }

static void on_bell(GhosttyTerminal t, void* userdata) {
  (void)t;
  ((PTerm*)userdata)->bell = true;
}

PTerm* pt_new(uint16_t cols, uint16_t rows, size_t scrollback) {
  PTerm* p = calloc(1, sizeof(PTerm));
  if (!p) return NULL;
  if (ghostty_terminal_new(NULL, &p->term, cols, rows) != GHOSTTY_SUCCESS ||
      ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &scrollback) != GHOSTTY_SUCCESS ||
      ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES, NULL) != GHOSTTY_SUCCESS ||
      ghostty_render_state_new(NULL, &p->rs) != GHOSTTY_SUCCESS ||
      ghostty_render_state_row_iterator_new(NULL, &p->rows) != GHOSTTY_SUCCESS ||
      ghostty_render_state_row_cells_new(NULL, &p->cells) != GHOSTTY_SUCCESS ||
      ghostty_selection_gesture_new(NULL, &p->gesture) != GHOSTTY_SUCCESS ||
      ghostty_selection_gesture_event_new(NULL, &p->press, GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_PRESS) != GHOSTTY_SUCCESS ||
      ghostty_selection_gesture_event_new(NULL, &p->drag, GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_DRAG) != GHOSTTY_SUCCESS ||
      ghostty_selection_gesture_event_new(NULL, &p->tick, GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_AUTOSCROLL_TICK) != GHOSTTY_SUCCESS ||
      ghostty_selection_gesture_event_new(NULL, &p->release, GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_RELEASE) != GHOSTTY_SUCCESS ||
      ghostty_mouse_encoder_new(NULL, &p->mouse) != GHOSTTY_SUCCESS ||
      ghostty_mouse_event_new(NULL, &p->mouse_event) != GHOSTTY_SUCCESS ||
      ghostty_key_encoder_new(NULL, &p->keys) != GHOSTTY_SUCCESS ||
      ghostty_key_event_new(NULL, &p->key_event) != GHOSTTY_SUCCESS) {
    pt_free(p);
    return NULL;
  }
  // GPUI already counted the clicks, so every press we pass on as a repeat does repeat.
  uint64_t time = 0, interval = UINT64_MAX;
  double distance = INFINITY;
  bool blink = true;
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK, &blink);
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_USERDATA, p);
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_BELL, (const void*)on_bell);
  ghostty_selection_gesture_event_set(p->press, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_TIME_NS, &time);
  ghostty_selection_gesture_event_set(p->press, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_INTERVAL_NS, &interval);
  ghostty_selection_gesture_event_set(p->press, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REPEAT_DISTANCE, &distance);
  return p;
}

void pt_write(PTerm* p, const uint8_t* data, size_t len) { ghostty_terminal_vt_write(p->term, data, len); }

uint8_t pt_take_bell(PTerm* p) {
  bool rang = p->bell;
  p->bell = false;
  return rang;
}

// Only ANSI 0–15 change; 16–255 stay libghostty's, and colours a program set by OSC stay too.
void pt_configure(PTerm* p, size_t scrollback, const uint8_t palette[16][3], uint8_t cursor, uint8_t blink) {
  bool b = blink;
  GhosttyTerminalCursorStyle style = (GhosttyTerminalCursorStyle)cursor;
  GhosttyColorRgb colors[256];
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT, colors);
  for (int i = 0; i < 16; i++) colors[i] = (GhosttyColorRgb){palette[i][0], palette[i][1], palette[i][2]};
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES, &scrollback);
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_COLOR_PALETTE, colors);
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE, &style);
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK, &b);
}

void pt_palette(PTerm* p, uint8_t out[16][3]) {
  GhosttyColorRgb colors[256];
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_COLOR_PALETTE, colors);
  for (int i = 0; i < 16; i++) rgb(out[i], colors[i]);
}

uint8_t pt_mode(PTerm* p, uint16_t dec) {
  GhosttyTerminalModeConfig m = {.mode = ghostty_mode_new(dec, false)};
  return ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_MODE, &m) == GHOSTTY_SUCCESS && m.value;
}

uint8_t pt_alt_screen(PTerm* p) {
  GhosttyTerminalScreen s = GHOSTTY_TERMINAL_SCREEN_PRIMARY;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &s);
  return s == GHOSTTY_TERMINAL_SCREEN_ALTERNATE;
}

uint8_t pt_mouse_tracking(PTerm* p) {
  bool on = false;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, &on);
  return on;
}

typedef struct {
  bool shift, ctrl, alt, cmd;
} PMods;

static const struct {
  const char* name;
  GhosttyKey key;
} NAMED_KEYS[] = {
    {"enter", GHOSTTY_KEY_ENTER}, {"backspace", GHOSTTY_KEY_BACKSPACE}, {"escape", GHOSTTY_KEY_ESCAPE}, {"tab", GHOSTTY_KEY_TAB},
    {"space", GHOSTTY_KEY_SPACE}, {"up", GHOSTTY_KEY_ARROW_UP}, {"down", GHOSTTY_KEY_ARROW_DOWN}, {"left", GHOSTTY_KEY_ARROW_LEFT},
    {"right", GHOSTTY_KEY_ARROW_RIGHT}, {"home", GHOSTTY_KEY_HOME}, {"end", GHOSTTY_KEY_END}, {"delete", GHOSTTY_KEY_DELETE},
    {"pageup", GHOSTTY_KEY_PAGE_UP}, {"pagedown", GHOSTTY_KEY_PAGE_DOWN}, {"f1", GHOSTTY_KEY_F1}, {"f2", GHOSTTY_KEY_F2},
    {"f3", GHOSTTY_KEY_F3}, {"f4", GHOSTTY_KEY_F4}, {"f5", GHOSTTY_KEY_F5}, {"f6", GHOSTTY_KEY_F6},
    {"f7", GHOSTTY_KEY_F7}, {"f8", GHOSTTY_KEY_F8}, {"f9", GHOSTTY_KEY_F9}, {"f10", GHOSTTY_KEY_F10},
    {"f11", GHOSTTY_KEY_F11}, {"f12", GHOSTTY_KEY_F12},
};

uint8_t pt_kitty_keyboard(PTerm* p) {
  GhosttyKittyKeyFlags flags = GHOSTTY_KITTY_KEY_DISABLED;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS, &flags);
  return flags != GHOSTTY_KITTY_KEY_DISABLED;
}

// Keys without a name here go by their text, which also covers layouts whose keys have no US name.
size_t pt_key(PTerm* p, const char* name, size_t name_len, PMods m, const char* text, size_t text_len, uint32_t unshifted, uint8_t* out, size_t cap) {
  GhosttyKey key = GHOSTTY_KEY_UNIDENTIFIED;
  for (size_t i = 0; i < sizeof NAMED_KEYS / sizeof *NAMED_KEYS; i++)
    if (strlen(NAMED_KEYS[i].name) == name_len && !memcmp(NAMED_KEYS[i].name, name, name_len)) key = NAMED_KEYS[i].key;
  if (key == GHOSTTY_KEY_UNIDENTIFIED && !text_len) return 0;
  GhosttyMods mods = (m.shift ? GHOSTTY_MODS_SHIFT : 0) | (m.ctrl ? GHOSTTY_MODS_CTRL : 0) | (m.alt ? GHOSTTY_MODS_ALT : 0) | (m.cmd ? GHOSTTY_MODS_SUPER : 0);
  ghostty_key_encoder_setopt_from_terminal(p->keys, p->term);
  // setopt_from_terminal resets option-as-alt, so it's set again on every key.
  GhosttyOptionAsAlt option_as_alt = GHOSTTY_OPTION_AS_ALT_TRUE;
  ghostty_key_encoder_setopt(p->keys, GHOSTTY_KEY_ENCODER_OPT_MACOS_OPTION_AS_ALT, &option_as_alt);
  ghostty_key_event_set_action(p->key_event, GHOSTTY_KEY_ACTION_PRESS);
  ghostty_key_event_set_key(p->key_event, key);
  ghostty_key_event_set_mods(p->key_event, mods);
  ghostty_key_event_set_utf8(p->key_event, text, text_len);
  ghostty_key_event_set_unshifted_codepoint(p->key_event, unshifted);
  size_t n = 0;
  return ghostty_key_encoder_encode(p->keys, p->key_event, (char*)out, cap, &n) == GHOSTTY_SUCCESS ? n : 0;
}

// Our cells aren't whole pixels and the encoder's are, so it gets the cell's centre on a whole-pixel grid rather than the raw position.
size_t pt_mouse(PTerm* p, const PPointer* at, uint8_t action, uint8_t button, uint8_t ctrl, uint8_t alt, uint8_t* out, size_t cap) {
  if (!pt_mouse_tracking(p)) {
    p->reported = false;
    return 0;
  }
  // The encoder's own motion dedupe forgets its cell whenever it's reconfigured, which is every event here.
  if (action == GHOSTTY_MOUSE_ACTION_MOTION && p->reported && p->reported_col == at->col && p->reported_row == at->row) return 0;
  uint16_t cols = 1, rows = 1;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_COLS, &cols);
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_ROWS, &rows);
  uint32_t cw = at->cell_width, ch = at->height > rows ? at->height / rows : 1;
  GhosttyMouseEncoderSize size = GHOSTTY_INIT_SIZED(GhosttyMouseEncoderSize);
  size.screen_width = cols * cw;
  size.screen_height = rows * ch;
  size.cell_width = cw;
  size.cell_height = ch;
  ghostty_mouse_encoder_setopt_from_terminal(p->mouse, p->term);
  ghostty_mouse_encoder_setopt(p->mouse, GHOSTTY_MOUSE_ENCODER_OPT_SIZE, &size);
  ghostty_mouse_event_set_action(p->mouse_event, (GhosttyMouseAction)action);
  if (button)
    ghostty_mouse_event_set_button(p->mouse_event, (GhosttyMouseButton)button);
  else
    ghostty_mouse_event_clear_button(p->mouse_event);
  ghostty_mouse_event_set_mods(p->mouse_event, (ctrl ? GHOSTTY_MODS_CTRL : 0) | (alt ? GHOSTTY_MODS_ALT : 0));
  ghostty_mouse_event_set_position(p->mouse_event, (GhosttyMousePosition){(at->col + 0.5f) * cw, (at->row + 0.5f) * ch});
  size_t n = 0;
  if (ghostty_mouse_encoder_encode(p->mouse, p->mouse_event, (char*)out, cap, &n) != GHOSTTY_SUCCESS) return 0;
  if (n) {
    p->reported = true;
    p->reported_col = at->col;
    p->reported_row = at->row;
  }
  return n;
}

void pt_scroll(PTerm* p, int tag, intptr_t delta) {
  GhosttyTerminalScrollViewport s = {.tag = (GhosttyTerminalScrollViewportTag)tag, .value = {.delta = delta}};
  ghostty_terminal_scroll_viewport(p->term, s);
}

size_t pt_scrollback_rows(PTerm* p) {
  size_t n = 0;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_SCROLLBACK_ROWS, &n);
  return n;
}

static bool ref_at(PTerm* p, const PPointer* at, GhosttyGridRef* ref) {
  *ref = GHOSTTY_INIT_SIZED(GhosttyGridRef);
  GhosttyPoint pt = {.tag = GHOSTTY_POINT_TAG_VIEWPORT, .value = {.coordinate = {.x = at->col, .y = at->row}}};
  return ghostty_terminal_grid_ref(p->term, pt, ref) == GHOSTTY_SUCCESS;
}

static void place(PTerm* p, GhosttySelectionGestureEvent ev, const PPointer* at) {
  uint16_t cols = 1;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_COLS, &cols);
  GhosttySurfacePosition pos = {.x = at->x, .y = at->y};
  GhosttySelectionGestureGeometry g = {.columns = cols, .cell_width = at->cell_width, .padding_left = 0, .screen_height = at->height};
  ghostty_selection_gesture_event_set(ev, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION, &pos);
  ghostty_selection_gesture_event_set(ev, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_GEOMETRY, &g);
}

static void select_from(PTerm* p, GhosttySelectionGestureEvent ev, bool clear_on_none) {
  GhosttySelection s = GHOSTTY_INIT_SIZED(GhosttySelection);
  if (ghostty_selection_gesture_event(p->gesture, p->term, ev, &s) == GHOSTTY_SUCCESS)
    ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SELECTION, &s);
  else if (clear_on_none)
    ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SELECTION, NULL);
}

static uint8_t autoscroll(PTerm* p) {
  GhosttySelectionGestureAutoscroll a = GHOSTTY_SELECTION_GESTURE_AUTOSCROLL_NONE;
  ghostty_selection_gesture_get(p->gesture, p->term, GHOSTTY_SELECTION_GESTURE_DATA_AUTOSCROLL, &a);
  return (uint8_t)a;
}

void pt_press(PTerm* p, const PPointer* at, uint8_t clicks) {
  if (clicks <= 1) ghostty_selection_gesture_reset(p->gesture, p->term);
  GhosttyGridRef ref;
  if (!ref_at(p, at, &ref)) return;
  GhosttySurfacePosition pos = {.x = at->x, .y = at->y};
  ghostty_selection_gesture_event_set(p->press, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &ref);
  ghostty_selection_gesture_event_set(p->press, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_POSITION, &pos);
  select_from(p, p->press, true);
}

uint8_t pt_drag(PTerm* p, const PPointer* at) {
  GhosttyGridRef ref;
  if (!ref_at(p, at, &ref)) return 0;
  ghostty_selection_gesture_event_set(p->drag, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_REF, &ref);
  place(p, p->drag, at);
  select_from(p, p->drag, true);
  return autoscroll(p);
}

uint8_t pt_tick(PTerm* p, const PPointer* at) {
  GhosttyPointCoordinate vp = {.x = at->col, .y = at->row};
  ghostty_selection_gesture_event_set(p->tick, GHOSTTY_SELECTION_GESTURE_EVENT_OPT_VIEWPORT, &vp);
  place(p, p->tick, at);
  select_from(p, p->tick, false);
  return autoscroll(p);
}

void pt_release(PTerm* p) { ghostty_selection_gesture_event(p->gesture, p->term, p->release, NULL); }

uint8_t pt_extend(PTerm* p, const PPointer* at) {
  GhosttySelection s = GHOSTTY_INIT_SIZED(GhosttySelection);
  GhosttyGridRef ref;
  if (ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_SELECTION, &s) != GHOSTTY_SUCCESS || !ref_at(p, at, &ref)) return 0;
  s.end = ref;
  ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SELECTION, &s);
  return 1;
}

void pt_select_all(PTerm* p) {
  GhosttySelection s = GHOSTTY_INIT_SIZED(GhosttySelection);
  if (ghostty_terminal_select_all(p->term, &s) == GHOSTTY_SUCCESS) ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SELECTION, &s);
}

void pt_select_none(PTerm* p) { ghostty_terminal_set(p->term, GHOSTTY_TERMINAL_OPT_SELECTION, NULL); }

uint8_t* pt_selection_text(PTerm* p, size_t* len) {
  GhosttyTerminalSelectionFormatOptions o = GHOSTTY_INIT_SIZED(GhosttyTerminalSelectionFormatOptions);
  o.emit = GHOSTTY_FORMATTER_FORMAT_PLAIN;
  o.unwrap = true;
  o.trim = true;
  uint8_t* buf = NULL;
  *len = 0;
  if (ghostty_terminal_selection_format_alloc(p->term, NULL, o, &buf, len) != GHOSTTY_SUCCESS) return NULL;
  return buf;
}

void pt_text_free(uint8_t* buf, size_t len) { ghostty_free(NULL, buf, len); }

void pt_resize(PTerm* p, uint16_t cols, uint16_t rows) { ghostty_terminal_resize(p->term, cols, rows, 0, 0); }

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
  f->cursor_style = cur.visual_style;
  f->cursor_blink = cur.blinking;
  bool at_bottom = true;
  ghostty_terminal_get(p->term, GHOSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE, &at_bottom);
  f->at_bottom = at_bottom;

  size_t n = 0;
  ghostty_render_state_get(p->rs, GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &p->rows);
  while (ghostty_render_state_row_iterator_next(p->rows)) {
    ghostty_render_state_row_get(p->rows, GHOSTTY_RENDER_STATE_ROW_DATA_CELLS, &p->cells);
    GhosttyRenderStateRowSelection sel = GHOSTTY_INIT_SIZED(GhosttyRenderStateRowSelection);
    bool any = ghostty_render_state_row_get(p->rows, GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION, &sel) == GHOSTTY_SUCCESS;
    for (uint16_t x = 0; ghostty_render_state_row_cells_next(p->cells) && n < cap; x++) {
      PCell* c = &out[n++];
      *c = (PCell){0};
      c->selected = any && x >= sel.start_x && x <= sel.end_x;
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
  ghostty_key_event_free(p->key_event);
  ghostty_key_encoder_free(p->keys);
  ghostty_mouse_event_free(p->mouse_event);
  ghostty_mouse_encoder_free(p->mouse);
  ghostty_selection_gesture_event_free(p->release);
  ghostty_selection_gesture_event_free(p->tick);
  ghostty_selection_gesture_event_free(p->drag);
  ghostty_selection_gesture_event_free(p->press);
  if (p->gesture) ghostty_selection_gesture_free(p->gesture, p->term);
  if (p->cells) ghostty_render_state_row_cells_free(p->cells);
  if (p->rows) ghostty_render_state_row_iterator_free(p->rows);
  if (p->rs) ghostty_render_state_free(p->rs);
  if (p->term) ghostty_terminal_free(p->term);
  free(p);
}
