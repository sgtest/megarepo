// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:native_assets_builder/native_assets_builder.dart'
    show BuildResult, DryRunResult;
import 'package:native_assets_cli/native_assets_cli_internal.dart'
    hide BuildMode;
import 'package:native_assets_cli/native_assets_cli_internal.dart'
    as native_assets_cli;

import '../../../base/file_system.dart';
import '../../../build_info.dart';
import '../../../globals.dart' as globals;

import '../macos/native_assets_host.dart';
import '../native_assets.dart';

/// Dry run the native builds.
///
/// This does not build native assets, it only simulates what the final paths
/// of all assets will be so that this can be embedded in the kernel file and
/// the Xcode project.
Future<Uri?> dryRunNativeAssetsIOS({
  required NativeAssetsBuildRunner buildRunner,
  required Uri projectUri,
  required FileSystem fileSystem,
}) async {
  if (!await nativeBuildRequired(buildRunner)) {
    return null;
  }

  final Uri buildUri = nativeAssetsBuildUri(projectUri, OS.iOS);
  final Iterable<Asset> assetTargetLocations = await dryRunNativeAssetsIOSInternal(
    fileSystem,
    projectUri,
    buildRunner,
  );
  final Uri nativeAssetsUri = await writeNativeAssetsYaml(
    assetTargetLocations,
    buildUri,
    fileSystem,
  );
  return nativeAssetsUri;
}

Future<Iterable<Asset>> dryRunNativeAssetsIOSInternal(
  FileSystem fileSystem,
  Uri projectUri,
  NativeAssetsBuildRunner buildRunner,
) async {
  const OS targetOS = OS.iOS;
  globals.logger.printTrace('Dry running native assets for $targetOS.');
  final DryRunResult dryRunResult = await buildRunner.dryRun(
    linkModePreference: LinkModePreference.dynamic,
    targetOS: targetOS,
    workingDirectory: projectUri,
    includeParentEnvironment: true,
  );
  ensureNativeAssetsBuildSucceed(dryRunResult);
  final List<Asset> nativeAssets = dryRunResult.assets;
  ensureNoLinkModeStatic(nativeAssets);
  globals.logger.printTrace('Dry running native assets for $targetOS done.');
  final Iterable<Asset> assetTargetLocations = _assetTargetLocations(nativeAssets).values;
  return assetTargetLocations;
}

/// Builds native assets.
Future<List<Uri>> buildNativeAssetsIOS({
  required NativeAssetsBuildRunner buildRunner,
  required List<DarwinArch> darwinArchs,
  required EnvironmentType environmentType,
  required Uri projectUri,
  required BuildMode buildMode,
  String? codesignIdentity,
  required Uri yamlParentDirectory,
  required FileSystem fileSystem,
}) async {
  if (!await nativeBuildRequired(buildRunner)) {
    await writeNativeAssetsYaml(<Asset>[], yamlParentDirectory, fileSystem);
    return <Uri>[];
  }

  final List<Target> targets = darwinArchs.map(_getNativeTarget).toList();
  final native_assets_cli.BuildMode buildModeCli = nativeAssetsBuildMode(buildMode);

  const OS targetOS = OS.iOS;
  final Uri buildUri = nativeAssetsBuildUri(projectUri, targetOS);
  final IOSSdk iosSdk = _getIOSSdk(environmentType);

  globals.logger.printTrace('Building native assets for $targets $buildModeCli.');
  final List<Asset> nativeAssets = <Asset>[];
  final Set<Uri> dependencies = <Uri>{};
  for (final Target target in targets) {
    final BuildResult result = await buildRunner.build(
      linkModePreference: LinkModePreference.dynamic,
      target: target,
      targetIOSSdk: iosSdk,
      buildMode: buildModeCli,
      workingDirectory: projectUri,
      includeParentEnvironment: true,
      cCompilerConfig: await buildRunner.cCompilerConfig,
    );
    ensureNativeAssetsBuildSucceed(result);
    nativeAssets.addAll(result.assets);
    dependencies.addAll(result.dependencies);
  }
  ensureNoLinkModeStatic(nativeAssets);
  globals.logger.printTrace('Building native assets for $targets done.');
  final Map<AssetPath, List<Asset>> fatAssetTargetLocations = _fatAssetTargetLocations(nativeAssets);
  await _copyNativeAssetsIOS(
    buildUri,
    fatAssetTargetLocations,
    codesignIdentity,
    buildMode,
    fileSystem,
  );

  final Map<Asset, Asset> assetTargetLocations = _assetTargetLocations(nativeAssets);
  await writeNativeAssetsYaml(
    assetTargetLocations.values,
    yamlParentDirectory,
    fileSystem,
  );
  return dependencies.toList();
}

IOSSdk _getIOSSdk(EnvironmentType environmentType) {
  switch (environmentType) {
    case EnvironmentType.physical:
      return IOSSdk.iPhoneOs;
    case EnvironmentType.simulator:
      return IOSSdk.iPhoneSimulator;
  }
}

/// Extract the [Target] from a [DarwinArch].
Target _getNativeTarget(DarwinArch darwinArch) {
  switch (darwinArch) {
    case DarwinArch.armv7:
      return Target.iOSArm;
    case DarwinArch.arm64:
      return Target.iOSArm64;
    case DarwinArch.x86_64:
      return Target.iOSX64;
  }
}

Map<AssetPath, List<Asset>> _fatAssetTargetLocations(List<Asset> nativeAssets) {
  final Set<String> alreadyTakenNames = <String>{};
  final Map<AssetPath, List<Asset>> result = <AssetPath, List<Asset>>{};
  for (final Asset asset in nativeAssets) {
    final AssetPath path = _targetLocationIOS(asset, alreadyTakenNames).path;
    result[path] ??= <Asset>[];
    result[path]!.add(asset);
  }
  return result;
}

Map<Asset, Asset> _assetTargetLocations(List<Asset> nativeAssets) {
  final Set<String> alreadyTakenNames = <String>{};
  return <Asset, Asset>{
    for (final Asset asset in nativeAssets)
      asset: _targetLocationIOS(asset, alreadyTakenNames),
  };
}

Asset _targetLocationIOS(Asset asset, Set<String> alreadyTakenNames) {
  final AssetPath path = asset.path;
  switch (path) {
    case AssetSystemPath _:
    case AssetInExecutable _:
    case AssetInProcess _:
      return asset;
    case AssetAbsolutePath _:
      final String fileName = path.uri.pathSegments.last;
      return asset.copyWith(
        path: AssetAbsolutePath(frameworkUri(fileName, alreadyTakenNames)),
      );
  }
  throw Exception(
      'Unsupported asset path type ${path.runtimeType} in asset $asset');
}

/// Copies native assets into a framework per dynamic library.
///
/// For `flutter run -release` a multi-architecture solution is needed. So,
/// `lipo` is used to combine all target architectures into a single file.
///
/// The install name is set so that it matches what the place it will
/// be bundled in the final app.
///
/// Code signing is also done here, so that it doesn't have to be done in
/// in xcode_backend.dart.
Future<void> _copyNativeAssetsIOS(
  Uri buildUri,
  Map<AssetPath, List<Asset>> assetTargetLocations,
  String? codesignIdentity,
  BuildMode buildMode,
  FileSystem fileSystem,
) async {
  if (assetTargetLocations.isNotEmpty) {
    globals.logger
        .printTrace('Copying native assets to ${buildUri.toFilePath()}.');
    for (final MapEntry<AssetPath, List<Asset>> assetMapping
        in assetTargetLocations.entries) {
      final Uri target = (assetMapping.key as AssetAbsolutePath).uri;
      final List<Uri> sources = <Uri>[
        for (final Asset source in assetMapping.value)
          (source.path as AssetAbsolutePath).uri
      ];
      final Uri targetUri = buildUri.resolveUri(target);
      final File dylibFile = fileSystem.file(targetUri);
      final Directory frameworkDir = dylibFile.parent;
      if (!await frameworkDir.exists()) {
        await frameworkDir.create(recursive: true);
      }
      await lipoDylibs(dylibFile, sources);
      await setInstallNameDylib(dylibFile);
      await createInfoPlist(targetUri.pathSegments.last, frameworkDir);
      await codesignDylib(codesignIdentity, buildMode, frameworkDir);
    }
    globals.logger.printTrace('Copying native assets done.');
  }
}
