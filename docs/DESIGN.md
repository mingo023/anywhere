# Design rules

Interaction rules every screen follows. Check new UI against them before it ships.

## Popovers and menus

- **A second press on a trigger shuts its popover.** Clicking the button that opened a menu, dropdown or picker closes it; it never closes and reopens.
- **A press outside a popover shuts it**, and a press on another trigger opens that one in the same press.
- **Rows inside a popover act on click** and shut it.

In the desktop app, build the trigger with `ui::popover_trigger(button, open, on_open)` and shut the popover with `on_mouse_down_out`. A plain `on_click` toggle breaks the first rule: the press runs the popover's `on_mouse_down_out` first, which shuts it, and the click that follows reopens it.
