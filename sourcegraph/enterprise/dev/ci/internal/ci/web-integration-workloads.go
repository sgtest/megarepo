package ci

import (
	"os"
	"path/filepath"
	"strings"
)

// Code in this file is used to split web integration tests workloads.

func contains(s []string, str string) bool {
	for _, v := range s {
		if v == str {
			return true
		}
	}
	return false
}

func getWebIntegrationFileNames() []string {
	var fileNames []string

	err := filepath.Walk("client/web/src/integration", func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if strings.HasSuffix(path, ".test.ts") {
			fileNames = append(fileNames, path)
		}

		return nil
	})

	if err != nil {
		panic(err)
	}

	return fileNames
}

func chunkItems(items []string, size int) [][]string {
	lenItems := len(items)
	lenChunks := lenItems/size + 1
	chunks := make([][]string, lenChunks)

	for i := 0; i < lenChunks; i++ {
		start := i * size
		end := min(start+size, lenItems)
		chunks[i] = items[start:end]
	}

	return chunks
}

func min(x int, y int) int {
	if x < y {
		return x
	}

	return y
}

// getChunkedWebIntegrationFileNames gets web integration test filenames and splits them in chunks for parallelizing client integration tests.
func getChunkedWebIntegrationFileNames(chunkSize int) []string {
	testFiles := getWebIntegrationFileNames()
	chunkedTestFiles := chunkItems(testFiles, chunkSize)
	chunkedTestFileStrings := make([]string, len(chunkedTestFiles))

	for i, v := range chunkedTestFiles {
		chunkedTestFileStrings[i] = strings.Join(v, " ")
	}

	return chunkedTestFileStrings
}
