// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/src/gestures/monodrag.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:leak_tracker_flutter_testing/leak_tracker_flutter_testing.dart';

import 'two_dimensional_utils.dart';

Widget? _testChildBuilder(BuildContext context, ChildVicinity vicinity) {
  return SizedBox(
    height: 200,
    width: 200,
    child: Center(child: Text('C${vicinity.xIndex}:R${vicinity.yIndex}')),
  );
}

void main() {
  group('TwoDimensionalScrollView',() {
    testWidgetsWithLeakTracking('asserts the axis directions do not conflict with one another', (WidgetTester tester) async {
      final List<Object> exceptions = <Object>[];
      final FlutterExceptionHandler? oldHandler = FlutterError.onError;
      FlutterError.onError = (FlutterErrorDetails details) {
        exceptions.add(details.exception);
      };
      // Horizontal wrong
      late final TwoDimensionalChildBuilderDelegate delegate1;
      addTearDown(() => delegate1.dispose());
      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          delegate: delegate1 = TwoDimensionalChildBuilderDelegate(builder: (_, __) => null),
          horizontalDetails: const ScrollableDetails.vertical(),
          // Horizontal has default const ScrollableDetails.horizontal()
        ),
      ));

      // Vertical wrong
      late final TwoDimensionalChildBuilderDelegate delegate2;
      addTearDown(() => delegate2.dispose());
      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          delegate: delegate2 = TwoDimensionalChildBuilderDelegate(builder: (_, __) => null),
          verticalDetails: const ScrollableDetails.horizontal(),
          // Horizontal has default const ScrollableDetails.horizontal()
        ),
      ));

      // Both wrong
      late final TwoDimensionalChildBuilderDelegate delegate3;
      addTearDown(() => delegate3.dispose());
      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          delegate: delegate3 = TwoDimensionalChildBuilderDelegate(builder: (_, __) => null),
          verticalDetails: const ScrollableDetails.horizontal(),
          horizontalDetails: const ScrollableDetails.vertical(),
        ),
      ));

      FlutterError.onError = oldHandler;
      expect(exceptions.length, 3);
      for (final Object exception in exceptions) {
        expect(exception, isAssertionError);
        expect((exception as AssertionError).message, contains('are not Axis'));
      }
    }, variant: TargetPlatformVariant.all());

    testWidgetsWithLeakTracking('ScrollableDetails.controller can set initial scroll positions, modify within bounds', (WidgetTester tester) async {
      final ScrollController verticalController = ScrollController(initialScrollOffset: 100);
      addTearDown(verticalController.dispose);
      final ScrollController horizontalController = ScrollController(initialScrollOffset: 50);
      addTearDown(horizontalController.dispose);
      late final TwoDimensionalChildBuilderDelegate delegate;
      addTearDown(() => delegate.dispose());

      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          verticalDetails: ScrollableDetails.vertical(controller: verticalController),
          horizontalDetails: ScrollableDetails.horizontal(controller: horizontalController),
          delegate: delegate = TwoDimensionalChildBuilderDelegate(
            builder: _testChildBuilder,
            maxXIndex: 99,
            maxYIndex: 99,
          ),
        ),
      ));
      await tester.pumpAndSettle();

      expect(verticalController.position.pixels, 100);
      expect(verticalController.position.maxScrollExtent, 19400);
      expect(horizontalController.position.pixels, 50);
      expect(horizontalController.position.maxScrollExtent, 19200);

      verticalController.jumpTo(verticalController.position.maxScrollExtent);
      horizontalController.jumpTo(horizontalController.position.maxScrollExtent);
      await tester.pump();

      expect(verticalController.position.pixels, 19400);
      expect(horizontalController.position.pixels, 19200);

      // Out of bounds
      verticalController.jumpTo(verticalController.position.maxScrollExtent + 100);
      horizontalController.jumpTo(horizontalController.position.maxScrollExtent + 100);
      // Account for varying scroll physics for different platforms (overscroll)
      await tester.pumpAndSettle();

      expect(verticalController.position.pixels, 19400);
      expect(horizontalController.position.pixels, 19200);
    }, variant: TargetPlatformVariant.all());

    testWidgetsWithLeakTracking('Properly assigns the PrimaryScrollController to the main axis on the correct platform', (WidgetTester tester) async {
      late ScrollController controller;
      Widget buildForPrimaryScrollController({
        bool? explicitPrimary,
        Axis mainAxis = Axis.vertical,
        bool addControllerConflict = false,
      }) {
        final ScrollController verticalController = ScrollController();
        addTearDown(verticalController.dispose);
        final ScrollController horizontalController = ScrollController();
        addTearDown(horizontalController.dispose);
        late final TwoDimensionalChildBuilderDelegate delegate;
        addTearDown(() => delegate.dispose());

        return MaterialApp(
          home: PrimaryScrollController(
            controller: controller,
            child: SimpleBuilderTableView(
              mainAxis: mainAxis,
              primary: explicitPrimary,
              verticalDetails: ScrollableDetails.vertical(
                controller: addControllerConflict && mainAxis == Axis.vertical
                  ? verticalController
                  : null
              ),
              horizontalDetails: ScrollableDetails.horizontal(
                controller: addControllerConflict && mainAxis == Axis.horizontal
                  ? horizontalController
                  : null
              ),
              delegate: delegate = TwoDimensionalChildBuilderDelegate(
                builder: _testChildBuilder,
                maxXIndex: 99,
                maxYIndex: 99,
              ),
            ),
          ),
        );
      }

      // Horizontal default - horizontal never automatically adopts PSC
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        mainAxis: Axis.horizontal,
      ));
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isFalse);
      }

      // Horizontal explicitly true
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        mainAxis: Axis.horizontal,
        explicitPrimary: true,
      ));
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
        // Primary explicitly true is always adopted.
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isTrue);
          expect(controller.position.axis, Axis.horizontal);
      }

      // Horizontal explicitly false
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        mainAxis: Axis.horizontal,
        explicitPrimary: false,
      ));
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
      // Primary explicitly false is never adopted.
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isFalse);
      }

      // Vertical default
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController());
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
      // Mobile platforms inherit the PSC without explicitly setting
      // primary
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
          expect(controller.hasClients, isTrue);
          expect(controller.position.axis, Axis.vertical);
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isFalse);
      }

      // Vertical explicitly true
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        explicitPrimary: true,
      ));
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
        // Primary explicitly true is always adopted.
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isTrue);
          expect(controller.position.axis, Axis.vertical);
      }

      // Vertical explicitly false
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        explicitPrimary: false,
      ));
      await tester.pumpAndSettle();

      switch (defaultTargetPlatform) {
      // Primary explicitly false is never adopted.
        case TargetPlatform.android:
        case TargetPlatform.fuchsia:
        case TargetPlatform.iOS:
        case TargetPlatform.linux:
        case TargetPlatform.macOS:
        case TargetPlatform.windows:
          expect(controller.hasClients, isFalse);
      }

      // Assertions
      final List<Object> exceptions = <Object>[];
      final FlutterExceptionHandler? oldHandler = FlutterError.onError;
      FlutterError.onError = (FlutterErrorDetails details) {
        exceptions.add(details.exception);
      };
      // Vertical asserts ScrollableDetails.controller has not been provided if
      // primary is explicitly set
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        explicitPrimary: true,
        addControllerConflict: true,
      ));
      expect(exceptions.length, 1);
      expect(exceptions[0], isAssertionError);
      expect(
        (exceptions[0] as AssertionError).message,
        contains('TwoDimensionalScrollView.primary was explicitly set to true'),
      );
      exceptions.clear();

      // Horizontal asserts ScrollableDetails.controller has not been provided
      // if primary is explicitly set true
      controller = ScrollController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(buildForPrimaryScrollController(
        mainAxis: Axis.horizontal,
        explicitPrimary: true,
        addControllerConflict: true,
      ));
      expect(exceptions.length, 1);
      expect(exceptions[0], isAssertionError);
      expect(
        (exceptions[0] as AssertionError).message,
        contains('TwoDimensionalScrollView.primary was explicitly set to true'),
      );
      FlutterError.onError = oldHandler;
    }, variant: TargetPlatformVariant.all());

    testWidgetsWithLeakTracking('TwoDimensionalScrollable receives the correct details from TwoDimensionalScrollView', (WidgetTester tester) async {
      late BuildContext capturedContext;
      // Default
      late final TwoDimensionalChildBuilderDelegate delegate1;
      addTearDown(() => delegate1.dispose());
      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          delegate: delegate1 = TwoDimensionalChildBuilderDelegate(
            builder: (BuildContext context, ChildVicinity vicinity) {
              capturedContext = context;
              return Text(vicinity.toString());
            },
          ),
        ),
      ));
      await tester.pumpAndSettle();
      TwoDimensionalScrollableState scrollable = TwoDimensionalScrollable.of(
        capturedContext,
      );
      expect(scrollable.widget.verticalDetails.direction, AxisDirection.down);
      expect(scrollable.widget.horizontalDetails.direction, AxisDirection.right);
      expect(scrollable.widget.diagonalDragBehavior, DiagonalDragBehavior.none);
      expect(scrollable.widget.dragStartBehavior, DragStartBehavior.start);

      // Customized
      late final TwoDimensionalChildBuilderDelegate delegate2;
      addTearDown(() => delegate2.dispose());
      await tester.pumpWidget(MaterialApp(
        home: SimpleBuilderTableView(
          verticalDetails: const ScrollableDetails.vertical(reverse: true),
          horizontalDetails: const ScrollableDetails.horizontal(reverse: true),
          diagonalDragBehavior: DiagonalDragBehavior.weightedContinuous,
          dragStartBehavior: DragStartBehavior.down,
          delegate: delegate2 = TwoDimensionalChildBuilderDelegate(
            builder: _testChildBuilder,
          ),
        ),
      ));
      await tester.pumpAndSettle();
      scrollable = TwoDimensionalScrollable.of(capturedContext);
      expect(scrollable.widget.verticalDetails.direction, AxisDirection.up);
      expect(scrollable.widget.horizontalDetails.direction, AxisDirection.left);
      expect(scrollable.widget.diagonalDragBehavior, DiagonalDragBehavior.weightedContinuous);
      expect(scrollable.widget.dragStartBehavior, DragStartBehavior.down);
    }, variant: TargetPlatformVariant.all());
  });
}
