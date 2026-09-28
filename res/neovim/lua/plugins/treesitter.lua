-- nvim-treesitter's `main` branch, the rewrite: it installs parsers and
-- queries, and Neovim itself highlights and indents with them. `master`, which
-- this used, is frozen. Building a parser needs the tree-sitter CLI and a C
-- compiler.
local parsers = {
  "rust", "python", "lua", "c", "cpp", "cmake", "fortran",
  "bash", "toml", "yaml", "json", "markdown", "markdown_inline",
  "vim", "vimdoc", "query", "regex", "diff", "gitcommit", "gitignore",
  "java", "javascript", "html", "css", "latex", "bibtex",
}

return {
  {
    "nvim-treesitter/nvim-treesitter",
    branch = "main",
    lazy = false,            -- `main` does not support lazy-loading
    build = ":TSUpdate",
    config = function()
      require("nvim-treesitter").install(parsers)
      vim.api.nvim_create_autocmd("FileType", {
        callback = function(ev)
          if not pcall(vim.treesitter.start, ev.buf) then
            return -- no parser for this filetype
          end
          vim.bo[ev.buf].indentexpr = "v:lua.require'nvim-treesitter'.indentexpr()"
          -- Incremental selection, as before: <CR> starts and grows it, <BS>
          -- shrinks it -- Neovim 0.12's own `an` and `in`.
          vim.keymap.set("n", "<CR>", "van", { buffer = ev.buf, remap = true, desc = "Select node" })
          vim.keymap.set("x", "<CR>", "an", { buffer = ev.buf, remap = true, desc = "Grow selection" })
          vim.keymap.set("x", "<BS>", "in", { buffer = ev.buf, remap = true, desc = "Shrink selection" })
        end,
      })
    end,
  },

  -- Sticky context header: shows the enclosing fn/class while you scroll
  {
    "nvim-treesitter/nvim-treesitter-context",
    event = { "BufReadPost", "BufNewFile" },
    opts = { max_lines = 3 },
  },
}
