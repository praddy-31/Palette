-- Palette: {{ theme }} (generated; edits are overwritten on the next switch)
-- Loaded from your wezterm.lua by the line `palette init` added:
--   config.colors = dofile(".../palette/current/wezterm.lua")
return {
  foreground = "{{ foreground }}",
  background = "{{ background }}",
  cursor_bg = "{{ bright_foreground }}",
  cursor_fg = "{{ background }}",
  cursor_border = "{{ bright_foreground }}",
  selection_fg = "{{ bright_foreground }}",
  selection_bg = "{{ selection }}",
  scrollbar_thumb = "{{ selection }}",
  split = "{{ muted }}",
  visual_bell = "{{ lighter_background }}",
  compose_cursor = "{{ orange }}",
  ansi = { {% for c in ansi[:8] %}"{{ c }}"{% if not loop.last %}, {% endif %}{% endfor %} },
  brights = { {% for c in ansi[8:] %}"{{ c }}"{% if not loop.last %}, {% endif %}{% endfor %} },
  tab_bar = {
    background = "{{ darker_background }}",
    inactive_tab_edge = "{{ muted }}",
    active_tab = { bg_color = "{{ accent }}", fg_color = "{{ accent | ink(darker_background, bright_foreground) }}" },
    inactive_tab = { bg_color = "{{ dark_background }}", fg_color = "{{ dark_foreground }}" },
    inactive_tab_hover = { bg_color = "{{ lighter_background }}", fg_color = "{{ foreground }}" },
    new_tab = { bg_color = "{{ darker_background }}", fg_color = "{{ dark_foreground }}" },
    new_tab_hover = { bg_color = "{{ lighter_background }}", fg_color = "{{ foreground }}" },
  },
}
