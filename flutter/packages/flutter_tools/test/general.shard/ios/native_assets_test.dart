// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file/file.dart';
import 'package:file/memory.dart';
import 'package:file_testing/file_testing.dart';
import 'package:flutter_tools/src/artifacts.dart';
import 'package:flutter_tools/src/base/file_system.dart';
import 'package:flutter_tools/src/base/logger.dart';
import 'package:flutter_tools/src/base/platform.dart';
import 'package:flutter_tools/src/build_info.dart';
import 'package:flutter_tools/src/build_system/build_system.dart';
import 'package:flutter_tools/src/features.dart';
import 'package:flutter_tools/src/globals.dart' as globals;
import 'package:flutter_tools/src/ios/native_assets.dart';
import 'package:native_assets_cli/native_assets_cli.dart' hide BuildMode, Target;
import 'package:native_assets_cli/native_assets_cli.dart' as native_assets_cli;
import 'package:package_config/package_config_types.dart';

import '../../src/common.dart';
import '../../src/context.dart';
import '../../src/fakes.dart';
import '../fake_native_assets_build_runner.dart';

void main() {
  late FakeProcessManager processManager;
  late Environment environment;
  late Artifacts artifacts;
  late FileSystem fileSystem;
  late BufferLogger logger;
  late Uri projectUri;

  setUp(() {
    processManager = FakeProcessManager.empty();
    logger = BufferLogger.test();
    artifacts = Artifacts.test();
    fileSystem = MemoryFileSystem.test();
    environment = Environment.test(
      fileSystem.currentDirectory,
      inputs: <String, String>{},
      artifacts: artifacts,
      processManager: processManager,
      fileSystem: fileSystem,
      logger: logger,
    );
    environment.buildDir.createSync(recursive: true);
    projectUri = environment.projectDir.uri;
  });

  testUsingContext('dry run with no package config', overrides: <Type, Generator>{
    ProcessManager: () => FakeProcessManager.empty(),
  }, () async {
    expect(
      await dryRunNativeAssetsIOS(
        projectUri: projectUri,
        fileSystem: fileSystem,
        buildRunner: FakeNativeAssetsBuildRunner(
          hasPackageConfigResult: false,
        ),
      ),
      null,
    );
    expect(
      (globals.logger as BufferLogger).traceText,
      contains('No package config found. Skipping native assets compilation.'),
    );
  });

  testUsingContext('build with no package config', overrides: <Type, Generator>{
    ProcessManager: () => FakeProcessManager.empty(),
  }, () async {
    await buildNativeAssetsIOS(
      darwinArchs: <DarwinArch>[DarwinArch.arm64],
      environmentType: EnvironmentType.simulator,
      projectUri: projectUri,
      buildMode: BuildMode.debug,
      fileSystem: fileSystem,
      yamlParentDirectory: environment.buildDir.uri,
      buildRunner: FakeNativeAssetsBuildRunner(
        hasPackageConfigResult: false,
      ),
    );
    expect(
      (globals.logger as BufferLogger).traceText,
      contains('No package config found. Skipping native assets compilation.'),
    );
  });

  testUsingContext('dry run with assets but not enabled', overrides: <Type, Generator>{
    ProcessManager: () => FakeProcessManager.empty(),
  }, () async {
    final File packageConfig = environment.projectDir.childFile('.dart_tool/package_config.json');
    await packageConfig.parent.create();
    await packageConfig.create();
    expect(
      () => dryRunNativeAssetsIOS(
        projectUri: projectUri,
        fileSystem: fileSystem,
        buildRunner: FakeNativeAssetsBuildRunner(
          packagesWithNativeAssetsResult: <Package>[
            Package('bar', projectUri),
          ],
        ),
      ),
      throwsToolExit(
        message: 'Package(s) bar require the native assets feature to be enabled. '
            'Enable using `flutter config --enable-native-assets`.',
      ),
    );
  });

  testUsingContext('dry run with assets', overrides: <Type, Generator>{
    FeatureFlags: () => TestFeatureFlags(isNativeAssetsEnabled: true),
    ProcessManager: () => FakeProcessManager.empty(),
  }, () async {
    final File packageConfig = environment.projectDir.childFile('.dart_tool/package_config.json');
    await packageConfig.parent.create();
    await packageConfig.create();
    final Uri? nativeAssetsYaml = await dryRunNativeAssetsIOS(
      projectUri: projectUri,
      fileSystem: fileSystem,
      buildRunner: FakeNativeAssetsBuildRunner(
        packagesWithNativeAssetsResult: <Package>[
          Package('bar', projectUri),
        ],
        dryRunResult: FakeNativeAssetsBuilderResult(
          assets: <Asset>[
            Asset(
              id: 'package:bar/bar.dart',
              linkMode: LinkMode.dynamic,
              target: native_assets_cli.Target.macOSArm64,
              path: AssetAbsolutePath(Uri.file('bar.dylib')),
            ),
            Asset(
              id: 'package:bar/bar.dart',
              linkMode: LinkMode.dynamic,
              target: native_assets_cli.Target.macOSX64,
              path: AssetAbsolutePath(Uri.file('bar.dylib')),
            ),
          ],
        ),
      ),
    );
    expect(
      (globals.logger as BufferLogger).traceText,
      stringContainsInOrder(<String>[
        'Dry running native assets for ios.',
        'Dry running native assets for ios done.',
      ]),
    );
    expect(
      nativeAssetsYaml,
      projectUri.resolve('build/native_assets/ios/native_assets.yaml'),
    );
    expect(
      await fileSystem.file(nativeAssetsYaml).readAsString(),
      contains('package:bar/bar.dart'),
    );
  });

  testUsingContext('build with assets but not enabled', () async {
    final File packageConfig = environment.projectDir.childFile('.dart_tool/package_config.json');
    await packageConfig.parent.create();
    await packageConfig.create();
    expect(
      () => buildNativeAssetsIOS(
        darwinArchs: <DarwinArch>[DarwinArch.arm64],
        environmentType: EnvironmentType.simulator,
        projectUri: projectUri,
        buildMode: BuildMode.debug,
        fileSystem: fileSystem,
        yamlParentDirectory: environment.buildDir.uri,
        buildRunner: FakeNativeAssetsBuildRunner(
          packagesWithNativeAssetsResult: <Package>[
            Package('bar', projectUri),
          ],
        ),
      ),
      throwsToolExit(
        message: 'Package(s) bar require the native assets feature to be enabled. '
            'Enable using `flutter config --enable-native-assets`.',
      ),
    );
  });

  testUsingContext('build no assets', overrides: <Type, Generator>{
    FeatureFlags: () => TestFeatureFlags(isNativeAssetsEnabled: true),
    ProcessManager: () => FakeProcessManager.empty(),
  }, () async {
    final File packageConfig = environment.projectDir.childFile('.dart_tool/package_config.json');
    await packageConfig.parent.create();
    await packageConfig.create();
    await buildNativeAssetsIOS(
      darwinArchs: <DarwinArch>[DarwinArch.arm64],
      environmentType: EnvironmentType.simulator,
      projectUri: projectUri,
      buildMode: BuildMode.debug,
      fileSystem: fileSystem,
      yamlParentDirectory: environment.buildDir.uri,
      buildRunner: FakeNativeAssetsBuildRunner(
        packagesWithNativeAssetsResult: <Package>[
          Package('bar', projectUri),
        ],
      ),
    );
    expect(
      environment.buildDir.childFile('native_assets.yaml'),
      exists,
    );
  });

  testUsingContext('build with assets', overrides: <Type, Generator>{
    FeatureFlags: () => TestFeatureFlags(isNativeAssetsEnabled: true),
    ProcessManager: () => FakeProcessManager.list(
      <FakeCommand>[
        const FakeCommand(
          command: <Pattern>[
            'lipo',
            '-create',
            '-output',
            '/build/native_assets/ios/bar.dylib',
            'bar.dylib',
          ],
        ),
        const FakeCommand(
          command: <Pattern>[
            'install_name_tool',
            '-id',
            '@executable_path/Frameworks/bar.dylib',
            '/build/native_assets/ios/bar.dylib',
          ],
        ),
        const FakeCommand(
          command: <Pattern>[
            'codesign',
            '--force',
            '--sign',
            '-',
            '--timestamp=none',
            '/build/native_assets/ios/bar.dylib',
          ],
        ),
      ],
    ),
  }, () async {
    if (const LocalPlatform().isWindows) {
      return; // Backslashes in commands, but we will never run these commands on Windows.
    }
    final File packageConfig = environment.projectDir.childFile('.dart_tool/package_config.json');
    await packageConfig.parent.create();
    await packageConfig.create();
    await buildNativeAssetsIOS(
      darwinArchs: <DarwinArch>[DarwinArch.arm64],
      environmentType: EnvironmentType.simulator,
      projectUri: projectUri,
      buildMode: BuildMode.debug,
      fileSystem: fileSystem,
      yamlParentDirectory: environment.buildDir.uri,
      buildRunner: FakeNativeAssetsBuildRunner(
        packagesWithNativeAssetsResult: <Package>[
          Package('bar', projectUri),
        ],
        buildResult: FakeNativeAssetsBuilderResult(
          assets: <Asset>[
            Asset(
              id: 'package:bar/bar.dart',
              linkMode: LinkMode.dynamic,
              target: native_assets_cli.Target.iOSArm64,
              path: AssetAbsolutePath(Uri.file('bar.dylib')),
            ),
          ],
        ),
      ),
    );
    expect(
      (globals.logger as BufferLogger).traceText,
      stringContainsInOrder(<String>[
        'Building native assets for [ios_arm64] debug.',
        'Building native assets for [ios_arm64] done.',
      ]),
    );
    expect(
      environment.buildDir.childFile('native_assets.yaml'),
      exists,
    );
  });
}
