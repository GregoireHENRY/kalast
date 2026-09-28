return {
  -- ── Pickers: snacks.nvim's (files, grep, buffers...), in place of telescope
  {
    "folke/snacks.nvim",
    keys = {
      { "<leader>ff", function() Snacks.picker.files({ hidden = true }) end, desc = "Find files" },
      { "<leader>fg", function() Snacks.picker.grep() end,          desc = "Grep in project" },
      { "<leader>fb", function() Snacks.picker.buffers() end,       desc = "Buffers" },
      { "<leader>fh", function() Snacks.picker.help() end,          desc = "Help tags" },
      { "<leader>fr", function() Snacks.picker.recent() end,        desc = "Recent files" },
      { "<leader>fs", function() Snacks.picker.lsp_symbols() end,   desc = "Document symbols" },
      { "<leader>fd", function() Snacks.picker.diagnostics() end,   desc = "Diagnostics" },
      { "<leader>fk", function() Snacks.picker.keymaps() end,       desc = "Keymaps" },
      { "<leader>fc", function() Snacks.picker.git_log() end,       desc = "Git commits" },
    },
  },

  -- ── Oil: edit your filesystem like a buffer ──────────────────────────────
  {
    "stevearc/oil.nvim",
    dependencies = { "nvim-tree/nvim-web-devicons" },
    lazy = false,
    keys = {
      { "-", "<cmd>Oil<cr>", desc = "Open parent directory" },
    },
    opts = {
      view_options = { show_hidden = true },
      keymaps = { ["q"] = "actions.close" },
    },
  },

  -- ── Git signs in the gutter, and hunks from the keyboard ─────────────────
  {
    "lewis6991/gitsigns.nvim",
    cond = true,
    event = { "BufReadPre", "BufNewFile" },
    opts = {
      on_attach = function(bufnr)
        local gs = require("gitsigns")
        local function map(mode, l, r, desc)
          vim.keymap.set(mode, l, r, { buffer = bufnr, desc = desc })
        end
        map("n", "]h", function() gs.nav_hunk("next") end, "Next hunk")
        map("n", "[h", function() gs.nav_hunk("prev") end, "Previous hunk")
        map("n", "<leader>hs", gs.stage_hunk, "Stage hunk")
        map("n", "<leader>hr", gs.reset_hunk, "Reset hunk")
        map("n", "<leader>hp", gs.preview_hunk, "Preview hunk")
        map("n", "<leader>hb", function() gs.blame_line({ full = true }) end, "Blame line")
      end,
    },
  },

  -- ── Editing quality of life ──────────────────────────────────────────────
  -- Commenting is Neovim's own since 0.10: gcc, gc{motion}.
  { "kylechui/nvim-surround", cond = true, event = "VeryLazy", opts = {} },     -- ys/cs/ds
  { "windwp/nvim-autopairs",  cond = true, event = "InsertEnter", opts = {} },
  {
    "folke/todo-comments.nvim",
    dependencies = { "nvim-lua/plenary.nvim" },
    event = { "BufReadPost", "BufNewFile" },
    opts = {},
    keys = {
      { "<leader>ft", function() Snacks.picker.todo_comments() end, desc = "Find TODOs" },
    },
  },

  -- ── Keep your existing buffer-history plugin ─────────────────────────────
  { "voxelprismatic/rabbit.nvim", cmd = "Rabbit" },
}
