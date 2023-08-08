// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/src/foundation/constants.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:leak_tracker/leak_tracker.dart';

import '../../leak_tracking.dart';
import '../leaking_widget.dart';

const String test1TrackingOnNoLeaks = 'test1, tracking-on, no leaks';
const String test2TrackingOffLeaks = 'test2, tracking-off, leaks';
const String test3TrackingOnLeaks = 'test3, tracking-on, leaks';
const String test4TrackingOnWithStackTrace = 'test4, tracking-on, with stack trace';

bool get _isTrackingOn => !LeakTracking.phase.isPaused && LeakTracking.isStarted;

/// For these tests `expect` for found leaks happens in flutter_test_config.dart.
void main() {
  group('group', () {
    testWidgetsWithLeakTracking(test1TrackingOnNoLeaks, (WidgetTester widgetTester) async {
      expect(_isTrackingOn, true);
      expect(LeakTracking.phase.name, test1TrackingOnNoLeaks);
      await widgetTester.pumpWidget(Container());
    });

    testWidgets(test2TrackingOffLeaks, (WidgetTester widgetTester) async {
      expect(LeakTracking.phase.name, null);
      expect(_isTrackingOn, false);
      await widgetTester.pumpWidget(StatelessLeakingWidget());
    });
  },
  skip: kIsWeb); // [intended] Leak tracking is off for web.

  testWidgetsWithLeakTracking(test3TrackingOnLeaks, (WidgetTester widgetTester) async {
    expect(_isTrackingOn, true);
    expect(LeakTracking.phase.name, test3TrackingOnLeaks);
    await widgetTester.pumpWidget(StatelessLeakingWidget());
  },
  skip: kIsWeb); // [intended] Leak tracking is off for web.

  testWidgetsWithLeakTracking(
    test4TrackingOnWithStackTrace,
    (WidgetTester widgetTester) async {
      expect(_isTrackingOn, true);
      expect(LeakTracking.phase.name, test4TrackingOnWithStackTrace);
      await widgetTester.pumpWidget(StatelessLeakingWidget());
    },
    leakTrackingTestConfig: const LeakTrackingTestConfig(
      leakDiagnosticConfig: LeakDiagnosticConfig(
        collectStackTraceOnStart: true,
      ),
    ),
  skip: kIsWeb); // [intended] Leak tracking is off for web.
}
