// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// This file is run as part of a reduced test set in CI on Mac and Windows
// machines.
@Tags(<String>['reduced-test-set'])
library;

import 'package:flutter/cupertino.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import 'package:flutter_test/flutter_test.dart';

import '../widgets/semantics_tester.dart';

void main() {
  testWidgets('Overall looks correctly under light theme', (WidgetTester tester) async {
    await tester.pumpWidget(
      TestScaffoldApp(
        theme: const CupertinoThemeData(brightness: Brightness.light),
        actionSheet: CupertinoActionSheet(
          message: const Text('The title'),
          actions: <Widget>[
            CupertinoActionSheetAction(child: const Text('One'), onPressed: () {}),
            CupertinoActionSheetAction(child: const Text('Two'), onPressed: () {}),
          ],
          cancelButton: CupertinoActionSheetAction(child: const Text('Cancel'), onPressed: () {}),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('One')));
    await tester.pumpAndSettle();
    // This golden file also verifies the structure of an action sheet that
    // has a message, no title, and no overscroll for any sections (in contrast
    // to cupertinoActionSheet.dark-theme.png).
    await expectLater(
      find.byType(CupertinoApp),
      matchesGoldenFile('cupertinoActionSheet.overall-light-theme.png'),
    );

    await gesture.up();
  });

  testWidgets('Overall looks correctly under dark theme', (WidgetTester tester) async {
    await tester.pumpWidget(
      TestScaffoldApp(
        theme: const CupertinoThemeData(brightness: Brightness.dark),
        actionSheet: CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message'),
          actions: List<Widget>.generate(20, (int i) =>
            CupertinoActionSheetAction(
              onPressed: () {},
              child: Text('Button $i'),
            ),
          ),
          cancelButton: CupertinoActionSheetAction(child: const Text('Cancel'), onPressed: () {}),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Button 0')));
    await tester.pumpAndSettle();
    // This golden file also verifies the structure of an action sheet that
    // has both a message and a title, and an overscrolled action section (in
    // contrast to cupertinoActionSheet.light-theme.png).
    await expectLater(
      find.byType(CupertinoApp),
      matchesGoldenFile('cupertinoActionSheet.overall-dark-theme.png'),
    );

    await gesture.up();
  });

  testWidgets('Verify that a tap on modal barrier dismisses an action sheet', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        const CupertinoActionSheet(
          title: Text('Action Sheet'),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(find.text('Action Sheet'), findsOneWidget);

    await tester.tapAt(const Offset(20.0, 20.0));
    await tester.pump();
    expect(find.text('Action Sheet'), findsNothing);
  });

  testWidgets('Verify that a tap on title section (not buttons) does not dismiss an action sheet', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        const CupertinoActionSheet(
          title: Text('Action Sheet'),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();
    await tester.pump(const Duration(seconds: 5));

    expect(find.text('Action Sheet'), findsOneWidget);

    await tester.tap(find.text('Action Sheet'));
    await tester.pump();
    expect(find.text('Action Sheet'), findsOneWidget);
  });

  testWidgets('Action sheet destructive text style', (WidgetTester tester) async {
    await tester.pumpWidget(
      boilerplate(
        CupertinoActionSheetAction(
          isDestructiveAction: true,
          child: const Text('Ok'),
          onPressed: () { },
        ),
      ),
    );

    final DefaultTextStyle widget = tester.widget(find.widgetWithText(DefaultTextStyle, 'Ok'));

    expect(widget.style.color, const CupertinoDynamicColor.withBrightnessAndContrast(
      color: Color.fromARGB(255, 255, 59, 48),
      darkColor: Color.fromARGB(255, 255, 69, 58),
      highContrastColor: Color.fromARGB(255, 215, 0, 21),
      darkHighContrastColor: Color.fromARGB(255, 255, 105, 97),
    ));
  });

  testWidgets('Action sheet dark mode', (WidgetTester tester) async {
    final Widget action = CupertinoActionSheetAction(
      child: const Text('action'),
      onPressed: () {},
    );

    Brightness brightness = Brightness.light;
    late StateSetter stateSetter;

    TextStyle actionTextStyle(String text) {
      return tester.widget<DefaultTextStyle>(
        find.descendant(
          of: find.widgetWithText(CupertinoActionSheetAction, text),
          matching: find.byType(DefaultTextStyle),
        ),
      ).style;
    }

    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        StatefulBuilder(
          builder: (BuildContext context, StateSetter setter) {
            stateSetter = setter;
            return CupertinoTheme(
              data: CupertinoThemeData(
                brightness: brightness,
                primaryColor: const CupertinoDynamicColor.withBrightnessAndContrast(
                  color: Color.fromARGB(255, 0, 122, 255),
                  darkColor: Color.fromARGB(255, 10, 132, 255),
                  highContrastColor: Color.fromARGB(255, 0, 64, 221),
                  darkHighContrastColor: Color.fromARGB(255, 64, 156, 255),
                ),
              ),
              child: CupertinoActionSheet(actions: <Widget>[action]),
            );
          },
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(
      actionTextStyle('action').color!.value,
      const Color.fromARGB(255, 0, 122, 255).value,
    );

    stateSetter(() { brightness = Brightness.dark; });
    await tester.pump();

    expect(
      actionTextStyle('action').color!.value,
      const Color.fromARGB(255, 10, 132, 255).value,
    );
  });

  testWidgets('Action sheet default text style', (WidgetTester tester) async {
    await tester.pumpWidget(
      boilerplate(
        CupertinoActionSheetAction(
          isDefaultAction: true,
          child: const Text('Ok'),
          onPressed: () { },
        ),
      ),
    );

    final DefaultTextStyle widget = tester.widget(find.widgetWithText(DefaultTextStyle, 'Ok'));

    expect(widget.style.fontWeight, equals(FontWeight.w600));
  });

  testWidgets('Action sheet text styles are correct when both title and message are included', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        const CupertinoActionSheet(
          title: Text('Action Sheet'),
          message: Text('An action sheet'),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    final DefaultTextStyle titleStyle = tester.firstWidget(find.widgetWithText(DefaultTextStyle, 'Action Sheet'));
    final DefaultTextStyle messageStyle = tester.firstWidget(find.widgetWithText(DefaultTextStyle, 'An action sheet'));

    expect(titleStyle.style.fontWeight, FontWeight.w600);
    expect(messageStyle.style.fontWeight, FontWeight.w400);
  });

  testWidgets('Action sheet text styles are correct when title but no message is included', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        const CupertinoActionSheet(
          title: Text('Action Sheet'),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    final DefaultTextStyle titleStyle = tester.firstWidget(find.widgetWithText(DefaultTextStyle, 'Action Sheet'));

    expect(titleStyle.style.fontWeight, FontWeight.w400);
  });

  testWidgets('Action sheet text styles are correct when message but no title is included', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        const CupertinoActionSheet(
          message: Text('An action sheet'),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    final DefaultTextStyle messageStyle = tester.firstWidget(find.widgetWithText(DefaultTextStyle, 'An action sheet'));

    expect(messageStyle.style.fontWeight, FontWeight.w600);
  });

  testWidgets('Content section but no actions', (WidgetTester tester) async {
    final ScrollController scrollController = ScrollController();
    addTearDown(scrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message.'),
          messageScrollController: scrollController,
        ),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    // Content section should be at the bottom left of action sheet
    // (minus padding).
    expect(
      tester.getBottomLeft(find.byType(ClipRRect)),
      tester.getBottomLeft(find.byType(CupertinoActionSheet)) - const Offset(-8.0, 10.0),
    );

    // Check that the dialog size is the same as the content section size
    // (minus padding).
    expect(
      tester.getSize(find.byType(ClipRRect)).height,
      tester.getSize(find.byType(CupertinoActionSheet)).height  - 20.0,
    );

    expect(
      tester.getSize(find.byType(ClipRRect)).width,
      tester.getSize(find.byType(CupertinoActionSheet)).width - 16.0,
    );
  });

  testWidgets('Actions but no content section', (WidgetTester tester) async {
    final ScrollController actionScrollController = ScrollController();
    addTearDown(actionScrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          actionScrollController: actionScrollController,
        ),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    final Finder finder = find.byElementPredicate(
      (Element element) {
        return element.widget.runtimeType.toString() == '_ActionSheetActionSection';
      },
    );

    // Check that the title/message section is not displayed (action section is
    // at the top of the action sheet + padding).
    expect(
      tester.getTopLeft(finder),
      tester.getTopLeft(find.byType(CupertinoActionSheet)) + const Offset(8.0, 10.0),
    );

    expect(
      tester.getTopLeft(find.byType(CupertinoActionSheet)) + const Offset(8.0, 10.0),
      tester.getTopLeft(find.widgetWithText(CupertinoActionSheetAction, 'One')),
    );
    expect(
      tester.getBottomLeft(find.byType(CupertinoActionSheet)) + const Offset(8.0, -10.0),
      tester.getBottomLeft(find.widgetWithText(CupertinoActionSheetAction, 'Two')),
    );
  });

  testWidgets('Action section is scrollable', (WidgetTester tester) async {
    final ScrollController actionScrollController = ScrollController();
    addTearDown(actionScrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return MediaQuery.withClampedTextScaling(
            minScaleFactor: 3.0,
            maxScaleFactor: 3.0,
            child: CupertinoActionSheet(
              title: const Text('The title'),
              message: const Text('The message.'),
              actions: <Widget>[
                CupertinoActionSheetAction(
                  child: const Text('One'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Two'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Three'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Four'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Five'),
                  onPressed: () { },
                ),
              ],
              actionScrollController: actionScrollController,
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    // Check that the action buttons list is scrollable.
    expect(actionScrollController.offset, 0.0);
    actionScrollController.jumpTo(100.0);
    expect(actionScrollController.offset, 100.0);
    actionScrollController.jumpTo(0.0);

    // Check that the action buttons are aligned vertically.
    expect(tester.getCenter(find.widgetWithText(CupertinoActionSheetAction, 'One')).dx, equals(400.0));
    expect(tester.getCenter(find.widgetWithText(CupertinoActionSheetAction, 'Two')).dx, equals(400.0));
    expect(tester.getCenter(find.widgetWithText(CupertinoActionSheetAction, 'Three')).dx, equals(400.0));
    expect(tester.getCenter(find.widgetWithText(CupertinoActionSheetAction, 'Four')).dx, equals(400.0));
    expect(tester.getCenter(find.widgetWithText(CupertinoActionSheetAction, 'Five')).dx, equals(400.0));

    // Check that the action buttons are the correct heights.
    expect(tester.getSize(find.widgetWithText(CupertinoActionSheetAction, 'One')).height, equals(83.0));
    expect(tester.getSize(find.widgetWithText(CupertinoActionSheetAction, 'Two')).height, equals(83.0));
    expect(tester.getSize(find.widgetWithText(CupertinoActionSheetAction, 'Three')).height, equals(83.0));
    expect(tester.getSize(find.widgetWithText(CupertinoActionSheetAction, 'Four')).height, equals(83.0));
    expect(tester.getSize(find.widgetWithText(CupertinoActionSheetAction, 'Five')).height, equals(83.0));
  });

  testWidgets('Content section is scrollable', (WidgetTester tester) async {
    final ScrollController messageScrollController = ScrollController();
    addTearDown(messageScrollController.dispose);
    late double screenHeight;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          screenHeight = MediaQuery.sizeOf(context).height;
          return MediaQuery.withClampedTextScaling(
            minScaleFactor: 3.0,
            maxScaleFactor: 3.0,
            child: CupertinoActionSheet(
              title: const Text('The title'),
              message: Text('Very long content' * 200),
              actions: <Widget>[
                CupertinoActionSheetAction(
                  child: const Text('One'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Two'),
                  onPressed: () { },
                ),
              ],
              messageScrollController: messageScrollController,
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(messageScrollController.offset, 0.0);
    messageScrollController.jumpTo(100.0);
    expect(messageScrollController.offset, 100.0);
    // Set the scroll position back to zero.
    messageScrollController.jumpTo(0.0);

    // Expect the action sheet to take all available height.
    expect(tester.getSize(find.byType(CupertinoActionSheet)).height, screenHeight);
  });

  testWidgets('CupertinoActionSheet scrollbars controllers should be different', (WidgetTester tester) async {
    // https://github.com/flutter/flutter/pull/81278
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
            title: const Text('The title'),
            message: Text('Very long content' * 200),
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () { },
              ),
            ],
          )
        ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    final List<CupertinoScrollbar> scrollbars =
      find.descendant(
        of: find.byType(CupertinoActionSheet),
        matching: find.byType(CupertinoScrollbar),
      ).evaluate().map((Element e) => e.widget as CupertinoScrollbar).toList();

    expect(scrollbars.length, 2);
    expect(scrollbars[0].controller != scrollbars[1].controller, isTrue);
  });

  testWidgets('Actions section correctly renders overscrolls', (WidgetTester tester) async {
    // Verifies that when the actions section overscrolls, the overscroll part
    // is correctly covered with background.
    final ScrollController actionScrollController = ScrollController();
    addTearDown(actionScrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: List<Widget>.generate(12, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {},
                child: Text('Button ${'*' * i}'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Button *')));
    await tester.pumpAndSettle();
    // The button should be pressed now, since the scrolling gesture has not
    // taken over.
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.overscroll.0.png'),
    );
    // The dragging gesture must be dispatched in at least two segments.
    // After the first movement, the gesture is started, but the delta is still
    // zero. The second movement gives the delta.
    await gesture.moveBy(const Offset(0, 40));
    await tester.pumpAndSettle();
    await gesture.moveBy(const Offset(0, 100));
    // Test the top overscroll. Use `pump` not `pumpAndSettle` to verify the
    // rendering result of the immediate next frame.
    await tester.pump();
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.overscroll.1.png'),
    );

    await gesture.moveBy(const Offset(0, -300));
    // Test the bottom overscroll. Use `pump` not `pumpAndSettle` to verify the
    // rendering result of the immediate next frame.
    await tester.pump();
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.overscroll.2.png'),
    );
    await gesture.up();
  });

  testWidgets('Actions section correctly renders overscrolls with very far scrolls', (WidgetTester tester) async {
    // When the scroll is really far, the overscroll might be longer than the
    // actions section, causing overflow if not controlled.
    final ScrollController actionScrollController = ScrollController();
    addTearDown(actionScrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            message: Text('message' * 300),
            actions: List<Widget>.generate(4, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {},
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Button 0')));
    await tester.pumpAndSettle();
    await gesture.moveBy(const Offset(0, 40)); // A short drag to start the gesture.
    await tester.pumpAndSettle();
    // The drag is far enough to make the overscroll longer than the section.
    await gesture.moveBy(const Offset(0, 1000));
    await tester.pump();
    // The buttons should be out of the screen
    expect(
      tester.getTopLeft(find.text('Button 0')).dy,
      greaterThan(tester.getBottomLeft(find.byType(CupertinoActionSheet)).dy)
    );
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.long-overscroll.0.png'),
    );
  });

  testWidgets('Taps on button calls onPressed', (WidgetTester tester) async {
    bool wasPressed = false;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () {
                  expect(wasPressed, false);
                  wasPressed = true;
                  Navigator.pop(context);
                },
              ),
            ],
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(wasPressed, isFalse);

    await tester.tap(find.text('One'));

    expect(wasPressed, isTrue);

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(find.text('One'), findsNothing);
  });

  testWidgets('Can tap after scrolling', (WidgetTester tester) async {
    int? wasPressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: List<Widget>.generate(20, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {
                  expect(wasPressed, null);
                  wasPressed = i;
                },
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();
    expect(find.text('Button 19').hitTestable(), findsNothing);

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Button 1')));
    await tester.pumpAndSettle();
    // The dragging gesture must be dispatched in at least two segments.
    // The first movement starts the gesture without setting a delta.
    await gesture.moveBy(const Offset(0, -20));
    await tester.pumpAndSettle();
    await gesture.moveBy(const Offset(0, -1000));
    await tester.pumpAndSettle();
    await gesture.up();
    await tester.pumpAndSettle();
    expect(find.text('Button 19').hitTestable(), findsOne);

    await tester.tap(find.text('Button 19'));
    await tester.pumpAndSettle();
    expect(wasPressed, 19);
  });

  testWidgets('Taps at the padding of buttons calls onPressed', (WidgetTester tester) async {
    // Ensures that the entire button responds to hit tests, not just the text
    // part.
    bool wasPressed = false;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () {
                  expect(wasPressed, false);
                  wasPressed = true;
                  Navigator.pop(context);
                },
              ),
            ],
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(wasPressed, isFalse);

    await tester.tapAt(
      tester.getTopLeft(find.text('One')) - const Offset(20, 0),
    );

    expect(wasPressed, isTrue);

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(find.text('One'), findsNothing);
  });

  testWidgets('Taps on a button can be slided to other buttons', (WidgetTester tester) async {
    int? pressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () {
                  expect(pressed, null);
                  pressed = 1;
                  Navigator.pop(context);
                },
              ),
              CupertinoActionSheetAction(
                child: const Text('Two'),
                onPressed: () {
                  expect(pressed, null);
                  pressed = 2;
                  Navigator.pop(context);
                },
              ),
            ],
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();
    expect(pressed, null);

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Two')));
    await tester.pumpAndSettle();

    await gesture.moveTo(tester.getCenter(find.text('One')));
    await tester.pumpAndSettle();
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.press-drag.png'),
    );

    await gesture.up();
    expect(pressed, 1);
    await tester.pumpAndSettle();
    expect(find.text('One'), findsNothing);
  });

  testWidgets('Taps on the content can be slided to other buttons', (WidgetTester tester) async {
    bool wasPressed = false;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            title: const Text('The title'),
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () {
                },
              ),
            ],
            cancelButton: CupertinoActionSheetAction(
              child: const Text('Cancel'),
              onPressed: () {
                expect(wasPressed, false);
                wasPressed = true;
                Navigator.pop(context);
              },
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();
    expect(wasPressed, false);

    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('The title')));
    await tester.pumpAndSettle();

    await gesture.moveTo(tester.getCenter(find.text('Cancel')));
    await tester.pumpAndSettle();
    await gesture.up();
    expect(wasPressed, true);
    await tester.pumpAndSettle();
    expect(find.text('One'), findsNothing);
  });

  testWidgets('Taps on the barrier can not be slided to buttons', (WidgetTester tester) async {
    bool wasPressed = false;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            title: const Text('The title'),
            cancelButton: CupertinoActionSheetAction(
              child: const Text('Cancel'),
              onPressed: () {
                expect(wasPressed, false);
                wasPressed = true;
                Navigator.pop(context);
              },
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();
    expect(wasPressed, false);

    // Press on the barrier.
    final TestGesture gesture = await tester.startGesture(const Offset(100, 100));
    await tester.pumpAndSettle();

    await gesture.moveTo(tester.getCenter(find.text('Cancel')));
    await tester.pumpAndSettle();
    await gesture.up();
    expect(wasPressed, false);
    await tester.pumpAndSettle();
    expect(find.text('Cancel'), findsOne);
  });

  testWidgets('Sliding taps can still yield to scrolling after horizontal movement', (WidgetTester tester) async {
    int? pressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            message: Text('Long message' * 200),
            actions: List<Widget>.generate(10, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {
                  expect(pressed, null);
                  pressed = i;
                },
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    // Starts on a button.
    final TestGesture gesture = await tester.startGesture(tester.getCenter(find.text('Button 0')));
    await tester.pumpAndSettle();
    // Move horizontally.
    await gesture.moveBy(const Offset(-10, 2));
    await gesture.moveBy(const Offset(-100, 2));
    await tester.pumpAndSettle();
    // Scroll up.
    await gesture.moveBy(const Offset(0, -40));
    await gesture.moveBy(const Offset(0, -1000));
    await tester.pumpAndSettle();
    // Stop scrolling.
    await gesture.up();
    await tester.pumpAndSettle();
    // The actions section should have been scrolled up and Button 9 is visible.
    await tester.tap(find.text('Button 9'));
    expect(pressed, 9);
  });

  testWidgets('Sliding taps is responsive even before the drag starts', (WidgetTester tester) async {
    int? pressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            message: Text('Long message' * 200),
            actions: List<Widget>.generate(10, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {
                  expect(pressed, null);
                  pressed = i;
                },
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    // Find the location right within the upper edge of button 1.
    final Offset start = tester.getTopLeft(find.text('Button 1')) + const Offset(30, -15);
    // Verify that the start location is within button 1.
    await tester.tapAt(start);
    expect(pressed, 1);
    pressed = null;

    final TestGesture gesture = await tester.startGesture(start);
    await tester.pumpAndSettle();
    // Move slightly upwards without starting the drag
    await gesture.moveBy(const Offset(0, -10));
    await tester.pumpAndSettle();
    // Stop scrolling.
    await gesture.up();
    await tester.pumpAndSettle();
    expect(pressed, 0);
  });

  testWidgets('Sliding taps only recognizes the primary pointer', (WidgetTester tester) async {
    int? pressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            title: const Text('The title'),
            actions: List<Widget>.generate(8, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {
                  expect(pressed, null);
                  pressed = i;
                },
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    // Start gesture 1 at button 0
    final TestGesture gesture1 = await tester.startGesture(tester.getCenter(find.text('Button 0')));
    await gesture1.moveBy(const Offset(0, 20)); // Starts the gesture
    await tester.pumpAndSettle();

    // Start gesture 2 at button 1.
    final TestGesture gesture2 = await tester.startGesture(tester.getCenter(find.text('Button 1')));
    await gesture2.moveBy(const Offset(0, 20)); // Starts the gesture
    await tester.pumpAndSettle();

    // Move gesture 1 to button 2 and release.
    await gesture1.moveTo(tester.getCenter(find.text('Button 2')));
    await tester.pumpAndSettle();
    await gesture1.up();
    await tester.pumpAndSettle();

    expect(pressed, 2);
    pressed = null;

    // Tap at button 3, which becomes the new primary pointer and is recognized.
    await tester.tap(find.text('Button 3'));
    await tester.pumpAndSettle();
    expect(pressed, 3);
    pressed = null;

    // Move gesture 2 to button 4 and release.
    await gesture2.moveTo(tester.getCenter(find.text('Button 4')));
    await tester.pumpAndSettle();
    await gesture2.up();
    await tester.pumpAndSettle();

    // Non-primary pointers should not be recognized.
    expect(pressed, null);
  });

  testWidgets('Non-primary pointers can trigger scroll', (WidgetTester tester) async {
    int? pressed;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            actions: List<Widget>.generate(12, (int i) =>
              CupertinoActionSheetAction(
                onPressed: () {
                  expect(pressed, null);
                  pressed = i;
                },
                child: Text('Button $i'),
              ),
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    // Start gesture 1 at button 0
    final TestGesture gesture1 = await tester.startGesture(tester.getCenter(find.text('Button 0')));
    await tester.pumpAndSettle();

    expect(tester.getTopLeft(find.text('Button 11')).dy, greaterThan(400));

    // Start gesture 2 at button 1 and scrolls.
    final TestGesture gesture2 = await tester.startGesture(tester.getCenter(find.text('Button 1')));
    await gesture2.moveBy(const Offset(0, -20));
    await gesture2.moveBy(const Offset(0, -500));
    await tester.pumpAndSettle();

    expect(tester.getTopLeft(find.text('Button 11')).dy, lessThan(400));

    // Release gesture 1, which should not trigger any buttons.
    await gesture1.up();
    await tester.pumpAndSettle();

    expect(pressed, null);
  });

  testWidgets('Action sheet width is correct when given infinite horizontal space', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Row(
          children: <Widget>[
            CupertinoActionSheet(
              actions: <Widget>[
                CupertinoActionSheetAction(
                  child: const Text('One'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Two'),
                  onPressed: () { },
                ),
              ],
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(tester.getSize(find.byType(CupertinoActionSheet)).width, 600.0);
  });

  testWidgets('Action sheet height is correct when given infinite vertical space', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Column(
          children: <Widget>[
            CupertinoActionSheet(
              actions: <Widget>[
                CupertinoActionSheetAction(
                  child: const Text('One'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Two'),
                  onPressed: () { },
                ),
              ],
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(tester.getSize(find.byType(CupertinoActionSheet)).height, moreOrLessEquals(132.3));
  });

  testWidgets('1 action button with cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    // Action section is size of one action button.
    expect(findScrollableActionsSectionRenderBox(tester).size.height, 56.0);
  });

  testWidgets('2 action buttons with cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(findScrollableActionsSectionRenderBox(tester).size.height, moreOrLessEquals(84.3));
  });

  testWidgets('3 action buttons with cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Three'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(findScrollableActionsSectionRenderBox(tester).size.height, moreOrLessEquals(84.3));
  });

  testWidgets('4+ action buttons with cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Three'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Four'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(findScrollableActionsSectionRenderBox(tester).size.height, moreOrLessEquals(84.3));
  });

  testWidgets('1 action button without cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(findScrollableActionsSectionRenderBox(tester).size.height, 56.0);
  });

  testWidgets('2+ action buttons without cancel button', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: Text('Very long content' * 200),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    expect(findScrollableActionsSectionRenderBox(tester).size.height, moreOrLessEquals(84.3));
  });

  testWidgets('Action sheet with just cancel button is correct', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    // Height should be cancel button height + padding
    expect(tester.getSize(find.byType(CupertinoActionSheet)).height, 76.0);
    expect(tester.getSize(find.byType(CupertinoActionSheet)).width, 600.0);
  });

  testWidgets('Cancel button tap calls onPressed', (WidgetTester tester) async {
    bool wasPressed = false;
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return CupertinoActionSheet(
            cancelButton: CupertinoActionSheetAction(
              child: const Text('Cancel'),
              onPressed: () {
                expect(wasPressed, false);
                wasPressed = true;
                Navigator.pop(context);
              },
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(wasPressed, isFalse);

    await tester.tap(find.text('Cancel'));

    expect(wasPressed, isTrue);

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(find.text('Cancel'), findsNothing);
  });

  testWidgets('Layout is correct when cancel button is present', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message'),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));

    await tester.pump();
    await tester.pump(const Duration(seconds: 1));

    expect(tester.getBottomLeft(find.widgetWithText(CupertinoActionSheetAction, 'Cancel')).dy, 590.0);
    expect(
      tester.getBottomLeft(find.widgetWithText(CupertinoActionSheetAction, 'One')).dy,
      moreOrLessEquals(469.7),
    );
    expect(tester.getBottomLeft(find.widgetWithText(CupertinoActionSheetAction, 'Two')).dy, 526.0);
  });

  testWidgets('Action buttons shows pressed color as soon as the pointer is down', (WidgetTester tester) async {
    // Verifies that the the pressed color is not delayed for some milliseconds,
    // a symptom if the color relies on a tap gesture timing out.
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    final TestGesture pointer = await tester.startGesture(tester.getCenter(find.text('Two')));
    // Just `pump`, not `pumpAndSettle`, as we want to verify the very next frame.
    await tester.pump();
    await expectLater(
      find.byType(CupertinoActionSheet),
      matchesGoldenFile('cupertinoActionSheet.pressed.png'),
    );
    await pointer.up();
  });

  testWidgets('Enter/exit animation is correct', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message'),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    // Enter animation
    await tester.tap(find.text('Go'));

    await tester.pump();
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, 600.0);

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(483.9, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(398.6, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(365.3, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(354.8, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(350.7, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(349.4, epsilon: 0.1));

    // Action sheet has reached final height
    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(349.4, epsilon: 0.1));

    // Exit animation
    await tester.tapAt(const Offset(20.0, 20.0));
    await tester.pump();
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(349.4, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(465.5, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(550.8, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(584.1, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(594.6, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(598.7, epsilon: 0.1));

    // Action sheet has disappeared
    await tester.pump(const Duration(milliseconds: 60));
    expect(find.byType(CupertinoActionSheet), findsNothing);
  });

  testWidgets('Modal barrier is pressed during transition', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message'),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    // Enter animation
    await tester.tap(find.text('Go'));

    await tester.pump();
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, 600.0);

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(483.92863239836686, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(398.5571539306641, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(365.3034101784229, epsilon: 0.1));

    // Exit animation
    await tester.tapAt(const Offset(20.0, 20.0));
    await tester.pump(const Duration(milliseconds: 60));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(398.5571539306641, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, moreOrLessEquals(483.92863239836686, epsilon: 0.1));

    await tester.pump(const Duration(milliseconds: 60));
    expect(tester.getTopLeft(find.byType(CupertinoActionSheet)).dy, 600.0);

    // Action sheet has disappeared
    await tester.pump(const Duration(milliseconds: 60));
    expect(find.byType(CupertinoActionSheet), findsNothing);
  });


  testWidgets('Action sheet semantics', (WidgetTester tester) async {
    final SemanticsTester semantics = SemanticsTester(tester);

    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
          title: const Text('The title'),
          message: const Text('The message'),
          actions: <Widget>[
            CupertinoActionSheetAction(
              child: const Text('One'),
              onPressed: () { },
            ),
            CupertinoActionSheetAction(
              child: const Text('Two'),
              onPressed: () { },
            ),
          ],
          cancelButton: CupertinoActionSheetAction(
            child: const Text('Cancel'),
            onPressed: () { },
          ),
        ),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pumpAndSettle();

    expect(
      semantics,
      hasSemantics(
        TestSemantics.root(
          children: <TestSemantics>[
            TestSemantics(
              children: <TestSemantics>[
                TestSemantics(
                  children: <TestSemantics>[
                    TestSemantics(
                      flags: <SemanticsFlag>[
                        SemanticsFlag.scopesRoute,
                        SemanticsFlag.namesRoute,
                      ],
                      label: 'Alert',
                      children: <TestSemantics>[
                        TestSemantics(
                          flags: <SemanticsFlag>[
                            SemanticsFlag.hasImplicitScrolling,
                          ],
                          children: <TestSemantics>[
                            TestSemantics(
                              label: 'The title',
                            ),
                            TestSemantics(
                              label: 'The message',
                            ),
                          ],
                        ),
                        TestSemantics(
                          flags: <SemanticsFlag>[
                            SemanticsFlag.hasImplicitScrolling,
                          ],
                          children: <TestSemantics>[
                            TestSemantics(
                              flags: <SemanticsFlag>[
                                SemanticsFlag.isButton,
                              ],
                              actions: <SemanticsAction>[
                                SemanticsAction.tap,
                              ],
                              label: 'One',
                            ),
                            TestSemantics(
                              flags: <SemanticsFlag>[
                                SemanticsFlag.isButton,
                              ],
                              actions: <SemanticsAction>[
                                SemanticsAction.tap,
                              ],
                              label: 'Two',
                            ),
                          ],
                        ),
                        TestSemantics(
                          flags: <SemanticsFlag>[
                            SemanticsFlag.isButton,
                          ],
                          actions: <SemanticsAction>[
                            SemanticsAction.tap,
                          ],
                          label: 'Cancel',
                        ),
                      ],
                    ),
                  ],
                ),
              ],
            ),
          ],
        ),
        ignoreId: true,
        ignoreRect: true,
        ignoreTransform: true,
      ),
    );

    semantics.dispose();
  });

  testWidgets('Conflicting scrollbars are not applied by ScrollBehavior to CupertinoActionSheet', (WidgetTester tester) async {
    // Regression test for https://github.com/flutter/flutter/issues/83819
    final ScrollController actionScrollController = ScrollController();
    addTearDown(actionScrollController.dispose);
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        Builder(builder: (BuildContext context) {
          return MediaQuery.withClampedTextScaling(
            minScaleFactor: 3.0,
            maxScaleFactor: 3.0,
            child: CupertinoActionSheet(
              title: const Text('The title'),
              message: const Text('The message.'),
              actions: <Widget>[
                CupertinoActionSheetAction(
                  child: const Text('One'),
                  onPressed: () { },
                ),
                CupertinoActionSheetAction(
                  child: const Text('Two'),
                  onPressed: () { },
                ),
              ],
              actionScrollController: actionScrollController,
            ),
          );
        }),
      ),
    );

    await tester.tap(find.text('Go'));
    await tester.pump();

    // The inherited ScrollBehavior should not apply Scrollbars since they are
    // already built in to the widget.
    expect(find.byType(Scrollbar), findsNothing);
    expect(find.byType(RawScrollbar), findsNothing);
    // Built in CupertinoScrollbars should only number 2: one for the actions,
    // one for the content.
    expect(find.byType(CupertinoScrollbar), findsNWidgets(2));
  }, variant: TargetPlatformVariant.all());

  testWidgets('Hovering over Cupertino action sheet action updates cursor to clickable on Web', (WidgetTester tester) async {
    await tester.pumpWidget(
      createAppWithButtonThatLaunchesActionSheet(
        CupertinoActionSheet(
            title: const Text('The title'),
            message: const Text('Message'),
            actions: <Widget>[
              CupertinoActionSheetAction(
                child: const Text('One'),
                onPressed: () { },
              ),
            ],
          )
        ),
    );
    await tester.tap(find.text('Go'));
    await tester.pump();

    final TestGesture gesture = await tester.createGesture(kind: PointerDeviceKind.mouse, pointer: 1);
    await gesture.addPointer(location: const Offset(10, 10));
    await tester.pumpAndSettle();
    expect(RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1), SystemMouseCursors.basic);

    final Offset actionSheetAction = tester.getCenter(find.text('One'));
    await gesture.moveTo(actionSheetAction);
    await tester.pumpAndSettle();
    expect(
      RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1),
      kIsWeb ? SystemMouseCursors.click : SystemMouseCursors.basic,
    );
  });
}

RenderBox findScrollableActionsSectionRenderBox(WidgetTester tester) {
  final RenderObject actionsSection = tester.renderObject(
    find.byElementPredicate((Element element) {
      return element.widget.runtimeType.toString() == '_ActionSheetActionSection';
    }),
  );
  assert(actionsSection is RenderBox);
  return actionsSection as RenderBox;
}

Widget createAppWithButtonThatLaunchesActionSheet(Widget actionSheet) {
  return CupertinoApp(
    home: Center(
      child: Builder(builder: (BuildContext context) {
        return CupertinoButton(
          onPressed: () {
            showCupertinoModalPopup<void>(
              context: context,
              builder: (BuildContext context) {
                return actionSheet;
              },
            );
          },
          child: const Text('Go'),
        );
      }),
    ),
  );
}

// Shows an app that has a button with text "Go", and clicking this button
// displays the `actionSheet` and hides the button.
//
// The `theme` will be applied to the app and determines the background.
class TestScaffoldApp extends StatefulWidget {
  const TestScaffoldApp({super.key, required this.theme, required this.actionSheet});
  final CupertinoThemeData theme;
  final Widget actionSheet;

  @override
  TestScaffoldAppState createState() => TestScaffoldAppState();
}

class TestScaffoldAppState extends State<TestScaffoldApp> {
  bool _pressedButton = false;

  @override
  Widget build(BuildContext context) {
    return CupertinoApp(
      theme: widget.theme,
      home: Builder(builder: (BuildContext context) =>
        CupertinoPageScaffold(
          child: Center(
            child: _pressedButton ? Container() : CupertinoButton(
              onPressed: () {
                setState(() {
                  _pressedButton = true;
                });
                showCupertinoModalPopup<void>(
                  context: context,
                  builder: (BuildContext context) {
                    return widget.actionSheet;
                  },
                );
              },
              child: const Text('Go'),
            ),
          ),
        ),
      ),
    );
  }
}

Widget boilerplate(Widget child) {
  return Directionality(
    textDirection: TextDirection.ltr,
    child: child,
  );
}
