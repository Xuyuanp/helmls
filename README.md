# helmls

Helm Language Server.

## Status

This project is still in very early development.

## Try with neovim

Installing with `packer`

```lua
require('packer').startup(function(use)
    use({
        'Xuyuanp/helmls',
        run = 'cargo build --release',
        config = function()
            require('lspconfig').helmls.setup({
                -- on_attach = ..
            })
        end,
    })
end)
```

## Features & Roadmap

* [x] `gotoDefinition` for helm variables.
* [ ] Autocomplete builtin objects (`.Values`, `.Chart`, etc).
* [ ] Autocomplete for helm variables.
* [ ] Autocomplete for templates defined in `_helpers.tpl`
* [ ] Maybe more
