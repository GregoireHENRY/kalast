return {
  -- ── Mason: installs language servers / formatters into ~/.local/share/nvim
  {
    "mason-org/mason.nvim",
    cmd = { "Mason", "MasonInstall", "MasonUpdate", "MasonLog", "MasonUninstall" },
    keys = { { "<leader>cm", "<cmd>Mason<cr>", desc = "Mason" } },
    opts = { ui = { border = "rounded" } },
  },

  -- ── LSP ──────────────────────────────────────────────────────────────────
  {
    "neovim/nvim-lspconfig",
    event = { "BufReadPre", "BufNewFile" },
    dependencies = { "mason-org/mason.nvim", "saghen/blink.cmp" },
    config = function()
      -- Diagnostics presentation
      vim.diagnostic.config({
        virtual_text = { prefix = "●", spacing = 2 },
        severity_sort = true,
        float = { border = "rounded", source = true },
        signs = {
          text = {
            [vim.diagnostic.severity.ERROR] = " ",
            [vim.diagnostic.severity.WARN]  = " ",
            [vim.diagnostic.severity.HINT]  = " ",
            [vim.diagnostic.severity.INFO]  = " ",
          },
        },
      })

      -- Keymaps, bound only in buffers that actually have a server attached
      vim.api.nvim_create_autocmd("LspAttach", {
        callback = function(ev)
          local function map(keys, fn, desc)
            vim.keymap.set("n", keys, fn, { buffer = ev.buf, desc = "LSP: " .. desc })
          end
          map("gd", vim.lsp.buf.definition,      "Goto definition")
          map("gD", vim.lsp.buf.declaration,     "Goto declaration")
          map("gr", vim.lsp.buf.references,      "References")
          map("gi", vim.lsp.buf.implementation,  "Goto implementation")
          map("K",  vim.lsp.buf.hover,           "Hover docs")
          map("<leader>rn", vim.lsp.buf.rename,  "Rename symbol")
          map("<leader>ca", vim.lsp.buf.code_action, "Code action")
          map("<leader>e",  vim.diagnostic.open_float, "Line diagnostics")
          map("[d", function() vim.diagnostic.jump({ count = -1 }) end, "Previous diagnostic")
          map("]d", function() vim.diagnostic.jump({ count = 1 })  end, "Next diagnostic")

          -- Inlay hints where the server supports them (great for Rust)
          local client = vim.lsp.get_client_by_id(ev.data.client_id)
          if client and client:supports_method("textDocument/inlayHint") then
            vim.lsp.inlay_hint.enable(true, { bufnr = ev.buf })
            map("<leader>th", function()
              vim.lsp.inlay_hint.enable(not vim.lsp.inlay_hint.is_enabled({ bufnr = ev.buf }),
                { bufnr = ev.buf })
            end, "Toggle inlay hints")
          end
        end,
      })

      -- Advertise blink.cmp's extra completion capabilities to every server
      vim.lsp.config("*", {
        capabilities = require("blink.cmp").get_lsp_capabilities({}, true),
      })

      vim.lsp.config("lua_ls", {
        settings = {
          Lua = {
            runtime = { version = "LuaJIT" },
            workspace = { checkThirdParty = false },
            diagnostics = { globals = { "vim" } },
            telemetry = { enable = false },
          },
        },
      })

      vim.lsp.config("basedpyright", {
        settings = {
          basedpyright = {
            analysis = { typeCheckingMode = "standard", autoImportCompletions = true },
          },
        },
      })

      -- rust-analyzer is handled by rustaceanvim below, not here.
      -- Each server is enabled only if its binary is actually present, so a
      -- server you haven't installed stays quiet instead of erroring on every
      -- matching buffer. Install one later (:Mason) and it lights up by itself.
      -- Python is ty, Astral's type checker and language server, beside ruff;
      -- basedpyright only where ty is not installed.
      local servers = {
        lua_ls       = "lua-language-server",
        ty           = "ty",                          -- python types, completion, hover
        ruff         = "ruff",                        -- python lint/format
        clangd       = "clangd",                      -- C / C++
        fortls       = "fortls",                      -- Fortran (brew)
        texlab       = "texlab",                      -- LaTeX
        taplo        = "taplo",                       -- TOML
        jsonls       = "vscode-json-language-server", -- needs node
        yamlls       = "yaml-language-server",        -- needs node
        bashls       = "bash-language-server",        -- needs node
      }
      for server, bin in pairs(servers) do
        if vim.fn.executable(bin) == 1 then
          vim.lsp.enable(server)
        end
      end
      if vim.fn.executable("ty") == 0 and vim.fn.executable("basedpyright-langserver") == 1 then
        vim.lsp.enable("basedpyright")
      end
    end,
  },

  -- ── Rust: rustaceanvim wires up rust-analyzer with the extra RA features ─
  {
    "mrcjkb/rustaceanvim",
    version = "^9",
    lazy = false,   -- the plugin registers itself as a filetype plugin
    init = function()
      vim.g.rustaceanvim = {
        server = {
          default_settings = {
            ["rust-analyzer"] = {
              cargo = { allFeatures = true, buildScripts = { enable = true } },
              procMacro = { enable = true },
              checkOnSave = true,
              check = { command = "clippy" },
              inlayHints = { lifetimeElisionHints = { enable = "skip_trivial" } },
            },
          },
        },
      }
    end,
  },

  -- ── Formatting on demand (<leader>cf) ────────────────────────────────────
  {
    "stevearc/conform.nvim",
    event = "BufWritePre",
    keys = {
      { "<leader>cf", function() require("conform").format({ async = true, lsp_format = "fallback" }) end,
        mode = { "n", "v" }, desc = "Format buffer" },
    },
    opts = {
      formatters_by_ft = {
        lua = { "stylua" },
        python = { "ruff_organize_imports", "ruff_format" },
        toml = { "taplo" },
        rust = { "rustfmt" },
        c = { "clang-format" },
        cpp = { "clang-format" },
        json = { "jq" },
        sh = { "shfmt" },
      },
      -- Explicit formatting only; no surprise reformats on save.
      format_on_save = nil,
    },
  },
}
