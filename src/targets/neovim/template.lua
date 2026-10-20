-- Palette: {{ theme }} (generated; edits are overwritten on the next switch)
-- Loaded from your init.lua. Reloads itself in running Neovims after a switch.
local c = {
  bg = "{{ background }}", bg_dark = "{{ dark_background }}", bg_darker = "{{ darker_background }}",
  bg_light = "{{ lighter_background }}", sel = "{{ selection }}", muted = "{{ muted }}",
  fg = "{{ foreground }}", fg_dark = "{{ dark_foreground }}", fg_light = "{{ light_foreground }}",
  fg_bright = "{{ bright_foreground }}", accent = "{{ accent }}",
  red = "{{ red }}", orange = "{{ orange }}", yellow = "{{ yellow }}", green = "{{ green }}",
  cyan = "{{ cyan }}", blue = "{{ blue }}", magenta = "{{ magenta }}", brown = "{{ brown }}",
}
local ansi = { {% for colour in ansi %}"{{ colour }}"{% if not loop.last %}, {% endif %}{% endfor %} }

vim.cmd("highlight clear")
if vim.fn.exists("syntax_on") == 1 then vim.cmd("syntax reset") end
vim.o.termguicolors = true
vim.o.background = "{{ mode }}"
vim.g.colors_name = "palette"
for i, colour in ipairs(ansi) do vim.g["terminal_color_" .. (i - 1)] = colour end

local function hl(group, spec) vim.api.nvim_set_hl(0, group, spec) end

-- Editor
hl("Normal", { fg = c.fg, bg = c.bg })
hl("NormalFloat", { fg = c.fg, bg = c.bg_dark })
hl("FloatBorder", { fg = c.muted, bg = c.bg_dark })
hl("FloatTitle", { fg = c.accent, bg = c.bg_dark, bold = true })
hl("NormalNC", { link = "Normal" })
hl("Cursor", { fg = c.bg, bg = c.fg_bright })
hl("CursorLine", { bg = c.bg_light })
hl("CursorColumn", { link = "CursorLine" })
hl("ColorColumn", { bg = c.bg_dark })
hl("CursorLineNr", { fg = c.accent, bold = true })
hl("LineNr", { fg = c.muted })
hl("SignColumn", { fg = c.muted, bg = c.bg })
hl("FoldColumn", { fg = c.muted, bg = c.bg })
hl("Folded", { fg = c.fg_dark, bg = c.bg_dark })
hl("VertSplit", { fg = c.muted })
hl("WinSeparator", { fg = c.muted })
hl("StatusLine", { fg = c.fg, bg = c.bg_light })
hl("StatusLineNC", { fg = c.fg_dark, bg = c.bg_dark })
hl("TabLine", { fg = c.fg_dark, bg = c.bg_dark })
hl("TabLineFill", { bg = c.bg_darker })
hl("TabLineSel", { fg = c.fg_bright, bg = c.bg, bold = true })
hl("WinBar", { fg = c.fg, bold = true })
hl("WinBarNC", { fg = c.fg_dark })
hl("Visual", { bg = c.sel })
hl("Search", { fg = c.bg, bg = c.yellow })
hl("IncSearch", { fg = c.bg, bg = c.orange })
hl("CurSearch", { link = "IncSearch" })
hl("Substitute", { fg = c.bg, bg = c.red })
hl("MatchParen", { fg = c.orange, bold = true, underline = true })
hl("Pmenu", { fg = c.fg, bg = c.bg_dark })
hl("PmenuSel", { fg = c.fg_bright, bg = c.sel })
hl("PmenuSbar", { bg = c.bg_light })
hl("PmenuThumb", { bg = c.muted })
hl("NonText", { fg = c.muted })
hl("Whitespace", { fg = c.muted })
hl("SpecialKey", { fg = c.muted })
hl("EndOfBuffer", { fg = c.bg })
hl("Directory", { fg = c.blue })
hl("Title", { fg = c.accent, bold = true })
hl("ErrorMsg", { fg = c.red })
hl("WarningMsg", { fg = c.yellow })
hl("MoreMsg", { fg = c.green })
hl("ModeMsg", { fg = c.fg, bold = true })
hl("Question", { fg = c.cyan })
hl("QuickFixLine", { bg = c.sel, bold = true })
hl("WildMenu", { link = "PmenuSel" })
hl("Conceal", { fg = c.muted })
hl("SpellBad", { sp = c.red, undercurl = true })
hl("SpellCap", { sp = c.yellow, undercurl = true })
hl("SpellLocal", { sp = c.cyan, undercurl = true })
hl("SpellRare", { sp = c.magenta, undercurl = true })

-- Syntax (Tree-sitter and LSP groups link to these by default)
hl("Comment", { fg = c.fg_dark, italic = true })
hl("Constant", { fg = c.orange })
hl("String", { fg = c.green })
hl("Character", { fg = c.green })
hl("Number", { fg = c.orange })
hl("Boolean", { fg = c.orange })
hl("Float", { fg = c.orange })
hl("Identifier", { fg = c.fg })
hl("Function", { fg = c.blue })
hl("Statement", { fg = c.magenta })
hl("Keyword", { fg = c.magenta })
hl("Conditional", { fg = c.magenta })
hl("Repeat", { fg = c.magenta })
hl("Label", { fg = c.cyan })
hl("Operator", { fg = c.cyan })
hl("Exception", { fg = c.red })
hl("PreProc", { fg = c.cyan })
hl("Include", { fg = c.magenta })
hl("Type", { fg = c.yellow })
hl("StorageClass", { fg = c.yellow })
hl("Structure", { fg = c.yellow })
hl("Special", { fg = c.cyan })
hl("Delimiter", { fg = c.fg_dark })
hl("Tag", { fg = c.red })
hl("Underlined", { underline = true })
hl("Todo", { fg = c.bg, bg = c.accent, bold = true })
hl("Error", { fg = c.red })
hl("@variable", { fg = c.fg })
hl("@property", { fg = c.cyan })
hl("@tag", { fg = c.red })
hl("@tag.attribute", { fg = c.yellow })

-- Diagnostics, diffs, git signs
hl("DiagnosticError", { fg = c.red })
hl("DiagnosticWarn", { fg = c.yellow })
hl("DiagnosticInfo", { fg = c.blue })
hl("DiagnosticHint", { fg = c.cyan })
hl("DiagnosticOk", { fg = c.green })
hl("DiagnosticUnderlineError", { sp = c.red, undercurl = true })
hl("DiagnosticUnderlineWarn", { sp = c.yellow, undercurl = true })
hl("DiagnosticUnderlineInfo", { sp = c.blue, undercurl = true })
hl("DiagnosticUnderlineHint", { sp = c.cyan, undercurl = true })
hl("DiffAdd", { fg = c.green, bg = c.bg_dark })
hl("DiffChange", { fg = c.yellow, bg = c.bg_dark })
hl("DiffDelete", { fg = c.red, bg = c.bg_dark })
hl("DiffText", { fg = c.bg, bg = c.yellow })
hl("Added", { fg = c.green })
hl("Changed", { fg = c.yellow })
hl("Removed", { fg = c.red })

-- Live reload: watch Palette's folder and re-run this file when `current`
-- is swapped. Set up once per Neovim.
local uv = vim.uv or vim.loop
if uv and not _G.palette_watch then
  local file = debug.getinfo(1, "S").source:sub(2)
  local root = vim.fn.fnamemodify(file, ":h:h")
  local handle = uv.new_fs_event()
  local timer = uv.new_timer()
  if handle and timer then
    _G.palette_watch = { handle = handle, timer = timer }
    handle:start(root, {}, function(err, name)
      if err or not (name and name:match("current$")) then return end
      timer:stop()
      timer:start(100, 0, vim.schedule_wrap(function()
        pcall(dofile, file)
        vim.cmd("redraw!")
      end))
    end)
  end
end
