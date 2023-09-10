// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:native_assets_builder/native_assets_builder.dart' show BuildResult;
import 'package:native_assets_cli/native_assets_cli.dart' hide BuildMode;
import 'package:native_assets_cli/native_assets_cli.dart' as native_assets_cli;

import '../base/file_system.dart';
import '../build_info.dart';
import '../globals.dart' as globals;
import '../native_assets.dart';
import 'native_assets_host.dart';

/// Dry run the native builds.
///
/// This does not build native assets, it only simulates what the final paths
/// of all assets will be so that this can be embedded in the kernel file and
/// the Xcode project.
Future<Uri?> dryRunNativeAssetsMacOS({
  required NativeAssetsBuildRunner buildRunner,
  required Uri projectUri,
  bool flutterTester = false,
  required FileSystem fileSystem,
}) async {
  if (await hasNoPackageConfig(buildRunner) || await isDisabledAndNoNativeAssets(buildRunner)) {
    return null;
  }

  final Uri buildUri_ = nativeAssetsBuildUri(projectUri, OS.macOS);
  final Iterable<Asset> nativeAssetPaths = await dryRunNativeAssetsMacOSInternal(fileSystem, projectUri, flutterTester, buildRunner);
  final Uri nativeAssetsUri = await writeNativeAssetsYaml(nativeAssetPaths, buildUri_, fileSystem);
  return nativeAssetsUri;
}

Future<Iterable<Asset>> dryRunNativeAssetsMacOSInternal(
  FileSystem fileSystem,
  Uri projectUri,
  bool flutterTester,
  NativeAssetsBuildRunner buildRunner,
) async {
  const OS targetOs = OS.macOS;
  final Uri buildUri_ = nativeAssetsBuildUri(projectUri, targetOs);

  globals.logger.printTrace('Dry running native assets for $targetOs.');
  final List<Asset> nativeAssets = (await buildRunner.dryRun(
    linkModePreference: LinkModePreference.dynamic,
    targetOs: targetOs,
    workingDirectory: projectUri,
    includeParentEnvironment: true,
  ))
      .assets;
  ensureNoLinkModeStatic(nativeAssets);
  globals.logger.printTrace('Dry running native assets for $targetOs done.');
  final Uri? absolutePath = flutterTester ? buildUri_ : null;
  final Map<Asset, Asset> assetTargetLocations = _assetTargetLocations(nativeAssets, absolutePath);
  final Iterable<Asset> nativeAssetPaths = assetTargetLocations.values;
  return nativeAssetPaths;
}

/// Builds native assets.
///
/// If [darwinArchs] is omitted, the current target architecture is used.
///
/// If [flutterTester] is true, absolute paths are emitted in the native
/// assets mapping. This can be used for JIT mode without sandbox on the host.
/// This is used in `flutter test` and `flutter run -d flutter-tester`.
Future<(Uri? nativeAssetsYaml, List<Uri> dependencies)> buildNativeAssetsMacOS({
  required NativeAssetsBuildRunner buildRunner,
  List<DarwinArch>? darwinArchs,
  required Uri projectUri,
  required BuildMode buildMode,
  bool flutterTester = false,
  String? codesignIdentity,
  Uri? yamlParentDirectory,
  required FileSystem fileSystem,
}) async {
  const OS targetOs = OS.macOS;
  final Uri buildUri_ = nativeAssetsBuildUri(projectUri, targetOs);
  if (await hasNoPackageConfig(buildRunner) || await isDisabledAndNoNativeAssets(buildRunner)) {
    final Uri nativeAssetsYaml = await writeNativeAssetsYaml(<Asset>[], yamlParentDirectory ?? buildUri_, fileSystem);
    return (nativeAssetsYaml, <Uri>[]);
  }

  final List<Target> targets = darwinArchs != null ? darwinArchs.map(_getNativeTarget).toList() : <Target>[Target.current];
  final native_assets_cli.BuildMode buildModeCli = nativeAssetsBuildMode(buildMode);

  globals.logger.printTrace('Building native assets for $targets $buildModeCli.');
  final List<Asset> nativeAssets = <Asset>[];
  final Set<Uri> dependencies = <Uri>{};
  for (final Target target in targets) {
    final BuildResult result = await buildRunner.build(
      linkModePreference: LinkModePreference.dynamic,
      target: target,
      buildMode: buildModeCli,
      workingDirectory: projectUri,
      includeParentEnvironment: true,
      cCompilerConfig: await buildRunner.cCompilerConfig,
    );
    nativeAssets.addAll(result.assets);
    dependencies.addAll(result.dependencies);
  }
  ensureNoLinkModeStatic(nativeAssets);
  globals.logger.printTrace('Building native assets for $targets done.');
  final Uri? absolutePath = flutterTester ? buildUri_ : null;
  final Map<Asset, Asset> assetTargetLocations = _assetTargetLocations(nativeAssets, absolutePath);
  final Map<AssetPath, List<Asset>> fatAssetTargetLocations = _fatAssetTargetLocations(nativeAssets, absolutePath);
  await copyNativeAssetsMacOSHost(buildUri_, fatAssetTargetLocations, codesignIdentity, buildMode, fileSystem);
  final Uri nativeAssetsUri = await writeNativeAssetsYaml(assetTargetLocations.values, yamlParentDirectory ?? buildUri_, fileSystem);
  return (nativeAssetsUri, dependencies.toList());
}

/// Extract the [Target] from a [DarwinArch].
Target _getNativeTarget(DarwinArch darwinArch) {
  switch (darwinArch) {
    case DarwinArch.arm64:
      return Target.macOSArm64;
    case DarwinArch.x86_64:
      return Target.macOSX64;
    case DarwinArch.armv7:
      throw Exception('Unknown DarwinArch: $darwinArch.');
  }
}

Map<AssetPath, List<Asset>> _fatAssetTargetLocations(List<Asset> nativeAssets, Uri? absolutePath) {
  final Map<AssetPath, List<Asset>> result = <AssetPath, List<Asset>>{};
  for (final Asset asset in nativeAssets) {
    final AssetPath path = _targetLocationMacOS(asset, absolutePath).path;
    result[path] ??= <Asset>[];
    result[path]!.add(asset);
  }
  return result;
}

Map<Asset, Asset> _assetTargetLocations(List<Asset> nativeAssets, Uri? absolutePath) => <Asset, Asset>{
  for (final Asset asset in nativeAssets)
    asset: _targetLocationMacOS(asset, absolutePath),
};

Asset _targetLocationMacOS(Asset asset, Uri? absolutePath) {
  final AssetPath path = asset.path;
  switch (path) {
    case AssetSystemPath _:
    case AssetInExecutable _:
    case AssetInProcess _:
      return asset;
    case AssetAbsolutePath _:
      final String fileName = path.uri.pathSegments.last;
      Uri uri;
      if (absolutePath != null) {
        // Flutter tester needs full host paths.
        uri = absolutePath.resolve(fileName);
      } else {
        // Flutter Desktop needs "absolute" paths inside the app.
        // "relative" in the context of native assets would be relative to the
        // kernel or aot snapshot.
        uri = Uri(path: fileName);
      }
      return asset.copyWith(path: AssetAbsolutePath(uri));
  }
  throw Exception('Unsupported asset path type ${path.runtimeType} in asset $asset');
}
