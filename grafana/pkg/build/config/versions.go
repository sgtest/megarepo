package config

var Versions = VersionMap{
	PullRequestMode: {
		Variants: []Variant{
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArm64,
			// VariantArm64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      false,
			AdminSign: false,
		},
		Docker: Docker{
			ShouldSave: false,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
			},
			Distribution: []Distribution{
				Alpine,
			},
		},
	},
	MainMode: {
		Variants: []Variant{
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArmV6,
			// VariantArmV7,
			// VariantArmV7Musl,
			VariantArm64,
			VariantArm64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: false,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
				// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
				// ArchARMv7, // GOARCH=ARM is used for both armv6 and armv7. They are differentiated by the GOARM variable.
			},
			Distribution: []Distribution{
				Alpine,
				Ubuntu,
			},
		},
		Buckets: Buckets{
			Artifacts:            "grafana-downloads",
			ArtifactsEnterprise2: "grafana-downloads-enterprise2",
			CDNAssets:            "grafana-static-assets",
			Storybook:            "grafana-storybook",
		},
	},
	DownstreamMode: {
		Variants: []Variant{
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArmV6,
			//VariantArmV7,
			// VariantArmV7Musl,
			VariantArm64,
			VariantArm64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: true,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
				// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
				// ArchARMv7, // GOARCH=ARM is used for both armv6 and armv7. They are differentiated by the GOARM variable.
			},
			Distribution: []Distribution{
				Alpine,
				Ubuntu,
			},
		},
		Buckets: Buckets{
			Artifacts:            "grafana-downloads",
			ArtifactsEnterprise2: "grafana-downloads-enterprise2",
			CDNAssets:            "grafana-static-assets",
		},
	},
	ReleaseBranchMode: {
		Variants: []Variant{
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArmV6,
			// VariantArmV7,
			// VariantArmV7Musl,
			VariantArm64,
			VariantArm64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: true,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
				ArchARMv7,
			},
			Distribution: []Distribution{
				Alpine,
				Ubuntu,
			},
			PrereleaseBucket: "grafana-prerelease/artifacts/docker",
		},
		Buckets: Buckets{
			Artifacts:            "grafana-downloads",
			ArtifactsEnterprise2: "grafana-downloads-enterprise2",
			CDNAssets:            "grafana-static-assets",
		},
	},
	TagMode: {
		Variants: []Variant{
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArmV6,
			// VariantArmV7,
			// VariantArmV7Musl,
			VariantArm64,
			VariantArm64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: true,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
				// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
				// ArchARMv7,
			},
			Distribution: []Distribution{
				Alpine,
				Ubuntu,
			},
			PrereleaseBucket: "grafana-prerelease/artifacts/docker",
		},
		Buckets: Buckets{
			Artifacts:            "grafana-prerelease/artifacts/downloads",
			ArtifactsEnterprise2: "grafana-prerelease/artifacts/downloads-enterprise2",
			CDNAssets:            "grafana-prerelease",
			CDNAssetsDir:         "artifacts/static-assets",
			Storybook:            "grafana-prerelease",
			StorybookSrcDir:      "artifacts/storybook",
		},
	},
	Enterprise2Mode: {
		Variants: []Variant{
			// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
			// VariantArmV6,
			// VariantArmV7,
			// VariantArmV7Musl,
			VariantArm64,
			VariantArm64Musl,
			VariantDarwinAmd64,
			VariantWindowsAmd64,
			VariantLinuxAmd64,
			VariantLinuxAmd64Musl,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: true,
			Architectures: []Architecture{
				ArchAMD64,
				ArchARM64,
				// https://github.com/golang/go/issues/58425 disabling arm builds until go issue is resolved
				// ArchARMv7,
			},
			Distribution: []Distribution{
				Alpine,
				Ubuntu,
			},
			PrereleaseBucket: "grafana-prerelease/artifacts/docker",
		},
		Buckets: Buckets{
			Artifacts:            "grafana-prerelease/artifacts/downloads",
			ArtifactsEnterprise2: "grafana-prerelease/artifacts/downloads-enterprise2",
			CDNAssets:            "grafana-prerelease",
			CDNAssetsDir:         "artifacts/static-assets",
			Storybook:            "grafana-prerelease",
			StorybookSrcDir:      "artifacts/storybook",
		},
	},
	CloudMode: {
		Variants: []Variant{
			VariantLinuxAmd64Musl,
			// We still need this variant to build the .deb file
			VariantLinuxAmd64,
		},
		PluginSignature: PluginSignature{
			Sign:      true,
			AdminSign: true,
		},
		Docker: Docker{
			ShouldSave: true,
			Architectures: []Architecture{
				ArchAMD64,
			},
			Distribution: []Distribution{
				Alpine,
			},
			PrereleaseBucket: "grafana-prerelease/artifacts/docker",
		},
		Buckets: Buckets{
			Artifacts:            "grafana-prerelease/artifacts/downloads",
			ArtifactsEnterprise2: "grafana-prerelease/artifacts/downloads-enterprise2",
			CDNAssets:            "grafana-prerelease",
			CDNAssetsDir:         "artifacts/static-assets",
			Storybook:            "grafana-prerelease",
			StorybookSrcDir:      "artifacts/storybook",
		},
	},
}
