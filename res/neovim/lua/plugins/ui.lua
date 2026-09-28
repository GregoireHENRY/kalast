return {
  -- ── Colourscheme ─────────────────────────────────────────────────────────
  {
    "catppuccin/nvim",
    name = "catppuccin",
    priority = 1000,   -- load before everything else
    config = function()
      require("catppuccin").setup({
        flavour = "mocha",
        transparent_background = false,
        term_colors = true,
        styles = {
          comments = { "italic" },
          conditionals = { "italic" },
        },
        -- LSP + treesitter highlighting are always-on core groups in current
        -- catppuccin; the underline styles moved here out of integrations.
        lsp_styles = {
          underlines = {
            errors      = { "undercurl" },
            warnings    = { "undercurl" },
            hints       = { "undercurl" },
            information = { "undercurl" },
          },
        },
        integrations = {
          blink_cmp = true,
          gitsigns = true,
          which_key = true,
          mason = true,
          snacks = { enabled = true, indent_scope_color = "lavender" },
          treesitter_context = true,
        },
      })
      vim.cmd.colorscheme("catppuccin")
    end,
  },

  -- ── Statusline ───────────────────────────────────────────────────────────
  {
    "nvim-lualine/lualine.nvim",
    dependencies = { "nvim-tree/nvim-web-devicons" },
    event = "VeryLazy",
    opts = {
      options = {
        theme = "catppuccin-mocha",  -- file is lualine/themes/catppuccin-mocha.lua
        globalstatus = true,
        section_separators = { left = "", right = "" },
        component_separators = { left = "", right = "" },
      },
      sections = {
        lualine_c = { { "filename", path = 1 } },
        lualine_x = { "diagnostics", "encoding", "filetype" },
      },
    },
  },

  -- ── Which-key: press <leader> and see what's available ───────────────────
  {
    "folke/which-key.nvim",
    event = "VeryLazy",
    opts = { preset = "helix" },
  },

  -- ── snacks.nvim: indent guides, pickers, notifications and more ──────────
  -- One plugin in place of indent-blankline and telescope; the pickers' keys
  -- are in editor.lua.
  {
    "folke/snacks.nvim",
    priority = 900,
    lazy = false,
    opts = {
      bigfile = { enabled = true },      -- don't choke on huge files
      indent = { enabled = true },       -- guides, and the current scope's
      input = { enabled = true },        -- vim.ui.input in a float (LSP rename...)
      notifier = { enabled = true },
      picker = { enabled = true },
      quickfile = { enabled = true },
      scope = { enabled = true },
      statuscolumn = { enabled = true },
      words = { enabled = true },        -- highlight other refs to word under cursor
    },
  },
}
