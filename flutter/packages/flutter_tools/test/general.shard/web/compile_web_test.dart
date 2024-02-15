// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:file/memory.dart';
import 'package:flutter_tools/src/base/logger.dart';
import 'package:flutter_tools/src/build_info.dart';
import 'package:flutter_tools/src/build_system/build_system.dart';
import 'package:flutter_tools/src/build_system/targets/web.dart';
import 'package:flutter_tools/src/project.dart';
import 'package:flutter_tools/src/reporting/reporting.dart';
import 'package:flutter_tools/src/web/compile.dart';
import 'package:flutter_tools/src/web/file_generators/flutter_service_worker_js.dart';
import 'package:unified_analytics/unified_analytics.dart';

import '../../src/common.dart';
import '../../src/context.dart';
import '../../src/fakes.dart';
import '../../src/test_build_system.dart';

void main() {
  late MemoryFileSystem fileSystem;
  late TestUsage testUsage;
  late FakeAnalytics fakeAnalytics;
  late BufferLogger logger;
  late FakeFlutterVersion flutterVersion;
  late FlutterProject flutterProject;

  setUp(() {
    fileSystem = MemoryFileSystem.test();
    testUsage = TestUsage();
    logger = BufferLogger.test();
    flutterVersion = FakeFlutterVersion(frameworkVersion: '1.0.0', engineRevision: '9.8.7');
    fakeAnalytics = getInitializedFakeAnalyticsInstance(
      fs: fileSystem,
      fakeFlutterVersion: flutterVersion,
    );

    flutterProject = FlutterProject.fromDirectoryTest(fileSystem.currentDirectory);
    fileSystem.file('.packages').createSync();
  });

  testUsingContext('WebBuilder sets environment on success', () async {
    final TestBuildSystem buildSystem =
        TestBuildSystem.all(BuildResult(success: true), (Target target, Environment environment) {
      expect(target, isA<WebServiceWorker>());
      expect(environment.defines, <String, String>{
        'TargetFile': 'target',
        'HasWebPlugins': 'false',
        'ServiceWorkerStrategy': ServiceWorkerStrategy.offlineFirst.cliName,
        'BuildMode': 'debug',
        'DartObfuscation': 'false',
        'TrackWidgetCreation': 'true',
        'TreeShakeIcons': 'false',
      });

      expect(environment.engineVersion, '9.8.7');
      expect(environment.generateDartPluginRegistry, isFalse);
    });

    final WebBuilder webBuilder = WebBuilder(
      logger: logger,
      processManager: FakeProcessManager.any(),
      buildSystem: buildSystem,
      usage: testUsage,
      flutterVersion: flutterVersion,
      fileSystem: fileSystem,
      analytics: fakeAnalytics,
    );
    await webBuilder.buildWeb(
      flutterProject,
      'target',
      BuildInfo.debug,
      ServiceWorkerStrategy.offlineFirst,
      compilerConfigs: <WebCompilerConfig>[
        const WasmCompilerConfig(
          optimizationLevel: 0,
          stripWasm: false,
          renderer: WebRendererMode.skwasm,
        ),
        const JsCompilerConfig.run(
          nativeNullAssertions: true,
          renderer: WebRendererMode.canvaskit,
        ),
      ],
    );

    expect(logger.statusText, contains('Compiling target for the Web...'));
    expect(logger.errorText, isEmpty);
    // Runs ScrubGeneratedPluginRegistrant migrator.
    expect(
      logger.traceText,
      contains('generated_plugin_registrant.dart not found. Skipping.'),
    );

    // Sends build config event
    expect(
      testUsage.events,
      unorderedEquals(
        <TestUsageEvent>[
      const TestUsageEvent(
        'build',
        'web',
        label: 'web-compile',
            parameters: CustomDimensions(
              buildEventSettings:
                  'optimizationLevel: 4; web-renderer: skwasm,canvaskit; web-target: wasm,js;',

      ),
          ),
        ],
      ),
    );

    expect(
      fakeAnalytics.sentEvents,
      containsAll(<Event>[
        Event.flutterBuildInfo(
          label: 'web-compile',
          buildType: 'web',
          settings: 'optimizationLevel: 4; web-renderer: skwasm,canvaskit; web-target: wasm,js;',
        ),
      ]),
    );

    // Sends timing event.
    final TestTimingEvent timingEvent = testUsage.timings.single;
    expect(timingEvent.category, 'build');
    expect(timingEvent.variableName, 'dual-compile');
    expect(
      analyticsTimingEventExists(
        sentEvents: fakeAnalytics.sentEvents,
        workflow: 'build',
        variableName: 'dual-compile',
      ),
      true,
    );
  });

  testUsingContext('WebBuilder throws tool exit on failure', () async {
    final TestBuildSystem buildSystem = TestBuildSystem.all(BuildResult(
      success: false,
      exceptions: <String, ExceptionMeasurement>{
        'hello': ExceptionMeasurement(
          'hello',
          const FormatException('illegal character in input string'),
          StackTrace.current,
        ),
      },
    ));

    final WebBuilder webBuilder = WebBuilder(
      logger: logger,
      processManager: FakeProcessManager.any(),
      buildSystem: buildSystem,
      usage: testUsage,
      flutterVersion: flutterVersion,
      fileSystem: fileSystem,
      analytics: fakeAnalytics,
    );
    await expectLater(
        () async => webBuilder.buildWeb(
              flutterProject,
              'target',
              BuildInfo.debug,
              ServiceWorkerStrategy.offlineFirst,
              compilerConfigs: <WebCompilerConfig>[
                const JsCompilerConfig.run(nativeNullAssertions: true, renderer: WebRendererMode.auto),
              ]
            ),
        throwsToolExit(message: 'Failed to compile application for the Web.'));

    expect(logger.errorText, contains('Target hello failed: FormatException: illegal character in input string'));
    expect(testUsage.timings, isEmpty);
    expect(fakeAnalytics.sentEvents, isEmpty);
  });
}
