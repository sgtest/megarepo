package main

import (
	"bufio"
	"bytes"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"

	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	k8syamlapi "k8s.io/apimachinery/pkg/util/yaml"
	"sigs.k8s.io/yaml"
)

func main() {
	helmRepoRoot := flag.String("deploy-sourcegraph-helm-path", filepath.Join("..", "deploy-sourcegraph-helm"), "Path to deploy-sourcegraph-helm repository checkout.")
	helmTemplateExtraArgs := flag.String("helm-template-extra-args", "", "extra args to pass to `helm template`")
	component := flag.String("component", "", "Which SG service to target (comma-separated list).")
	goldenFile := flag.String("golden-file", "", "Which golden fixture to compare.")
	noColor := flag.Bool("no-color", false, "Do not try to produce diffs in color. This is necessary for non-GNU diff users.")
	flag.Parse()

	if *component == "" {
		fatal("must pass -component")
	}
	if *goldenFile == "" {
		fatal("must pass -golden-file")
	}

	components := strings.Split(*component, ",")

	helmObjs := parseHelmResources(*helmTemplateExtraArgs, *helmRepoRoot, components)

	goldenContent, err := os.ReadFile(*goldenFile)
	must(err)
	var goldenResources goldenResources
	must(k8syamlapi.Unmarshal(goldenContent, &goldenResources))

	tmpDir, err := os.MkdirTemp("", "compare-helm-")
	must(err)
	defer os.RemoveAll(tmpDir)
	sortedGoldenPath := filepath.Join(tmpDir, "sorted-goldens.yaml")
	helmTemplateOutputPath := filepath.Join(tmpDir, "sorted-helm-template.yaml")

	sortedHelmResourceFile, err := openForWriting(helmTemplateOutputPath)
	must(err)
	sortedGoldenFile, err := openForWriting(sortedGoldenPath)
	must(err)

	// Write all helm and golden objects to their respective files for diffing.
	// The order of these objects (by {kind, metadata.name}) should match, so
	// that the diff has a chance of making sense.
	// The key order within each object should be normalized too, since
	// semantically we don't want that to influence the diff. We achieve this by
	// unmarshalling and re-marshalling each object.
	for _, helmObj := range helmObjs {
		fmt.Fprintln(sortedHelmResourceFile, "---")
		fmt.Fprintln(sortedGoldenFile, "---")

		fmt.Fprintf(sortedHelmResourceFile, "# helm: %s/%s\n", helmObj.GetKind(), helmObj.GetName())
		helmObjBytes, err := yaml.Marshal(helmObj)
		must(err)
		_, err = sortedHelmResourceFile.Write(helmObjBytes)
		must(err)

		// find corresponding golden object
		for i, goldenObj := range goldenResources.Resources {
			if helmObj.GetName() == goldenObj.GetName() &&
				helmObj.GetKind() == goldenObj.GetKind() {

				fmt.Fprintf(sortedGoldenFile, "# golden: %s/%s\n", helmObj.GetKind(), helmObj.GetName())
				goldenBytes, err := yaml.Marshal(goldenObj)
				must(err)
				_, err = sortedGoldenFile.Write(goldenBytes)
				must(err)

				// remove the golden object so that only unmatched resources
				// remain
				goldenResources.Resources = append(goldenResources.Resources[:i], goldenResources.Resources[i+1:]...)

				break
			}
		}
	}

	// Print any golden resources that didn't correspond to any helm resources,
	// so that they appear in the diff.
	for _, unmatchedGolden := range goldenResources.Resources {
		fmt.Fprintln(sortedGoldenFile, "---")
		fmt.Fprintf(sortedGoldenFile, "# golden: %s/%s\n", unmatchedGolden.GetKind(), unmatchedGolden.GetName())
		goldenBytes, err := yaml.Marshal(unmatchedGolden)
		must(err)
		_, err = sortedGoldenFile.Write(goldenBytes)
		must(err)
	}

	must(sortedHelmResourceFile.Close())
	must(sortedGoldenFile.Close())

	var diffCmdArgs []string
	if !*noColor {
		diffCmdArgs = append(diffCmdArgs, "--color=auto")
	}
	diffCmdArgs = append(diffCmdArgs, helmTemplateOutputPath, sortedGoldenPath)
	diffCmd := exec.Command("diff", diffCmdArgs...)
	diffCmd.Stdout = os.Stdout
	diffCmd.Stderr = os.Stderr
	must(diffCmd.Run())
}

func parseHelmResources(helmTemplateExtraArgs, helmRepoRoot string, components []string) []*unstructured.Unstructured {
	helmShellCmd := "helm template . " + helmTemplateExtraArgs
	helmCmd := exec.Command("sh", "-c", helmShellCmd)
	helmCmd.Dir = filepath.Join(helmRepoRoot, "charts", "sourcegraph")

	var helmStdout bytes.Buffer
	helmCmd.Stdout = &helmStdout
	helmCmd.Stderr = os.Stderr

	must(helmCmd.Run())

	var helmObjs []*unstructured.Unstructured
	multiDocYAMLReader := k8syamlapi.NewYAMLReader(bufio.NewReader(&helmStdout))
	for {
		yamlDoc, err := multiDocYAMLReader.Read()
		if err == io.EOF {
			break
		}
		must(err)
		jsonDoc, err := k8syamlapi.ToJSON(yamlDoc)
		must(err)
		obj, _, err := unstructured.UnstructuredJSONScheme.Decode(jsonDoc, nil, nil)
		must(err)

		k8sObj := obj.(*unstructured.Unstructured)
		if slices.Contains(components, k8sObj.GetLabels()["app.kubernetes.io/component"]) {
			helmObjs = append(helmObjs, k8sObj)
		}
	}
	return helmObjs
}

// A shame to dup this
type goldenResources struct {
	Resources []*unstructured.Unstructured `json:"resources"`
}

func openForWriting(pathname string) (*os.File, error) {
	return os.OpenFile(pathname, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0600)
}

func must(err error) {
	if err != nil {
		fatal(err.Error())
	}
}

func fatal(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}
