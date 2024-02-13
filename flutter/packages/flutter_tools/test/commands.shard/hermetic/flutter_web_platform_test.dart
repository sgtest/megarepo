// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file/file.dart';
import 'package:file/memory.dart';
import 'package:flutter_tools/src/artifacts.dart';
import 'package:flutter_tools/src/base/logger.dart';
import 'package:flutter_tools/src/base/platform.dart';
import 'package:flutter_tools/src/build_info.dart';
import 'package:flutter_tools/src/test/flutter_web_platform.dart';
import 'package:flutter_tools/src/web/chrome.dart';
import 'package:flutter_tools/src/web/compile.dart';
import 'package:flutter_tools/src/web/memory_fs.dart';
import 'package:shelf/shelf.dart' as shelf;
import 'package:test/test.dart';

import '../../src/context.dart';
import '../../src/fakes.dart';

class MockServer implements shelf.Server {
  shelf.Handler? mountedHandler;

  @override
  Future<void> close() async {}

  @override
  void mount(shelf.Handler handler) {
    mountedHandler = handler;
  }

  @override
  Uri get url => Uri.parse('');
}

void main() {
  late FileSystem fileSystem;
  late BufferLogger logger;
  late Platform platform;
  late Artifacts artifacts;
  late ProcessManager processManager;
  late FakeOperatingSystemUtils operatingSystemUtils;

  setUp(() {
    fileSystem = MemoryFileSystem.test();
    logger = BufferLogger.test();
    platform = FakePlatform();
    artifacts = Artifacts.test(fileSystem: fileSystem);
    processManager = FakeProcessManager.empty();
    operatingSystemUtils = FakeOperatingSystemUtils();

    for (final HostArtifact artifact in <HostArtifact>[
      HostArtifact.webPrecompiledCanvaskitAndHtmlSoundSdk,
      HostArtifact.webPrecompiledCanvaskitAndHtmlSdk,
      HostArtifact.webPrecompiledCanvaskitSoundSdk,
      HostArtifact.webPrecompiledCanvaskitSdk,
      HostArtifact.webPrecompiledSoundSdk,
      HostArtifact.webPrecompiledSdk,
    ]) {
      final File artifactFile = artifacts.getHostArtifact(artifact) as File;
      artifactFile.createSync();
      artifactFile.writeAsStringSync(artifact.name);
    }
  });

  testUsingContext('FlutterWebPlatform serves the correct dart_sdk.js for the passed web renderer', () async {
    final ChromiumLauncher chromiumLauncher = ChromiumLauncher(
      fileSystem: fileSystem,
      platform: platform,
      processManager: processManager,
      operatingSystemUtils: operatingSystemUtils,
      browserFinder: (Platform platform, FileSystem filesystem) => 'chrome',
      logger: logger,
    );
    final MockServer server = MockServer();
    fileSystem.directory('/test').createSync();
    final FlutterWebPlatform webPlatform = await FlutterWebPlatform.start(
      'ProjectRoot',
      buildInfo: const BuildInfo(
        BuildMode.debug,
        '',
        treeShakeIcons: false
      ),
      webMemoryFS: WebMemoryFS(),
      fileSystem: fileSystem,
      logger: logger,
      chromiumLauncher: chromiumLauncher,
      artifacts: artifacts,
      processManager: processManager,
      webRenderer: WebRendererMode.canvaskit,
      serverFactory: () async => server,
      testPackageUri: Uri.parse('test'),
    );
    final shelf.Handler? handler = server.mountedHandler;
    expect(handler, isNotNull);
    handler!;
    final shelf.Response response = await handler(shelf.Request(
      'GET',
      Uri.parse('http://localhost/dart_sdk.js'),
    ));
    final String contents = await response.readAsString();
    expect(contents, HostArtifact.webPrecompiledCanvaskitSoundSdk.name);
    await webPlatform.close();
  }, overrides: <Type, Generator>{
    FileSystem: () => fileSystem,
    ProcessManager: () => processManager,
    Logger: () => logger,
  });
}
