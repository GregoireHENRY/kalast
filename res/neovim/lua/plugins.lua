-- Bootstrap lazy.nvim
local lazypath = vim.fn.stdpath("data") .. "/lazy/lazy.nvim"
if not (vim.uv or vim.loop).fs_stat(lazypath) then
  vim.fn.system({
    "git", "clone", "--filter=blob:none",
    "https://github.com/folke/lazy.nvim.git",
    "--branch=stable", lazypath,
  })
end
vim.opt.rtp:prepend(lazypath)

require("lazy").setup({
  spec = {
    { import = "plugins.ui" },
    { import = "plugins.editor" },
    { import = "plugins.treesitter" },
    { import = "plugins.lsp" },
    { import = "plugins.completion" },
  },
  -- Inside kalast (vim.g.kalast) the script editor draws the text itself and
  -- brings its own language server: plugins' windows, colours, statusline and
  -- completion menu are never seen there, and blink.cmp's unseen menu would
  -- still take <Tab> and <CR>. Only the plugins marked `cond = true` -- the
  -- editing ones -- load there, and the others are not even installed.
  defaults = { cond = function() return not vim.g.kalast end },
  install = { colorscheme = { "catppuccin" } },
  checker = { enabled = not vim.g.kalast, notify = false },  -- check for plugin updates quietly
  change_detection = { notify = false },
  performance = {
    rtp = {
      disabled_plugins = { "gzip", "tarPlugin", "tohtml", "zipPlugin", "tutor" },
    },
  },
})
