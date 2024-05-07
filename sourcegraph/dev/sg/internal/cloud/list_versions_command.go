package cloud

import (
	"encoding/json"
	"os"
	"time"

	"github.com/grafana/regexp"
	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var ListVersionsEphemeralCommand = cli.Command{
	Name:        "list-versions",
	Usage:       "sg could list-versions",
	Description: "list ephemeral cloud instances attached to your GCP account",
	Action:      listTagsCloudEphemeral,
	Flags: []cli.Flag{
		&cli.BoolFlag{
			Name:  "json",
			Usage: "print the instance details in JSON",
		},
		&cli.BoolFlag{
			Name:  "raw",
			Usage: "print all of the instance details",
		},
		&cli.IntFlag{
			Name:  "limit",
			Usage: "limit the number of versions to list - to list everything set limt to a negative value",
			Value: 100,
		},
		&cli.StringFlag{
			Name:    "filter",
			Usage:   "filter versions by regex",
			Aliases: []string{"f"},
		},
	},
}

func listTagsCloudEphemeral(ctx *cli.Context) error {
	var filterRegex *regexp.Regexp
	if ctx.String("filter") != "" {
		filterRegex = regexp.MustCompile(ctx.String("filter"))
	}
	ar, err := NewArtifactRegistry(ctx.Context, "sourcegraph-ci", "us-central1", "cloud-ephemeral")
	if err != nil {
		return err
	}
	pending := std.Out.Pending(output.Linef(CloudEmoji, output.StylePending, "Retrieving docker images from registry %q", ar.RepositoryName))
	images, err := ar.ListDockerImages(ctx.Context, FilterTagByRegex(filterRegex))
	if err != nil {
		pending.Complete(output.Linef(output.EmojiFailure, output.StyleFailure, "Failed to retreive images from registry %q", ar.RepositoryName))
		return err
	}
	pending.Complete(output.Linef(CloudEmoji, output.StyleSuccess, "Retrieved %d docker images from registry %q", len(images), ar.RepositoryName))

	imagesByTag := map[string][]*DockerImage{}
	for _, image := range images {
		for _, tag := range image.Tags {
			if filterRegex == nil || filterRegex.MatchString(tag) {
				imagesByTag[tag] = append(imagesByTag[tag], image)
			}
		}
	}

	switch {
	case ctx.Bool("json"):
		{
			return json.NewEncoder(os.Stdout).Encode(imagesByTag)
		}
	case ctx.Bool("raw"):
		{
			count := 0
			limit := ctx.Int("limit")
			for tag, images := range imagesByTag {
				image := images[0]
				std.Out.Writef(`Tag                   : %s
Upload Time           : %s
Image count           : %d`, tag, image.UploadTime.AsTime().Format(time.DateTime), len(images))
				count++
				if limit >= 1 && count >= limit {
					break
				}
			}
		}
	default:
		{
			count := 0
			limit := ctx.Int("limit")
			std.Out.Writef("%-50s %-20s %-5s", "Tag", "Upload time", "Image count")
			for tag, images := range imagesByTag {
				// we use the first image to get the upload time
				image := images[0]
				if len(tag) > 50 {
					tag = tag[:47] + "..."
				}
				std.Out.Writef("%-50s %-20s %-5d", tag, image.UploadTime.AsTime().Format(time.DateTime), len(images))
				count++
				if limit >= 1 && count >= limit {
					break
				}
			}
			std.Out.WriteSuggestionf("Some tags might have been truncated. To see the full tag ouput use the --raw format or filter the tags by using --filter")
		}
	}
	return nil
}
