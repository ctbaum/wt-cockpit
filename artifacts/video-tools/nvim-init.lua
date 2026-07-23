vim.opt.number = true
vim.opt.relativenumber = false
vim.opt.termguicolors = true
vim.opt.laststatus = 3
vim.opt.showmode = false
vim.opt.cmdheight = 1
vim.opt.swapfile = false
vim.opt.rtp:prepend("/Users/Shared/herdr-deck-demo/nvim-pack/snacks.nvim")
vim.opt.rtp:prepend("/Users/Shared/herdr-deck-demo/nvim-pack/codex.nvim")
vim.opt.rtp:prepend("/Users/Shared/herdr-deck-demo/nvim-pack/herdr-agents.nvim")

vim.cmd.colorscheme("habamax")
require("snacks").setup({})
require("herdr-agents").setup({
  claude = { enabled = false },
  codex = {
    enabled = true,
    opts = {
      focus_after_send = false,
    },
  },
})
