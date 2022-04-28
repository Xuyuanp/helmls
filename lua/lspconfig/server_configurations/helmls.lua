local util = require("lspconfig.util")

local suffix = "/lua/lspconfig/server_configurations/helm.lua"
local source_path = debug.getinfo(1, "S").source
local root_path = source_path:sub(2, source_path:len() - suffix:len() - 2)
local cmd = root_path .. "/target/release/helmls"

return {
	default_config = {
		cmd = { cmd },
		filetypes = { "helm", "yaml" },
		root_dir = function(fname)
			return util.root_pattern("Chart.yaml", "values.yaml")(fname)
		end,
		single_file_support = false,
		settings = {
			helm = {},
		},
	},
	docs = {
		description = [[
    Helm language server
]],
		default_config = {
			root_dir = [[root_pattern("values.yaml", "Chart.yaml")]],
		},
	},
}
