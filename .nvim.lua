

vim.cmd([[
let g:neoformat_rust_rustfmt = {
            \ 'args': ['--edition 2021'],
        \ 'exe': 'rustfmt',
        \ 'stdin': 0,
            \ }

let g:neoformat_enabled_rust = []

augroup fmt
  autocmd!
  autocmd BufWritePost *.rs silent !rustfmt % --edition 2021
augroup END

]])
