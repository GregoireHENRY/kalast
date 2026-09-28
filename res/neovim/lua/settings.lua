local o = vim.o
-- Line numbers with cursor.
o.nu = true
o.rnu = true

-- Share clipboard with system.
o.clipboard = 'unnamedplus'

-- No swap file.
o.swapfile = false
o.undodir = vim.fn.stdpath('state') .. '/undo'
o.undofile = true

-- Keep lines above and below cursor
o.scrolloff = 5

-- Tabs
o.tabstop = 4
o.softtabstop = 4
o.shiftwidth = 4

local g = vim.g
g.mapleader = " "
g.maplocalleader = " "

local m = vim.api.nvim_set_keymap
m("n", "zz", ":w<CR>", {noremap = true, silent = true})
m("n", "<leader>w", "zz", {noremap = true, silent = true})
m("n", "qq", ":q<CR>", {noremap = true, silent = true})
m("n", "<leader>q", "qq", {noremap = true, silent = true})
m("n", "<F2>", ":b#<CR>", {noremap = true, silent = true})

vim.api.nvim_create_autocmd("BufReadPost", {
    pattern = {"*"},
    callback = function()
        if vim.fn.line("'\"") > 1 and vim.fn.line("'\"") <= vim.fn.line("$") then
            vim.api.nvim_exec("normal! g'\"",false)
        end
    end
})

-- ═══════════════════════════════════════════════════════════════════════════
--  Added 2026-09-10 — options that the plugin set assumes.
--  Your tab settings, keymaps and undo config above are untouched.
-- ═══════════════════════════════════════════════════════════════════════════

o.termguicolors = true      -- 24-bit colour, required by catppuccin
o.signcolumn = "yes"        -- stop the text jumping when diagnostics appear
o.cursorline = true
o.mouse = "a"
o.updatetime = 250          -- faster CursorHold → quicker diagnostics/hover
o.timeoutlen = 400          -- which-key pops up sooner

-- Searching
o.ignorecase = true
o.smartcase = true          -- ...unless you type a capital
o.inccommand = "split"      -- live preview of :s///

-- Splits open where you expect
o.splitright = true
o.splitbelow = true

-- Whitespace made visible
o.list = true
vim.opt.listchars = { tab = "» ", trail = "·", nbsp = "␣" }

o.confirm = true            -- ask instead of failing on :q with unsaved changes
o.winborder = "rounded"

-- Briefly highlight whatever you just yanked
vim.api.nvim_create_autocmd("TextYankPost", {
    callback = function() vim.hl.on_yank() end,
})

-- Extra keymaps
local km = vim.keymap.set
km("n", "<Esc>", "<cmd>nohlsearch<CR>", { desc = "Clear search highlight" })
km("n", "<C-h>", "<C-w><C-h>", { desc = "Window left" })
km("n", "<C-l>", "<C-w><C-l>", { desc = "Window right" })
km("n", "<C-j>", "<C-w><C-j>", { desc = "Window down" })
km("n", "<C-k>", "<C-w><C-k>", { desc = "Window up" })
km("v", "<", "<gv", { desc = "Outdent and keep selection" })
km("v", ">", ">gv", { desc = "Indent and keep selection" })
