package changed

import "path/filepath"

func contains(s []string, str string) bool {
	for _, v := range s {
		if v == str {
			return true
		}
	}
	return false
}

// Changes in the root directory files should trigger client tests.
var clientRootFiles = []string{
	"package.json",
	"pnpm-lock.yaml",
	"jest.config.base.js",
	"jest.config.js",
	"postcss.config.js",
	"tsconfig.all.json",
	"tsconfig.json",
	"babel.config.js",
	".percy.yml",
	".eslintrc.js",
	"gulpfile.js",
}

func isRootClientFile(p string) bool {
	return filepath.Dir(p) == "." && contains(clientRootFiles, p)
}
