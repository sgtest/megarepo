// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Logic for native assets shared between all host OSes.

import 'package:logging/logging.dart' as logging;
import 'package:native_assets_builder/native_assets_builder.dart' hide NativeAssetsBuildRunner;
import 'package:native_assets_builder/native_assets_builder.dart' as native_assets_builder show NativeAssetsBuildRunner;
import 'package:native_assets_cli/native_assets_cli.dart';
import 'package:package_config/package_config_types.dart';

import 'base/common.dart';
import 'base/file_system.dart';
import 'base/logger.dart';
import 'base/platform.dart';
import 'build_info.dart' as build_info;
import 'cache.dart';
import 'features.dart';
import 'globals.dart' as globals;
import 'ios/native_assets.dart';
import 'macos/native_assets.dart';
import 'macos/native_assets_host.dart';
import 'resident_runner.dart';

/// Programmatic API to be used by Dart launchers to invoke native builds.
///
/// It enables mocking `package:native_assets_builder` package.
/// It also enables mocking native toolchain discovery via [cCompilerConfig].
abstract class NativeAssetsBuildRunner {
  /// Whether the project has a `.dart_tools/package_config.json`.
  ///
  /// If there is no package config, [packagesWithNativeAssets], [build], and
  /// [dryRun] must not be invoked.
  Future<bool> hasPackageConfig();

  /// All packages in the transitive dependencies that have a `build.dart`.
  Future<List<Package>> packagesWithNativeAssets();

  /// Runs all [packagesWithNativeAssets] `build.dart` in dry run.
  Future<DryRunResult> dryRun({
    required bool includeParentEnvironment,
    required LinkModePreference linkModePreference,
    required OS targetOs,
    required Uri workingDirectory,
  });

  /// Runs all [packagesWithNativeAssets] `build.dart`.
  Future<BuildResult> build({
    required bool includeParentEnvironment,
    required BuildMode buildMode,
    required LinkModePreference linkModePreference,
    required Target target,
    required Uri workingDirectory,
    CCompilerConfig? cCompilerConfig,
    int? targetAndroidNdkApi,
    IOSSdk? targetIOSSdk,
  });

  /// The C compiler config to use for compilation.
  Future<CCompilerConfig> get cCompilerConfig;
}

/// Uses `package:native_assets_builder` for its implementation.
class NativeAssetsBuildRunnerImpl implements NativeAssetsBuildRunner {
  NativeAssetsBuildRunnerImpl(
    this.projectUri,
    this.packageConfig,
    this.fileSystem,
    this.logger,
  );

  final Uri projectUri;
  final PackageConfig packageConfig;
  final FileSystem fileSystem;
  final Logger logger;

  late final logging.Logger _logger = logging.Logger('')
    ..onRecord.listen((logging.LogRecord record) {
      final int levelValue = record.level.value;
      final String message = record.message;
      if (levelValue >= logging.Level.SEVERE.value) {
        logger.printError(message);
      } else if (levelValue >= logging.Level.WARNING.value) {
        logger.printWarning(message);
      } else if (levelValue >= logging.Level.INFO.value) {
        logger.printTrace(message);
      } else {
        logger.printTrace(message);
      }
    });

  late final Uri _dartExecutable = fileSystem.directory(Cache.flutterRoot).uri.resolve('bin/dart');

  late final native_assets_builder.NativeAssetsBuildRunner _buildRunner = native_assets_builder.NativeAssetsBuildRunner(
    logger: _logger,
    dartExecutable: _dartExecutable,
  );

  @override
  Future<bool> hasPackageConfig() {
    final File packageConfigJson = fileSystem
        .directory(projectUri.toFilePath())
        .childDirectory('.dart_tool')
        .childFile('package_config.json');
    return packageConfigJson.exists();
  }

  @override
  Future<List<Package>> packagesWithNativeAssets() async {
    final PackageLayout packageLayout = PackageLayout.fromPackageConfig(
      packageConfig,
      projectUri.resolve('.dart_tool/package_config.json'),
    );
    return packageLayout.packagesWithNativeAssets;
  }

  @override
  Future<DryRunResult> dryRun({
    required bool includeParentEnvironment,
    required LinkModePreference linkModePreference,
    required OS targetOs,
    required Uri workingDirectory,
  }) {
    final PackageLayout packageLayout = PackageLayout.fromPackageConfig(
      packageConfig,
      projectUri.resolve('.dart_tool/package_config.json'),
    );
    return _buildRunner.dryRun(
      includeParentEnvironment: includeParentEnvironment,
      linkModePreference: linkModePreference,
      targetOs: targetOs,
      workingDirectory: workingDirectory,
      packageLayout: packageLayout,
    );
  }

  @override
  Future<BuildResult> build({
    required bool includeParentEnvironment,
    required BuildMode buildMode,
    required LinkModePreference linkModePreference,
    required Target target,
    required Uri workingDirectory,
    CCompilerConfig? cCompilerConfig,
    int? targetAndroidNdkApi,
    IOSSdk? targetIOSSdk,
  }) {
    final PackageLayout packageLayout = PackageLayout.fromPackageConfig(
      packageConfig,
      projectUri.resolve('.dart_tool/package_config.json'),
    );
    return _buildRunner.build(
      buildMode: buildMode,
      cCompilerConfig: cCompilerConfig,
      includeParentEnvironment: includeParentEnvironment,
      linkModePreference: linkModePreference,
      target: target,
      targetAndroidNdkApi: targetAndroidNdkApi,
      targetIOSSdk: targetIOSSdk,
      workingDirectory: workingDirectory,
      packageLayout: packageLayout,
    );
  }

  @override
  late final Future<CCompilerConfig> cCompilerConfig = () {
    if (globals.platform.isMacOS || globals.platform.isIOS) {
      return cCompilerConfigMacOS();
    }
    throwToolExit(
      'Native assets feature not yet implemented for Linux, Windows and Android.',
    );
  }();
}

/// Write [assets] to `native_assets.yaml` in [yamlParentDirectory].
Future<Uri> writeNativeAssetsYaml(
  Iterable<Asset> assets,
  Uri yamlParentDirectory,
  FileSystem fileSystem,
) async {
  globals.logger.printTrace('Writing native_assets.yaml.');
  final String nativeAssetsDartContents = assets.toNativeAssetsFile();
  final Directory parentDirectory = fileSystem.directory(yamlParentDirectory);
  if (!await parentDirectory.exists()) {
    await parentDirectory.create(recursive: true);
  }
  final File nativeAssetsFile = parentDirectory.childFile('native_assets.yaml');
  await nativeAssetsFile.writeAsString(nativeAssetsDartContents);
  globals.logger.printTrace('Writing ${nativeAssetsFile.path} done.');
  return nativeAssetsFile.uri;
}

/// Select the native asset build mode for a given Flutter build mode.
BuildMode nativeAssetsBuildMode(build_info.BuildMode buildMode) {
  switch (buildMode) {
    case build_info.BuildMode.debug:
      return BuildMode.debug;
    case build_info.BuildMode.jitRelease:
    case build_info.BuildMode.profile:
    case build_info.BuildMode.release:
      return BuildMode.release;
  }
}

/// Checks whether this project does not yet have a package config file.
///
/// A project has no package config when `pub get` has not yet been run.
///
/// Native asset builds cannot be run without a package config. If there is
/// no package config, leave a logging trace about that.
Future<bool> hasNoPackageConfig(NativeAssetsBuildRunner buildRunner) async {
  final bool packageConfigExists = await buildRunner.hasPackageConfig();
  if (!packageConfigExists) {
    globals.logger.printTrace('No package config found. Skipping native assets compilation.');
  }
  return !packageConfigExists;
}

/// Checks that if native assets is disabled, none of the dependencies declare
/// native assets.
///
/// If any of the dependencies have native assets, but native assets are
/// disabled, exits the tool.
Future<bool> isDisabledAndNoNativeAssets(NativeAssetsBuildRunner buildRunner) async {
  if (featureFlags.isNativeAssetsEnabled) {
    return false;
  }
  final List<Package> packagesWithNativeAssets = await buildRunner.packagesWithNativeAssets();
  if (packagesWithNativeAssets.isEmpty) {
    return true;
  }
  final String packageNames = packagesWithNativeAssets.map((Package p) => p.name).join(' ');
  throwToolExit(
    'Package(s) $packageNames require the native assets feature to be enabled. '
    'Enable using `flutter config --enable-native-assets`.',
  );
}

/// Ensures that either this project has no native assets, or that native assets
/// are supported on that operating system.
///
/// Exits the tool if the above condition is not satisfied.
Future<void> ensureNoNativeAssetsOrOsIsSupported(
  Uri workingDirectory,
  String os,
  FileSystem fileSystem,
  NativeAssetsBuildRunner buildRunner,
) async {
  if (await hasNoPackageConfig(buildRunner)) {
    return;
  }
  final List<Package> packagesWithNativeAssets = await buildRunner.packagesWithNativeAssets();
  if (packagesWithNativeAssets.isEmpty) {
    return;
  }
  final String packageNames = packagesWithNativeAssets.map((Package p) => p.name).join(' ');
  throwToolExit(
    'Package(s) $packageNames require the native assets feature. '
    'This feature has not yet been implemented for `$os`. '
    'For more info see https://github.com/flutter/flutter/issues/129757.',
  );
}

/// Ensure all native assets have a linkmode declared to be dynamic loading.
///
/// In JIT, the link mode must always be dynamic linking.
/// In AOT, the static linking has not yet been implemented in Dart:
/// https://github.com/dart-lang/sdk/issues/49418.
///
/// Therefore, ensure all `build.dart` scripts return only dynamic libraries.
void ensureNoLinkModeStatic(List<Asset> nativeAssets) {
  final Iterable<Asset> staticAssets = nativeAssets.whereLinkMode(LinkMode.static);
  if (staticAssets.isNotEmpty) {
    final String assetIds = staticAssets.map((Asset a) => a.id).toSet().join(', ');
    throwToolExit(
      'Native asset(s) $assetIds have their link mode set to static, '
      'but this is not yet supported. '
      'For more info see https://github.com/dart-lang/sdk/issues/49418.',
    );
  }
}

/// This should be the same for different archs, debug/release, etc.
/// It should work for all macOS.
Uri nativeAssetsBuildUri(Uri projectUri, OS os) {
  final String buildDir = build_info.getBuildDirectory();
  return projectUri.resolve('$buildDir/native_assets/$os/');
}

/// Gets the native asset id to dylib mapping to embed in the kernel file.
///
/// Run hot compiles a kernel file that is pushed to the device after hot
/// restart. We need to embed the native assets mapping in order to access
/// native assets after hot restart.
Future<Uri?> dryRunNativeAssets({
  required Uri projectUri,
  required FileSystem fileSystem,
  required NativeAssetsBuildRunner buildRunner,
  required List<FlutterDevice> flutterDevices,
}) async {
  if (flutterDevices.length != 1) {
    return dryRunNativeAssetsMultipeOSes(
      projectUri: projectUri,
      fileSystem: fileSystem,
      targetPlatforms: flutterDevices.map((FlutterDevice d) => d.targetPlatform).nonNulls,
      buildRunner: buildRunner,
    );
  }
  final FlutterDevice flutterDevice = flutterDevices.single;
  final build_info.TargetPlatform targetPlatform = flutterDevice.targetPlatform!;

  final Uri? nativeAssetsYaml;
  switch (targetPlatform) {
    case build_info.TargetPlatform.darwin:
      nativeAssetsYaml = await dryRunNativeAssetsMacOS(
        projectUri: projectUri,
        fileSystem: fileSystem,
        buildRunner: buildRunner,
      );
    case build_info.TargetPlatform.ios:
      nativeAssetsYaml = await dryRunNativeAssetsIOS(
        projectUri: projectUri,
        fileSystem: fileSystem,
        buildRunner: buildRunner,
      );
    case build_info.TargetPlatform.tester:
      if (const LocalPlatform().isMacOS) {
        nativeAssetsYaml = await dryRunNativeAssetsMacOS(
          projectUri: projectUri,
          flutterTester: true,
          fileSystem: fileSystem,
          buildRunner: buildRunner,
        );
      } else {
        await ensureNoNativeAssetsOrOsIsSupported(
          projectUri,
          const LocalPlatform().operatingSystem,
          fileSystem,
          buildRunner,
        );
        nativeAssetsYaml = null;
      }
    case build_info.TargetPlatform.android_arm:
    case build_info.TargetPlatform.android_arm64:
    case build_info.TargetPlatform.android_x64:
    case build_info.TargetPlatform.android_x86:
    case build_info.TargetPlatform.android:
    case build_info.TargetPlatform.fuchsia_arm64:
    case build_info.TargetPlatform.fuchsia_x64:
    case build_info.TargetPlatform.linux_arm64:
    case build_info.TargetPlatform.linux_x64:
    case build_info.TargetPlatform.web_javascript:
    case build_info.TargetPlatform.windows_x64:
      await ensureNoNativeAssetsOrOsIsSupported(
        projectUri,
        targetPlatform.toString(),
        fileSystem,
        buildRunner,
      );
      nativeAssetsYaml = null;
  }
  return nativeAssetsYaml;
}

/// Dry run the native builds for multiple OSes.
///
/// Needed for `flutter run -d all`.
Future<Uri?> dryRunNativeAssetsMultipeOSes({
  required NativeAssetsBuildRunner buildRunner,
  required Uri projectUri,
  required FileSystem fileSystem,
  required Iterable<build_info.TargetPlatform> targetPlatforms,
}) async {
  if (await hasNoPackageConfig(buildRunner) || await isDisabledAndNoNativeAssets(buildRunner)) {
    return null;
  }

  final Uri buildUri_ = buildUriMultiple(projectUri);
  final Iterable<Asset> nativeAssetPaths = <Asset>[
    if (targetPlatforms.contains(build_info.TargetPlatform.darwin) ||
        (targetPlatforms.contains(build_info.TargetPlatform.tester) && OS.current == OS.macOS))
      ...await dryRunNativeAssetsMacOSInternal(fileSystem, projectUri, false, buildRunner),
    if (targetPlatforms.contains(build_info.TargetPlatform.ios)) ...await dryRunNativeAssetsIOSInternal(fileSystem, projectUri, buildRunner)
  ];
  final Uri nativeAssetsUri = await writeNativeAssetsYaml(nativeAssetPaths, buildUri_, fileSystem);
  return nativeAssetsUri;
}

/// With `flutter run -d all` we need a place to store the native assets
/// mapping for multiple OSes combined.
Uri buildUriMultiple(Uri projectUri) {
  final String buildDir = build_info.getBuildDirectory();
  return projectUri.resolve('$buildDir/native_assets/multiple/');
}
