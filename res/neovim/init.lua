if vim.g.vscode then
    vim.opt.clipboard = 'unnamedplus'
else
    require('settings')
    require('plugins')
end
