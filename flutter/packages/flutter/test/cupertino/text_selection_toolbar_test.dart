// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/cupertino.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

import '../rendering/mock_canvas.dart';
import '../widgets/editable_text_utils.dart' show textOffsetToPosition;

// These constants are copied from cupertino/text_selection_toolbar.dart.
const double _kArrowScreenPadding = 26.0;
const double _kToolbarContentDistance = 8.0;
const double _kToolbarHeight = 45.0;

// A custom text selection menu that just displays a single custom button.
class _CustomCupertinoTextSelectionControls extends CupertinoTextSelectionControls {
  @override
  Widget buildToolbar(
    BuildContext context,
    Rect globalEditableRegion,
    double textLineHeight,
    Offset selectionMidpoint,
    List<TextSelectionPoint> endpoints,
    TextSelectionDelegate delegate,
    ValueListenable<ClipboardStatus>? clipboardStatus,
    Offset? lastSecondaryTapDownPosition,
  ) {
    final EdgeInsets mediaQueryPadding = MediaQuery.paddingOf(context);
    final double anchorX = (selectionMidpoint.dx + globalEditableRegion.left).clamp(
      _kArrowScreenPadding + mediaQueryPadding.left,
      MediaQuery.sizeOf(context).width - mediaQueryPadding.right - _kArrowScreenPadding,
    );
    final Offset anchorAbove = Offset(
      anchorX,
      endpoints.first.point.dy - textLineHeight + globalEditableRegion.top,
    );
    final Offset anchorBelow = Offset(
      anchorX,
      endpoints.last.point.dy + globalEditableRegion.top,
    );

    return CupertinoTextSelectionToolbar(
      anchorAbove: anchorAbove,
      anchorBelow: anchorBelow,
      children: <Widget>[
        CupertinoTextSelectionToolbarButton(
          onPressed: () {},
          child: const Text('Custom button'),
        ),
      ],
    );
  }
}

class TestBox extends SizedBox {
  const TestBox({super.key}) : super(width: itemWidth, height: itemHeight);

  static const double itemHeight = 44.0;
  static const double itemWidth = 100.0;
}

const CupertinoDynamicColor _kToolbarTextColor = CupertinoDynamicColor.withBrightness(
  color: CupertinoColors.black,
  darkColor: CupertinoColors.white,
);

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // Find by a runtimeType String, including private types.
  Finder findPrivate(String type) {
    return find.descendant(
      of: find.byType(CupertinoApp),
      matching: find.byWidgetPredicate((Widget w) => '${w.runtimeType}' == type),
    );
  }

  // Finding CupertinoTextSelectionToolbar won't give you the position as the user sees
  // it because it's a full-sized Stack at the top level. This method finds the
  // visible part of the toolbar for use in measurements.
  Finder findToolbar() => findPrivate('_CupertinoTextSelectionToolbarContent');

  // Check if the middle point of the chevron is pointing left or right.
  //
  // Offset.dx: a right or left margin (_kToolbarChevronSize / 4 => 2.5) to center the icon horizontally
  // Offset.dy: always in the exact vertical center (_kToolbarChevronSize / 2 => 5)
  PaintPattern overflowNextPaintPattern() => paints
    ..line(p1: const Offset(2.5, 0), p2: const Offset(7.5, 5))
    ..line(p1: const Offset(7.5, 5), p2: const Offset(2.5, 10));
  PaintPattern overflowBackPaintPattern() => paints
    ..line(p1: const Offset(7.5, 0), p2: const Offset(2.5, 5))
    ..line(p1: const Offset(2.5, 5), p2: const Offset(7.5, 10));

  Finder findOverflowNextButton() => find.byWidgetPredicate((Widget widget) =>
    widget is CustomPaint &&
    '${widget.painter?.runtimeType}' == '_RightCupertinoChevronPainter',
  );
  Finder findOverflowBackButton() => find.byWidgetPredicate((Widget widget) =>
    widget is CustomPaint &&
    '${widget.painter?.runtimeType}' == '_LeftCupertinoChevronPainter',
  );

  testWidgets('chevrons point to the correct side', (WidgetTester tester) async {
    // Add enough TestBoxes to need 3 pages.
    final List<Widget> children = List<Widget>.generate(15, (int i) => const TestBox());
    await tester.pumpWidget(
      CupertinoApp(
        home: Center(
          child: CupertinoTextSelectionToolbar(
            anchorAbove: const Offset(50.0, 100.0),
            anchorBelow: const Offset(50.0, 200.0),
            children: children,
          ),
        ),
      ),
    );

    expect(findOverflowBackButton(), findsNothing);
    expect(findOverflowNextButton(), findsOneWidget);

    expect(findOverflowNextButton(), overflowNextPaintPattern());

    // Tap the overflow next button to show the next page of children.
    await tester.tapAt(tester.getCenter(findOverflowNextButton()));
    await tester.pumpAndSettle();

    expect(findOverflowBackButton(), findsOneWidget);
    expect(findOverflowNextButton(), findsOneWidget);

    expect(findOverflowBackButton(), overflowBackPaintPattern());
    expect(findOverflowNextButton(), overflowNextPaintPattern());

    // Tap the overflow next button to show the last page of children.
    await tester.tapAt(tester.getCenter(findOverflowNextButton()));
    await tester.pumpAndSettle();

    expect(findOverflowBackButton(), findsOneWidget);
    expect(findOverflowNextButton(), findsNothing);

    expect(findOverflowBackButton(), overflowBackPaintPattern());
  }, skip: kIsWeb); // Path.combine is not implemented in the HTML backend https://github.com/flutter/flutter/issues/44572

  testWidgets('paginates children if they overflow', (WidgetTester tester) async {
    late StateSetter setState;
    final List<Widget> children = List<Widget>.generate(7, (int i) => const TestBox());
    await tester.pumpWidget(
      CupertinoApp(
        home: Center(
          child: StatefulBuilder(
            builder: (BuildContext context, StateSetter setter) {
              setState = setter;
              return CupertinoTextSelectionToolbar(
                anchorAbove: const Offset(50.0, 100.0),
                anchorBelow: const Offset(50.0, 200.0),
                children: children,
              );
            },
          ),
        ),
      ),
    );

    // All children fit on the screen, so they are all rendered.
    expect(find.byType(TestBox), findsNWidgets(children.length));
    expect(findOverflowNextButton(), findsNothing);
    expect(findOverflowBackButton(), findsNothing);

    // Adding one more child makes the children overflow.
    setState(() {
      children.add(
        const TestBox(),
      );
    });
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(children.length - 1));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsNothing);

    // Tap the overflow next button to show the next page of children.
    // The next button is hidden as there's no next page.
    await tester.tapAt(tester.getCenter(findOverflowNextButton()));
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(1));
    expect(findOverflowNextButton(), findsNothing);
    expect(findOverflowBackButton(), findsOneWidget);

    // Tap the overflow back button to go back to the first page.
    await tester.tapAt(tester.getCenter(findOverflowBackButton()));
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(7));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsNothing);

    // Adding 7 more children overflows onto a third page.
    setState(() {
      children.add(const TestBox());
      children.add(const TestBox());
      children.add(const TestBox());
      children.add(const TestBox());
      children.add(const TestBox());
      children.add(const TestBox());
    });
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(7));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsNothing);

    // Tap the overflow next button to show the second page of children.
    await tester.tapAt(tester.getCenter(findOverflowNextButton()));
    await tester.pumpAndSettle();
    // With the back button, only six children fit on this page.
    expect(find.byType(TestBox), findsNWidgets(6));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsOneWidget);

    // Tap the overflow next button again to show the third page of children.
    await tester.tapAt(tester.getCenter(findOverflowNextButton()));
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(1));
    expect(findOverflowNextButton(), findsNothing);
    expect(findOverflowBackButton(), findsOneWidget);

    // Tap the overflow back button to go back to the second page.
    await tester.tapAt(tester.getCenter(findOverflowBackButton()));
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(6));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsOneWidget);

    // Tap the overflow back button to go back to the first page.
    await tester.tapAt(tester.getCenter(findOverflowBackButton()));
    await tester.pumpAndSettle();
    expect(find.byType(TestBox), findsNWidgets(7));
    expect(findOverflowNextButton(), findsOneWidget);
    expect(findOverflowBackButton(), findsNothing);
  }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.

  testWidgets('does not paginate if children fit with zero margin', (WidgetTester tester) async {
    final List<Widget> children = List<Widget>.generate(7, (int i) => const TestBox());
    final double spacerWidth = 1.0 / tester.view.devicePixelRatio;
    final double dividerWidth = 1.0 / tester.view.devicePixelRatio;
    const double borderRadius = 8.0; // Should match _kToolbarBorderRadius
    final double width = 7 * TestBox.itemWidth + 6 * (dividerWidth + 2 * spacerWidth) + 2 * borderRadius;
    await tester.pumpWidget(
      CupertinoApp(
        home: Center(
          child: SizedBox(
            width: width,
            child: CupertinoTextSelectionToolbar(
              anchorAbove: const Offset(50.0, 100.0),
              anchorBelow: const Offset(50.0, 200.0),
              children: children,
            ),
          ),
        ),
      ),
    );

    // All children fit on the screen, so they are all rendered.
    expect(find.byType(TestBox), findsNWidgets(children.length));
    expect(findOverflowNextButton(), findsNothing);
    expect(findOverflowBackButton(), findsNothing);
  }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.

  testWidgets('positions itself at anchorAbove if it fits', (WidgetTester tester) async {
    late StateSetter setState;
    const double height = _kToolbarHeight;
    const double anchorBelowY = 500.0;
    double anchorAboveY = 0.0;
    const double paddingAbove = 12.0;

    await tester.pumpWidget(
      CupertinoApp(
        home: Center(
          child: StatefulBuilder(
            builder: (BuildContext context, StateSetter setter) {
              setState = setter;
              final MediaQueryData data = MediaQuery.of(context);
              // Add some custom vertical padding to make this test more strict.
              // By default in the testing environment, _kToolbarContentDistance
              // and the built-in padding from CupertinoApp can end up canceling
              // each other out.
              return MediaQuery(
                data: data.copyWith(
                  padding: data.viewPadding.copyWith(
                    top: paddingAbove,
                  ),
                ),
                child: CupertinoTextSelectionToolbar(
                  anchorAbove: Offset(50.0, anchorAboveY),
                  anchorBelow: const Offset(50.0, anchorBelowY),
                  children: <Widget>[
                    Container(color: const Color(0xffff0000), width: 50.0, height: height),
                    Container(color: const Color(0xff00ff00), width: 50.0, height: height),
                    Container(color: const Color(0xff0000ff), width: 50.0, height: height),
                  ],
                ),
              );
            },
          ),
        ),
      ),
    );

    // When the toolbar doesn't fit above aboveAnchor, it positions itself below
    // belowAnchor.
    double toolbarY = tester.getTopLeft(findToolbar()).dy;
    expect(toolbarY, equals(anchorBelowY + _kToolbarContentDistance));
    expect(find.byType(CustomSingleChildLayout), findsOneWidget);
    final CustomSingleChildLayout layout = tester.widget(find.byType(CustomSingleChildLayout));
    final TextSelectionToolbarLayoutDelegate delegate = layout.delegate as TextSelectionToolbarLayoutDelegate;
    expect(delegate.anchorBelow.dy, anchorBelowY - paddingAbove);

    // Even when it barely doesn't fit.
    setState(() {
      anchorAboveY = 70.0;
    });
    await tester.pump();
    toolbarY = tester.getTopLeft(findToolbar()).dy;
    expect(toolbarY, equals(anchorBelowY + _kToolbarContentDistance));

    // When it does fit above aboveAnchor, it positions itself there.
    setState(() {
      anchorAboveY = 80.0;
    });
    await tester.pump();
    toolbarY = tester.getTopLeft(findToolbar()).dy;
    expect(toolbarY, equals(anchorAboveY - height - _kToolbarContentDistance));
  }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.

  testWidgets('can create and use a custom toolbar', (WidgetTester tester) async {
    final TextEditingController controller = TextEditingController(
      text: 'Select me custom menu',
    );
    await tester.pumpWidget(
      CupertinoApp(
        home: Center(
          child: CupertinoTextField(
            controller: controller,
            selectionControls: _CustomCupertinoTextSelectionControls(),
          ),
        ),
      ),
    );

    // The selection menu is not initially shown.
    expect(find.text('Custom button'), findsNothing);

    // Long press on "custom" to select it.
    final Offset customPos = textOffsetToPosition(tester, 11);
    final TestGesture gesture = await tester.startGesture(customPos, pointer: 7);
    await tester.pump(const Duration(seconds: 2));
    await gesture.up();
    await tester.pump();

    // The custom selection menu is shown.
    expect(find.text('Custom button'), findsOneWidget);
    expect(find.text('Cut'), findsNothing);
    expect(find.text('Copy'), findsNothing);
    expect(find.text('Paste'), findsNothing);
    expect(find.text('Select all'), findsNothing);
  }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.

  for (final Brightness? themeBrightness in <Brightness?>[...Brightness.values, null]) {
    for (final Brightness? mediaBrightness in <Brightness?>[...Brightness.values, null]) {
      testWidgets('draws dark buttons in dark mode and light button in light mode when theme is $themeBrightness and MediaQuery is $mediaBrightness', (WidgetTester tester) async {
        await tester.pumpWidget(
          CupertinoApp(
            theme: CupertinoThemeData(
              brightness: themeBrightness,
            ),
            home: Center(
              child: Builder(
                builder: (BuildContext context) {
                  return MediaQuery(
                    data: MediaQuery.of(context).copyWith(platformBrightness: mediaBrightness),
                    child: CupertinoTextSelectionToolbar(
                      anchorAbove: const Offset(100.0, 0.0),
                      anchorBelow: const Offset(100.0, 0.0),
                      children: <Widget>[
                        CupertinoTextSelectionToolbarButton.text(
                          onPressed: () {},
                          text: 'Button',
                        ),
                      ],
                    ),
                  );
                },
              ),
            ),
          ),
        );

        final Finder buttonFinder = find.byType(CupertinoButton);
        expect(buttonFinder, findsOneWidget);

        final Finder textFinder = find.descendant(
          of: find.byType(CupertinoButton),
          matching: find.byType(Text)
        );
        expect(textFinder, findsOneWidget);
        final Text text = tester.widget(textFinder);

        // Theme brightness is preferred, otherwise MediaQuery brightness is
        // used. If both are null, defaults to light.
        late final Brightness effectiveBrightness;
        if (themeBrightness != null) {
          effectiveBrightness = themeBrightness;
        } else {
          effectiveBrightness = mediaBrightness ?? Brightness.light;
        }

        expect(
          text.style!.color!.value,
          effectiveBrightness == Brightness.dark
              ? _kToolbarTextColor.darkColor.value
              : _kToolbarTextColor.color.value,
        );
      }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.
    }
  }

  testWidgets('draws a shadow below the toolbar in light mode', (WidgetTester tester) async {
    late StateSetter setState;
    const double height = _kToolbarHeight;
    double anchorAboveY = 0.0;

    await tester.pumpWidget(
      CupertinoApp(
        theme: const CupertinoThemeData(
          brightness: Brightness.light,
        ),
        home: Center(
          child: StatefulBuilder(
            builder: (BuildContext context, StateSetter setter) {
              setState = setter;
              final MediaQueryData data = MediaQuery.of(context);
              // Add some custom vertical padding to make this test more strict.
              // By default in the testing environment, _kToolbarContentDistance
              // and the built-in padding from CupertinoApp can end up canceling
              // each other out.
              return MediaQuery(
                data: data.copyWith(
                  padding: data.viewPadding.copyWith(
                    top: 12.0,
                  ),
                ),
                child: CupertinoTextSelectionToolbar(
                  anchorAbove: Offset(50.0, anchorAboveY),
                  anchorBelow: const Offset(50.0, 500.0),
                  children: <Widget>[
                    Container(color: const Color(0xffff0000), width: 50.0, height: height),
                    Container(color: const Color(0xff00ff00), width: 50.0, height: height),
                    Container(color: const Color(0xff0000ff), width: 50.0, height: height),
                  ],
                ),
              );
            },
          ),
        ),
      ),
    );

    // When the toolbar is below the content, the shadow hangs below the entire
    // toolbar.
    final Finder finder = find.descendant(
      of: find.byType(CupertinoTextSelectionToolbar),
      matching: find.byType(DecoratedBox),
    );
    expect(finder, findsOneWidget);
    DecoratedBox decoratedBox = tester.widget(finder.first);
    BoxDecoration boxDecoration = decoratedBox.decoration as BoxDecoration;
    List<BoxShadow>? shadows = boxDecoration.boxShadow;
    expect(shadows, isNotNull);
    expect(shadows, hasLength(1));
    BoxShadow shadow = boxDecoration.boxShadow!.first;
    expect(shadow.offset.dy, equals(7.0));

    // When the toolbar is above the content, the shadow sits around the arrow
    // with no offset.
    setState(() {
      anchorAboveY = 80.0;
    });
    await tester.pump();
    decoratedBox = tester.widget(finder.first);
    boxDecoration = decoratedBox.decoration as BoxDecoration;
    shadows = boxDecoration.boxShadow;
    expect(shadows, isNotNull);
    expect(shadows, hasLength(1));
    shadow = boxDecoration.boxShadow!.first;
    expect(shadow.offset.dy, equals(0.0));
  }, skip: kIsWeb); // [intended] We do not use Flutter-rendered context menu on the Web.
}
