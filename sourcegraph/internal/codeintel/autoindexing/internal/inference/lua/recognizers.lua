local languages = {
  "clang",
  "go",
  "java",
  "python",
  "rust",
  "test",
  "typescript",
}

local recognizers = {}
for _, name in ipairs(languages) do
  recognizers["sg." .. name] = loadfile(name .. ".lua")()
end

return recognizers
