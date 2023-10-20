// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file/memory.dart';
import 'package:file_testing/file_testing.dart';
import 'package:flutter_tools/src/artifacts.dart';
import 'package:flutter_tools/src/base/file_system.dart';
import 'package:flutter_tools/src/base/logger.dart';
import 'package:flutter_tools/src/build_info.dart';
import 'package:flutter_tools/src/build_system/build_system.dart';
import 'package:flutter_tools/src/build_system/exceptions.dart';
import 'package:flutter_tools/src/build_system/targets/native_assets.dart';
import 'package:flutter_tools/src/features.dart';
import 'package:flutter_tools/src/native_assets.dart';
import 'package:native_assets_cli/native_assets_cli.dart' as native_assets_cli;
import 'package:package_config/package_config.dart' show Package;

import '../../../src/common.dart';
import '../../../src/context.dart';
import '../../../src/fakes.dart';
import '../../fake_native_assets_build_runner.dart';

void main() {
  late FakeProcessManager processManager;
  late Environment iosEnvironment;
  late Artifacts artifacts;
  late FileSystem fileSystem;
  late Logger logger;

  setUp(() {
    processManager = FakeProcessManager.empty();
    logger = BufferLogger.test();
    artifacts = Artifacts.test();
    fileSystem = MemoryFileSystem.test();
    iosEnvironment = Environment.test(
      fileSystem.currentDirectory,
      defines: <String, String>{
        kBuildMode: BuildMode.profile.cliName,
        kTargetPlatform: getNameForTargetPlatform(TargetPlatform.ios),
        kIosArchs: 'arm64',
        kSdkRoot: 'path/to/iPhoneOS.sdk',
      },
      inputs: <String, String>{},
      artifacts: artifacts,
      processManager: processManager,
      fileSystem: fileSystem,
      logger: logger,
    );
    iosEnvironment.buildDir.createSync(recursive: true);
  });

  testWithoutContext('NativeAssets throws error if missing target platform', () async {
    iosEnvironment.defines.remove(kTargetPlatform);
    expect(const NativeAssets().build(iosEnvironment), throwsA(isA<MissingDefineException>()));
  });

  testUsingContext('NativeAssets defaults to ios archs if missing', () async {
    await createPackageConfig(iosEnvironment);

    iosEnvironment.defines.remove(kIosArchs);

    final NativeAssetsBuildRunner buildRunner = FakeNativeAssetsBuildRunner();
    await NativeAssets(buildRunner: buildRunner).build(iosEnvironment);

    final File nativeAssetsYaml =
        iosEnvironment.buildDir.childFile('native_assets.yaml');
    final File depsFile = iosEnvironment.buildDir.childFile('native_assets.d');
    expect(depsFile, exists);
    expect(nativeAssetsYaml, exists);
  });

  testUsingContext('NativeAssets throws error if missing sdk root', () async {
    await createPackageConfig(iosEnvironment);

    iosEnvironment.defines.remove(kSdkRoot);
    expect(const NativeAssets().build(iosEnvironment), throwsA(isA<MissingDefineException>()));
  });

  // The NativeAssets Target should _always_ be creating a yaml an d file.
  // The caching logic depends on this.
  for (final bool isNativeAssetsEnabled in <bool>[true, false]) {
    final String postFix = isNativeAssetsEnabled ? 'enabled' : 'disabled';
    testUsingContext(
      'Successfull native_assets.yaml and native_assets.d creation with feature $postFix',
      overrides: <Type, Generator>{
        FileSystem: () => fileSystem,
        ProcessManager: () => processManager,
        FeatureFlags: () => TestFeatureFlags(
          isNativeAssetsEnabled: isNativeAssetsEnabled,
        ),
      },
      () async {
        await createPackageConfig(iosEnvironment);

        final NativeAssetsBuildRunner buildRunner = FakeNativeAssetsBuildRunner();
        await NativeAssets(buildRunner: buildRunner).build(iosEnvironment);

        expect(iosEnvironment.buildDir.childFile('native_assets.d'), exists);
        expect(iosEnvironment.buildDir.childFile('native_assets.yaml'), exists);
      },
    );
  }

  testUsingContext(
    'NativeAssets with an asset',
    overrides: <Type, Generator>{
      FileSystem: () => fileSystem,
      ProcessManager: () => FakeProcessManager.any(),
      FeatureFlags: () => TestFeatureFlags(isNativeAssetsEnabled: true),
    },
    () async {
      await createPackageConfig(iosEnvironment);

      final NativeAssetsBuildRunner buildRunner = FakeNativeAssetsBuildRunner(
        packagesWithNativeAssetsResult: <Package>[Package('foo', iosEnvironment.buildDir.uri)],
        buildResult: FakeNativeAssetsBuilderResult(assets: <native_assets_cli.Asset>[
          native_assets_cli.Asset(
            id: 'package:foo/foo.dart',
            linkMode: native_assets_cli.LinkMode.dynamic,
            target: native_assets_cli.Target.iOSArm64,
            path: native_assets_cli.AssetAbsolutePath(
              Uri.file('libfoo.dylib'),
            ),
          )
        ], dependencies: <Uri>[
          Uri.file('src/foo.c'),
        ]),
      );
      await NativeAssets(buildRunner: buildRunner).build(iosEnvironment);

      final File nativeAssetsYaml = iosEnvironment.buildDir.childFile('native_assets.yaml');
      final File depsFile = iosEnvironment.buildDir.childFile('native_assets.d');
      expect(depsFile, exists);
      // We don't care about the specific format, but it should contain the
      // yaml as the file depending on the source files that went in to the
      // build.
      expect(
        depsFile.readAsStringSync(),
        stringContainsInOrder(<String>[
          nativeAssetsYaml.path,
          ':',
          'src/foo.c',
        ]),
      );
      expect(nativeAssetsYaml, exists);
      // We don't care about the specific format, but it should contain the
      // asset id and the path to the dylib.
      expect(
        nativeAssetsYaml.readAsStringSync(),
        stringContainsInOrder(<String>[
          'package:foo/foo.dart',
          'libfoo.dylib',
        ]),
      );
    },
  );
}

Future<void> createPackageConfig(Environment iosEnvironment) async {
  final File packageConfig = iosEnvironment.projectDir
      .childDirectory('.dart_tool')
      .childFile('package_config.json');
  await packageConfig.parent.create();
  await packageConfig.create();
}
