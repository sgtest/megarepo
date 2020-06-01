package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"sort"
	"strings"
	"time"
)

const resultsBuffer = 5

type options struct {
	slackWebhook    *string
	sheetID         *string
	window          *time.Duration
	highlightWindow *time.Duration

	gcp                *bool
	gcpLabelsWhitelist map[string]string

	aws              *bool
	awsTagsWhitelist map[string]string

	runID   *string
	dry     *bool
	verbose *bool
	timeout *time.Duration
}

func main() {
	help := flag.Bool("help", false, "Show help text")
	gcpWhitelistLabelsStr := flag.String("gcp.whitelist", "", "GCP labels to whitelist (comma-separated key:value pairs)")
	awsWhitelistTagsStr := flag.String("aws.whitelist", "", "AWS tags to whitelist (comma-separated key:value pairs)")
	opts := options{
		slackWebhook:    flag.String("slack.webhook", os.Getenv("SLACK_WEBHOOK"), "Slack webhook to post updates to"),
		sheetID:         flag.String("sheet.id", os.Getenv("SHEET_ID"), "Slack webhook to post updates to"),
		gcp:             flag.Bool("gcp", false, "Report on Google Cloud resources"),
		aws:             flag.Bool("aws", false, "Report on Amazon Web Services resources"),
		window:          flag.Duration("window", 48*time.Hour, "Restrict results to resources created within a period"),
		highlightWindow: flag.Duration("window.highlight", 24*time.Hour, "Highlight resources created within a period"),

		runID:   flag.String("run.id", os.Getenv("GITHUB_RUN_ID"), "ID of workflow run"),
		dry:     flag.Bool("dry", false, "Do not post updates to slack, but print them to stdout"),
		verbose: flag.Bool("verbose", false, "Print debug output to stdout"),
		timeout: flag.Duration("timeout", time.Minute, "Set a timeout for report generation"),
	}
	flag.Parse()
	if *help {
		flag.CommandLine.Usage()
		return
	}
	if *gcpWhitelistLabelsStr != "" {
		opts.gcpLabelsWhitelist = csvToMap(*gcpWhitelistLabelsStr)
	}
	if *awsWhitelistTagsStr != "" {
		opts.awsTagsWhitelist = csvToMap(*awsWhitelistTagsStr)
	}
	if err := run(opts); err != nil {
		log.Fatal(err)
	}
	log.Println("done")
}

func run(opts options) error {
	ctx, cancel := context.WithTimeout(context.Background(), *opts.timeout)
	defer cancel()

	// Collect resources - let detailed errors be handled by reportErr, which
	// will attempt to send it to Slack. This hopefully prevents errors from
	// revealing too much in our public build logs. If Slack fails, just log it
	// and hope Slack doesn't spit out anything sensitive.
	var resources Resources
	since := time.Now().UTC().Add(-*opts.window)
	if *opts.gcp {
		rs, err := collectGCPResources(ctx, since, *opts.verbose, opts.gcpLabelsWhitelist)
		if err != nil {
			reportError(ctx, opts, err, "gcp")
			return fmt.Errorf("gcp: failed to collect resources")
		}
		resources = append(resources, rs...)
	}
	if *opts.aws {
		rs, err := collectAWSResources(ctx, since, *opts.verbose, opts.awsTagsWhitelist)
		if err != nil {
			reportError(ctx, opts, err, "aws")
			return fmt.Errorf("aws: failed to collect resources")
		}
		resources = append(resources, rs...)
	}
	sort.Sort(resources)

	// report results
	if *opts.verbose {
		log.Println("collected resources:\n", reportString(resources))
		log.Printf("found a total of %d resources created since %s", len(resources), since.String())
	}
	if !*opts.dry {
		if err := generateReport(ctx, opts, resources); err != nil {
			return fmt.Errorf("report: %w", err)
		}
	}

	return nil
}

func reportString(resources Resources) string {
	var output string
	for _, r := range resources {
		output += fmt.Sprintf(" * %+v\n", r)
	}
	return output
}

// csvToMap accepts a comma-delimited set of key:pair values (e.g. `key1:value1,key2:value2`)
// and converts it to a map.
func csvToMap(str string) map[string]string {
	m := map[string]string{}
	for _, pair := range strings.Split(str, ",") {
		keyValue := strings.Split(pair, ":")
		m[keyValue[0]] = keyValue[1]
	}
	return m
}
