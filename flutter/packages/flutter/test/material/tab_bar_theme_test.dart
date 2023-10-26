// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// This file is run as part of a reduced test set in CI on Mac and Windows
// machines.
@Tags(<String>['reduced-test-set'])
library;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:leak_tracker_flutter_testing/leak_tracker_flutter_testing.dart';

const String _tab1Text = 'tab 1';
const String _tab2Text = 'tab 2';
const String _tab3Text = 'tab 3';

final Key _painterKey = UniqueKey();

const List<Tab> _tabs = <Tab>[
  Tab(text: _tab1Text, icon: Icon(Icons.looks_one)),
  Tab(text: _tab2Text, icon: Icon(Icons.looks_two)),
  Tab(text: _tab3Text, icon: Icon(Icons.looks_3)),
];

final List<SizedBox> _sizedTabs = <SizedBox>[
  SizedBox(key: UniqueKey(), width: 100.0, height: 50.0),
  SizedBox(key: UniqueKey(), width: 100.0, height: 50.0),
];

Widget buildTabBar({
  TabBarTheme? tabBarTheme,
  bool secondaryTabBar = false,
  List<Widget> tabs = _tabs,
  bool isScrollable = false,
  bool useMaterial3 = false,
}) {
  final TabController controller = TabController(
    length: tabs.length,
    vsync: const TestVSync(),
  );
  addTearDown(controller.dispose);

  if (secondaryTabBar) {
    return MaterialApp(
      theme: ThemeData(tabBarTheme: tabBarTheme, useMaterial3: useMaterial3),
      home: Scaffold(
        body: RepaintBoundary(
          key: _painterKey,
          child: TabBar.secondary(
            tabs: tabs,
            isScrollable: isScrollable,
            controller: controller,
          ),
        ),
      ),
    );
  }
  return MaterialApp(
    theme: ThemeData(tabBarTheme: tabBarTheme, useMaterial3: useMaterial3),
    home: Scaffold(
      body: RepaintBoundary(
        key: _painterKey,
        child: TabBar(
          tabs: tabs,
          isScrollable: isScrollable,
          controller: controller,
        ),
      ),
    ),
  );
}


RenderParagraph _getIcon(WidgetTester tester, IconData icon) {
  return tester.renderObject<RenderParagraph>(
    find.descendant(of: find.byIcon(icon), matching: find.byType(RichText)),
  );
}

RenderParagraph _getText(WidgetTester tester, String text) {
  return  tester.renderObject<RenderParagraph>(find.text(text));
}

void main() {
  test('TabBarTheme copyWith, ==, hashCode, defaults', () {
    expect(const TabBarTheme(), const TabBarTheme().copyWith());
    expect(const TabBarTheme().hashCode, const TabBarTheme().copyWith().hashCode);

    expect(const TabBarTheme().indicator, null);
    expect(const TabBarTheme().indicatorColor, null);
    expect(const TabBarTheme().indicatorSize, null);
    expect(const TabBarTheme().dividerColor, null);
    expect(const TabBarTheme().dividerHeight, null);
    expect(const TabBarTheme().labelColor, null);
    expect(const TabBarTheme().labelPadding, null);
    expect(const TabBarTheme().labelStyle, null);
    expect(const TabBarTheme().unselectedLabelColor, null);
    expect(const TabBarTheme().unselectedLabelStyle, null);
    expect(const TabBarTheme().overlayColor, null);
    expect(const TabBarTheme().splashFactory, null);
    expect(const TabBarTheme().mouseCursor, null);
    expect(const TabBarTheme().tabAlignment, null);
  });

  test('TabBarTheme lerp special cases', () {
    const TabBarTheme theme = TabBarTheme();
    expect(identical(TabBarTheme.lerp(theme, theme, 0.5), theme), true);
  });

  testWidgetsWithLeakTracking('Tab bar defaults (primary)', (WidgetTester tester) async {
    // Test default label color and label styles.
    await tester.pumpWidget(buildTabBar(useMaterial3: true));

    final ThemeData theme = ThemeData(useMaterial3: true);
    final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
    expect(selectedLabel.text.style!.fontFamily, equals(theme.textTheme.titleSmall!.fontFamily));
    expect(selectedLabel.text.style!.fontSize, equals(14.0));
    expect(selectedLabel.text.style!.color, equals(theme.colorScheme.primary));
    final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
    expect(unselectedLabel.text.style!.fontFamily, equals(theme.textTheme.titleSmall!.fontFamily));
    expect(unselectedLabel.text.style!.fontSize, equals(14.0));
    expect(unselectedLabel.text.style!.color, equals(theme.colorScheme.onSurfaceVariant));

    // Test default labelPadding.
    await tester.pumpWidget(buildTabBar(tabs: _sizedTabs, isScrollable: true));

    const double indicatorWeight = 2.0;
    final Rect tabBar = tester.getRect(find.byType(TabBar));
    final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
    final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));
    const double tabStartOffset = 52.0;

    // Verify tabOne coordinates.
    expect(tabOneRect.left, equals(kTabLabelPadding.left + tabStartOffset));
    expect(tabOneRect.top, equals(kTabLabelPadding.top));
    expect(tabOneRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

    // Verify tabTwo coordinates.
    final double tabTwoRight = tabStartOffset + kTabLabelPadding.horizontal + tabOneRect.width
      + kTabLabelPadding.left + tabTwoRect.width;
    expect(tabTwoRect.right, tabTwoRight);
    expect(tabTwoRect.top, equals(kTabLabelPadding.top));
    expect(tabTwoRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

    // Verify tabOne and tabTwo are separated by right padding of tabOne and left padding of tabTwo.
    expect(tabOneRect.right, equals(tabTwoRect.left - kTabLabelPadding.left - kTabLabelPadding.right));

    // Test default indicator & divider color.
    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(
      tabBarBox,
      paints
        ..line(
          color: theme.colorScheme.surfaceVariant,
          strokeWidth: 1.0,
        )
        ..rrect(color: theme.colorScheme.primary),
    );
  });

  testWidgetsWithLeakTracking('Tab bar defaults (secondary)', (WidgetTester tester) async {
    // Test default label color and label styles.
    await tester.pumpWidget(buildTabBar(secondaryTabBar: true, useMaterial3: true));

    final ThemeData theme = ThemeData(useMaterial3: true);
    final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
    expect(selectedLabel.text.style!.fontFamily, equals(theme.textTheme.titleSmall!.fontFamily));
    expect(selectedLabel.text.style!.fontSize, equals(14.0));
    expect(selectedLabel.text.style!.color, equals(theme.colorScheme.onSurface));
    final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
    expect(unselectedLabel.text.style!.fontFamily, equals(theme.textTheme.titleSmall!.fontFamily));
    expect(unselectedLabel.text.style!.fontSize, equals(14.0));
    expect(unselectedLabel.text.style!.color, equals(theme.colorScheme.onSurfaceVariant));

    // Test default labelPadding.
    await tester.pumpWidget(buildTabBar(
      secondaryTabBar: true,
      tabs: _sizedTabs,
      isScrollable: true,
      useMaterial3: true,
    ));

    const double indicatorWeight = 2.0;
    final Rect tabBar = tester.getRect(find.byType(TabBar));
    final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
    final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));
    const double tabStartOffset = 52.0;

    // Verify tabOne coordinates.
    expect(tabOneRect.left, equals(kTabLabelPadding.left + tabStartOffset));
    expect(tabOneRect.top, equals(kTabLabelPadding.top));
    expect(tabOneRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

    // Verify tabTwo coordinates.
    final double tabTwoRight = tabStartOffset + kTabLabelPadding.horizontal + tabOneRect.width
      + kTabLabelPadding.left + tabTwoRect.width;
    expect(tabTwoRect.right, tabTwoRight);
    expect(tabTwoRect.top, equals(kTabLabelPadding.top));
    expect(tabTwoRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

    // Verify tabOne and tabTwo are separated by right padding of tabOne and left padding of tabTwo.
    expect(tabOneRect.right, equals(tabTwoRect.left - kTabLabelPadding.left - kTabLabelPadding.right));

    // Test default indicator & divider color.
    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(
      tabBarBox,
      paints
        ..line(
          color: theme.colorScheme.surfaceVariant,
          strokeWidth: 1.0,
        )
        ..line(color: theme.colorScheme.primary),
      );
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides label color (selected)', (WidgetTester tester) async {
    const Color labelColor = Colors.black;
    const TabBarTheme tabBarTheme = TabBarTheme(labelColor: labelColor);

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final RenderParagraph tabLabel = _getText(tester, _tab1Text);
    expect(tabLabel.text.style!.color, equals(labelColor));
    final RenderParagraph tabIcon = _getIcon(tester, Icons.looks_one);
    expect(tabIcon.text.style!.color, equals(labelColor));
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides label padding', (WidgetTester tester) async {
    const double topPadding = 10.0;
    const double bottomPadding = 7.0;
    const double rightPadding = 13.0;
    const double leftPadding = 16.0;
    const double indicatorWeight = 2.0; // default value

    const EdgeInsetsGeometry labelPadding = EdgeInsets.fromLTRB(
      leftPadding, topPadding, rightPadding, bottomPadding,
    );

    const TabBarTheme tabBarTheme = TabBarTheme(labelPadding: labelPadding);

    await tester.pumpWidget(buildTabBar(
      tabBarTheme: tabBarTheme,
      tabs: _sizedTabs,
      isScrollable: true,
    ));

    final Rect tabBar = tester.getRect(find.byType(TabBar));
    final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
    final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));

    // verify coordinates of tabOne
    expect(tabOneRect.left, equals(leftPadding));
    expect(tabOneRect.top, equals(topPadding));
    expect(tabOneRect.bottom, equals(tabBar.bottom - bottomPadding - indicatorWeight));

    // verify coordinates of tabTwo
    expect(tabTwoRect.right, equals(tabBar.width - rightPadding));
    expect(tabTwoRect.top, equals(topPadding));
    expect(tabTwoRect.bottom, equals(tabBar.bottom - bottomPadding - indicatorWeight));

    // verify tabOne and tabTwo are separated by right padding of tabOne and left padding of tabTwo
    expect(tabOneRect.right, equals(tabTwoRect.left - leftPadding - rightPadding));
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides label styles', (WidgetTester tester) async {
    const TextStyle labelStyle = TextStyle(fontFamily: 'foobar');
    const TextStyle unselectedLabelStyle = TextStyle(fontFamily: 'baz');
    const TabBarTheme tabBarTheme = TabBarTheme(
      labelStyle: labelStyle,
      unselectedLabelStyle: unselectedLabelStyle,
    );

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
    expect(selectedLabel.text.style!.fontFamily, equals(labelStyle.fontFamily));
    final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
    expect(unselectedLabel.text.style!.fontFamily, equals(unselectedLabelStyle.fontFamily));
  });

  testWidgetsWithLeakTracking('Tab bar theme with just label style specified', (WidgetTester tester) async {
    // Regression test for https://github.com/flutter/flutter/issues/28784
    const TextStyle labelStyle = TextStyle(fontFamily: 'foobar');
    const TabBarTheme tabBarTheme = TabBarTheme(
      labelStyle: labelStyle,
    );

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
    expect(selectedLabel.text.style!.fontFamily, equals(labelStyle.fontFamily));
    final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
    expect(unselectedLabel.text.style!.fontFamily, equals('Roboto'));
    expect(unselectedLabel.text.style!.fontSize, equals(14.0));
    expect(unselectedLabel.text.style!.color, equals(Colors.white.withAlpha(0xB2)));
  });

  testWidgetsWithLeakTracking('Tab bar label styles override theme label styles', (WidgetTester tester) async {
    const TextStyle labelStyle = TextStyle(fontFamily: '1');
    const TextStyle unselectedLabelStyle = TextStyle(fontFamily: '2');
    const TextStyle themeLabelStyle = TextStyle(fontFamily: '3');
    const TextStyle themeUnselectedLabelStyle = TextStyle(fontFamily: '4');
    const TabBarTheme tabBarTheme = TabBarTheme(
      labelStyle: themeLabelStyle,
      unselectedLabelStyle: themeUnselectedLabelStyle,
    );
    final TabController controller = TabController(
      length: _tabs.length,
      vsync: const TestVSync(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(tabBarTheme: tabBarTheme),
        home: Scaffold(
          body: TabBar(
            tabs: _tabs,
            controller: controller,
            labelStyle: labelStyle,
            unselectedLabelStyle: unselectedLabelStyle,
          ),
        ),
      ),
    );

    final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
    expect(selectedLabel.text.style!.fontFamily, equals(labelStyle.fontFamily));
    final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
    expect(unselectedLabel.text.style!.fontFamily, equals(unselectedLabelStyle.fontFamily));
  });

  testWidgetsWithLeakTracking('Material2 - Tab bar label padding overrides theme label padding', (WidgetTester tester) async {
    const double verticalPadding = 10.0;
    const double horizontalPadding = 10.0;
    const EdgeInsetsGeometry labelPadding = EdgeInsets.symmetric(
      vertical: verticalPadding,
      horizontal: horizontalPadding,
    );

    const double verticalThemePadding = 20.0;
    const double horizontalThemePadding = 20.0;
    const EdgeInsetsGeometry themeLabelPadding = EdgeInsets.symmetric(
      vertical: verticalThemePadding,
      horizontal: horizontalThemePadding,
    );

    const double indicatorWeight = 2.0; // default value

    const TabBarTheme tabBarTheme = TabBarTheme(labelPadding: themeLabelPadding);

    final TabController controller = TabController(
      length: _sizedTabs.length,
      vsync: const TestVSync(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(tabBarTheme: tabBarTheme, useMaterial3: false),
        home: Scaffold(body:
          RepaintBoundary(
            key: _painterKey,
            child: TabBar(
              tabs: _sizedTabs,
              isScrollable: true,
              controller: controller,
              labelPadding: labelPadding,
            ),
          ),
        ),
      ),
    );

    final Rect tabBar = tester.getRect(find.byType(TabBar));
    final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
    final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));

    // verify coordinates of tabOne
    expect(tabOneRect.left, equals(horizontalPadding));
    expect(tabOneRect.top, equals(verticalPadding));
    expect(tabOneRect.bottom, equals(tabBar.bottom - verticalPadding - indicatorWeight));

    // verify coordinates of tabTwo
    expect(tabTwoRect.right, equals(tabBar.width - horizontalPadding));
    expect(tabTwoRect.top, equals(verticalPadding));
    expect(tabTwoRect.bottom, equals(tabBar.bottom - verticalPadding - indicatorWeight));

    // verify tabOne and tabTwo are separated by 2x horizontalPadding
    expect(tabOneRect.right, equals(tabTwoRect.left - (2 * horizontalPadding)));
  });

  testWidgetsWithLeakTracking('Material3 - Tab bar label padding overrides theme label padding', (WidgetTester tester) async {
    const double tabStartOffset = 52.0;
    const double verticalPadding = 10.0;
    const double horizontalPadding = 10.0;
    const EdgeInsetsGeometry labelPadding = EdgeInsets.symmetric(
      vertical: verticalPadding,
      horizontal: horizontalPadding,
    );

    const double verticalThemePadding = 20.0;
    const double horizontalThemePadding = 20.0;
    const EdgeInsetsGeometry themeLabelPadding = EdgeInsets.symmetric(
      vertical: verticalThemePadding,
      horizontal: horizontalThemePadding,
    );

    const double indicatorWeight = 2.0; // default value

    const TabBarTheme tabBarTheme = TabBarTheme(labelPadding: themeLabelPadding);

    final TabController controller = TabController(
      length: _sizedTabs.length,
      vsync: const TestVSync(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(tabBarTheme: tabBarTheme, useMaterial3: true),
        home: Scaffold(body:
          RepaintBoundary(
            key: _painterKey,
            child: TabBar(
              tabs: _sizedTabs,
              isScrollable: true,
              controller: controller,
              labelPadding: labelPadding,
            ),
          ),
        ),
      ),
    );

    final Rect tabBar = tester.getRect(find.byType(TabBar));
    final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
    final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));

    // verify coordinates of tabOne
    expect(tabOneRect.left, equals(horizontalPadding + tabStartOffset));
    expect(tabOneRect.top, equals(verticalPadding));
    expect(tabOneRect.bottom, equals(tabBar.bottom - verticalPadding - indicatorWeight));

    // verify coordinates of tabTwo
    expect(tabTwoRect.right, equals(tabStartOffset + horizontalThemePadding + tabOneRect.width + tabTwoRect.width + (horizontalThemePadding / 2)));
    expect(tabTwoRect.top, equals(verticalPadding));
    expect(tabTwoRect.bottom, equals(tabBar.bottom - verticalPadding - indicatorWeight));

    // verify tabOne and tabTwo are separated by 2x horizontalPadding
    expect(tabOneRect.right, equals(tabTwoRect.left - (2 * horizontalPadding)));
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides label color (unselected)', (WidgetTester tester) async {
    const Color unselectedLabelColor = Colors.black;
    const TabBarTheme tabBarTheme = TabBarTheme(unselectedLabelColor: unselectedLabelColor);

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final RenderParagraph textRenderObject = tester.renderObject<RenderParagraph>(find.text(_tab2Text));
    expect(textRenderObject.text.style!.color, equals(unselectedLabelColor));
    final RenderParagraph iconRenderObject = _getIcon(tester, Icons.looks_two);
    expect(iconRenderObject.text.style!.color, equals(unselectedLabelColor));
  });

  testWidgetsWithLeakTracking('Tab bar default tab indicator size (primary)', (WidgetTester tester) async {
    await tester.pumpWidget(buildTabBar(useMaterial3: true, isScrollable: true));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar.default.tab_indicator_size.png'),
    );
  });

  testWidgetsWithLeakTracking('Tab bar default tab indicator size (secondary)', (WidgetTester tester) async {
    await tester.pumpWidget(buildTabBar(useMaterial3: true, isScrollable: true));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar_secondary.default.tab_indicator_size.png'),
    );
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides tab indicator size (tab)', (WidgetTester tester) async {
    const TabBarTheme tabBarTheme = TabBarTheme(indicatorSize: TabBarIndicatorSize.tab);

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar_theme.tab_indicator_size_tab.png'),
    );
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides tab indicator size (label)', (WidgetTester tester) async {
    const TabBarTheme tabBarTheme = TabBarTheme(indicatorSize: TabBarIndicatorSize.label);

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar_theme.tab_indicator_size_label.png'),
    );
  });

  testWidgetsWithLeakTracking('Tab bar theme overrides tab mouse cursor', (WidgetTester tester) async {
    const TabBarTheme tabBarTheme = TabBarTheme(mouseCursor: MaterialStateMouseCursor.textable);

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final Offset tabBar = tester.getCenter(
      find.ancestor(of: find.text('tab 1'),matching: find.byType(TabBar)),
    );
    final TestGesture gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer();
    await gesture.moveTo(tabBar);
    await tester.pumpAndSettle();
    expect(RendererBinding.instance.mouseTracker.debugDeviceActiveCursor(1), SystemMouseCursors.text);
  });

  testWidgetsWithLeakTracking('Tab bar theme - custom tab indicator', (WidgetTester tester) async {
    final TabBarTheme tabBarTheme = TabBarTheme(
      indicator: BoxDecoration(
        border: Border.all(),
      ),
    );

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar_theme.custom_tab_indicator.png'),
    );
  });

  testWidgetsWithLeakTracking('Tab bar theme - beveled rect indicator', (WidgetTester tester) async {
    const TabBarTheme tabBarTheme = TabBarTheme(
      indicator: ShapeDecoration(
        shape: BeveledRectangleBorder(borderRadius: BorderRadius.all(Radius.circular(20.0))),
        color: Colors.black,
      ),
    );

    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    await expectLater(
      find.byKey(_painterKey),
      matchesGoldenFile('tab_bar_theme.beveled_rect_indicator.png'),
    );
  });

  testWidgetsWithLeakTracking('TabAlignment.fill from TabBarTheme only supports non-scrollable tab bar', (WidgetTester tester) async {
    const TabBarTheme tabBarTheme = TabBarTheme(tabAlignment: TabAlignment.fill);

    // Test TabAlignment.fill from TabBarTheme with non-scrollable tab bar.
    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    expect(tester.takeException(), isNull);

    // Test TabAlignment.fill from TabBarTheme with scrollable tab bar.
    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme, isScrollable: true));

    expect(tester.takeException(), isAssertionError);
  });

  testWidgetsWithLeakTracking(
    'TabAlignment.start & TabAlignment.startOffset from TabBarTheme only supports scrollable tab bar',
    (WidgetTester tester) async {
      TabBarTheme tabBarTheme = const TabBarTheme(tabAlignment: TabAlignment.start);

      // Test TabAlignment.start from TabBarTheme with scrollable tab bar.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme, isScrollable: true));

      expect(tester.takeException(), isNull);

      // Test TabAlignment.start from TabBarTheme with non-scrollable tab bar.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

      expect(tester.takeException(), isAssertionError);

      tabBarTheme = const TabBarTheme(tabAlignment: TabAlignment.startOffset);

      // Test TabAlignment.startOffset from TabBarTheme with scrollable tab bar.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme, isScrollable: true));

      expect(tester.takeException(), isNull);

      // Test TabAlignment.startOffset from TabBarTheme with non-scrollable tab bar.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

      expect(tester.takeException(), isAssertionError);
  });

  testWidgetsWithLeakTracking('TabBarTheme.indicatorSize provides correct tab indicator (primary)', (WidgetTester tester) async {
    final ThemeData theme = ThemeData(
      tabBarTheme: const TabBarTheme(indicatorSize: TabBarIndicatorSize.tab),
      useMaterial3: true,
    );
    final List<Widget> tabs = List<Widget>.generate(4, (int index) {
      return Tab(text: 'Tab $index');
    });

    final TabController controller = TabController(
      vsync: const TestVSync(),
      length: tabs.length,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: theme,
        home: Scaffold(
          body: Container(
            alignment: Alignment.topLeft,
            child: TabBar(
              controller: controller,
              tabs: tabs,
            ),
          ),
        ),
      ),
    );

    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox.size.height, 48.0);

    const double indicatorWeight = 2.0;
    const double indicatorY = 48 - (indicatorWeight / 2.0);
    const double indicatorLeft =  indicatorWeight / 2.0;
    const double indicatorRight = 200.0 - (indicatorWeight / 2.0);

    expect(
      tabBarBox,
      paints
        // Divider.
        ..line(
          color: theme.colorScheme.surfaceVariant,
          strokeWidth: 1.0,
        )
        // Tab indicator.
        ..line(
          color: theme.colorScheme.primary,
          strokeWidth: indicatorWeight,
          p1: const Offset(indicatorLeft, indicatorY),
          p2: const Offset(indicatorRight, indicatorY),
        ),
    );
  });

  testWidgetsWithLeakTracking('TabBarTheme.indicatorSize provides correct tab indicator (secondary)', (WidgetTester tester) async {
    final ThemeData theme = ThemeData(
      tabBarTheme: const TabBarTheme(indicatorSize: TabBarIndicatorSize.label),
      useMaterial3: true,
    );
    final List<Widget> tabs = List<Widget>.generate(4, (int index) {
      return Tab(text: 'Tab $index');
    });

    final TabController controller = TabController(
      vsync: const TestVSync(),
      length: tabs.length,
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: theme,
        home: Scaffold(
          body: Container(
            alignment: Alignment.topLeft,
            child: TabBar.secondary(
              controller: controller,
              tabs: tabs,
            ),
          ),
        ),
      ),
    );

    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox.size.height, 48.0);

    const double indicatorWeight = 2.0;
    const double indicatorY = 48 - (indicatorWeight / 2.0);

    expect(
      tabBarBox,
      paints
        // Divider.
        ..line(
          color: theme.colorScheme.surfaceVariant,
          strokeWidth: 1.0,
        )
        // Tab indicator
        ..line(
          color: theme.colorScheme.primary,
          strokeWidth: indicatorWeight,
          p1: const Offset(65.75, indicatorY),
          p2: const Offset(134.25, indicatorY),
        ),
    );
  });

  testWidgetsWithLeakTracking('TabBar divider can use TabBarTheme.dividerColor & TabBarTheme.dividerHeight', (WidgetTester tester) async {
    const Color dividerColor = Color(0xff00ff00);
    const double dividerHeight = 10.0;

    final TabController controller = TabController(
      length: 3,
      vsync: const TestVSync(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          tabBarTheme: const TabBarTheme(
            dividerColor: dividerColor,
            dividerHeight: dividerHeight,
          ),
          useMaterial3: true,
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              controller: controller,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 2'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );

    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    // Test divider color.
    expect(tabBarBox, paints..line(color: dividerColor, strokeWidth: dividerHeight));
  });

  testWidgetsWithLeakTracking('dividerColor & dividerHeight overrides TabBarTheme.dividerColor', (WidgetTester tester) async {
    const Color dividerColor = Color(0xff0000ff);
    const double dividerHeight = 8.0;

    final TabController controller = TabController(
      length: 3,
      vsync: const TestVSync(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          useMaterial3: true,
          tabBarTheme: const TabBarTheme(
            dividerColor: Colors.pink,
            dividerHeight: 5.0,
          ),
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              dividerColor: dividerColor,
              dividerHeight: dividerHeight,
              controller: controller,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 2'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );

    final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    // Test divider color.
    expect(tabBarBox, paints..line(color: dividerColor, strokeWidth: dividerHeight));
  });

  testWidgetsWithLeakTracking('TabBar respects TabBarTheme.tabAlignment', (WidgetTester tester) async {
    final TabController controller1 = TabController(
      length: 2,
      vsync: const TestVSync(),
    );
    addTearDown(controller1.dispose);

    // Test non-scrollable tab bar.
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.center),
          useMaterial3: true,
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              controller: controller1,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );

    const double availableWidth = 800.0;
    Rect tabOneRect = tester.getRect(find.byType(Tab).first);
    Rect tabTwoRect = tester.getRect(find.byType(Tab).last);

    double tabOneLeft = (availableWidth / 2) - tabOneRect.width - kTabLabelPadding.left;
    expect(tabOneRect.left, equals(tabOneLeft));
    double tabTwoRight = (availableWidth / 2) + tabTwoRect.width + kTabLabelPadding.right;
    expect(tabTwoRect.right, equals(tabTwoRight));

    final TabController controller2 = TabController(
      length: 2,
      vsync: const TestVSync(),
    );
    addTearDown(controller2.dispose);

    // Test scrollable tab bar.
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.start),
          useMaterial3: true,
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              isScrollable: true,
              controller: controller2,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    tabOneRect = tester.getRect(find.byType(Tab).first);
    tabTwoRect = tester.getRect(find.byType(Tab).last);

    tabOneLeft = kTabLabelPadding.left;
    expect(tabOneRect.left, equals(tabOneLeft));
    tabTwoRight = kTabLabelPadding.horizontal + tabOneRect.width + kTabLabelPadding.left + tabTwoRect.width;
    expect(tabTwoRect.right, equals(tabTwoRight));
  });

  testWidgetsWithLeakTracking('TabBar.tabAlignment overrides TabBarTheme.tabAlignment', (WidgetTester tester) async {
    final TabController controller1 = TabController(
      length: 2,
      vsync: const TestVSync(),
    );
    addTearDown(controller1.dispose);

    /// Test non-scrollable tab bar.
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.fill),
          useMaterial3: true,
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              tabAlignment: TabAlignment.center,
              controller: controller1,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );

    const double availableWidth = 800.0;
    Rect tabOneRect = tester.getRect(find.byType(Tab).first);
    Rect tabTwoRect = tester.getRect(find.byType(Tab).last);

    double tabOneLeft = (availableWidth / 2) - tabOneRect.width - kTabLabelPadding.left;
    expect(tabOneRect.left, equals(tabOneLeft));
    double tabTwoRight = (availableWidth / 2) + tabTwoRect.width + kTabLabelPadding.right;
    expect(tabTwoRect.right, equals(tabTwoRight));

    final TabController controller2 = TabController(
      length: 2,
      vsync: const TestVSync(),
    );
    addTearDown(controller2.dispose);

    /// Test scrollable tab bar.
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(
          tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.center),
          useMaterial3: true,
        ),
        home: Scaffold(
          appBar: AppBar(
            bottom: TabBar(
              isScrollable: true,
              tabAlignment: TabAlignment.start,
              controller: controller2,
              tabs: const <Widget>[
                Tab(text: 'Tab 1'),
                Tab(text: 'Tab 3'),
              ],
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    tabOneRect = tester.getRect(find.byType(Tab).first);
    tabTwoRect = tester.getRect(find.byType(Tab).last);

    tabOneLeft = kTabLabelPadding.left;
    expect(tabOneRect.left, equals(tabOneLeft));
    tabTwoRight = kTabLabelPadding.horizontal + tabOneRect.width + kTabLabelPadding.left + tabTwoRect.width;
    expect(tabTwoRect.right, equals(tabTwoRight));
  });

  testWidgetsWithLeakTracking(
    'TabBar labels use colors from TabBarTheme.labelStyle & TabBarTheme.unselectedLabelStyle',
    (WidgetTester tester) async {
      const TextStyle labelStyle = TextStyle(
        color: Color(0xff0000ff),
        fontStyle: FontStyle.italic,
      );
      const TextStyle unselectedLabelStyle = TextStyle(
        color: Color(0x950000ff),
        fontStyle: FontStyle.italic,
      );
      const TabBarTheme tabBarTheme = TabBarTheme(
        labelStyle: labelStyle,
        unselectedLabelStyle: unselectedLabelStyle,
      );

      // Test tab bar with TabBarTheme labelStyle & unselectedLabelStyle.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

      final IconThemeData selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      final IconThemeData uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      final TextStyle selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text))
        .text.style!;
      final TextStyle unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text))
        .text.style!;

      // Selected tab should use labelStyle color.
      expect(selectedTabIcon.color, labelStyle.color);
      expect(selectedTextStyle.color, labelStyle.color);
      expect(selectedTextStyle.fontStyle, labelStyle.fontStyle);
      // Unselected tab should use unselectedLabelStyle color.
      expect(uselectedTabIcon.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.fontStyle, unselectedLabelStyle.fontStyle);
  });

  testWidgetsWithLeakTracking(
    "TabBarTheme's labelColor & unselectedLabelColor override labelStyle & unselectedLabelStyle colors",
    (WidgetTester tester) async {
      const Color labelColor = Color(0xfff00000);
      const Color unselectedLabelColor = Color(0x95ff0000);
      const TextStyle labelStyle = TextStyle(
        color: Color(0xff0000ff),
        fontStyle: FontStyle.italic,
      );
      const TextStyle unselectedLabelStyle = TextStyle(
        color: Color(0x950000ff),
        fontStyle: FontStyle.italic,
      );
      TabBarTheme tabBarTheme = const TabBarTheme(
        labelStyle: labelStyle,
        unselectedLabelStyle: unselectedLabelStyle,
      );

      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

      // Test tab bar with TabBarTheme labelStyle & unselectedLabelStyle.
      await tester.pumpWidget(buildTabBar());

      IconThemeData selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      IconThemeData uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      TextStyle selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text))
        .text.style!;
      TextStyle unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text))
        .text.style!;

      // Selected tab should use the labelStyle color.
      expect(selectedTabIcon.color, labelStyle.color);
      expect(selectedTextStyle.color, labelStyle.color);
      expect(selectedTextStyle.fontStyle, labelStyle.fontStyle);
      // Unselected tab should use the unselectedLabelStyle color.
      expect(uselectedTabIcon.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.fontStyle, unselectedLabelStyle.fontStyle);

      // Update the TabBarTheme with labelColor & unselectedLabelColor.
      tabBarTheme = const TabBarTheme(
        labelColor: labelColor,
        unselectedLabelColor: unselectedLabelColor,
        labelStyle: labelStyle,
        unselectedLabelStyle: unselectedLabelStyle,
      );
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));
      await tester.pumpAndSettle();

      selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text)).text.style!;
      unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text)).text.style!;

      // Selected tab should use the labelColor.
      expect(selectedTabIcon.color, labelColor);
      expect(selectedTextStyle.color, labelColor);
      expect(selectedTextStyle.fontStyle, labelStyle.fontStyle);
      // Unselected tab should use the unselectedLabelColor.
      expect(uselectedTabIcon.color, unselectedLabelColor);
      expect(unselectedTextStyle.color, unselectedLabelColor);
      expect(unselectedTextStyle.fontStyle, unselectedLabelStyle.fontStyle);
  });

  testWidgetsWithLeakTracking(
    "TabBarTheme's labelColor & unselectedLabelColor override TabBar.labelStyle & TabBar.unselectedLabelStyle colors",
    (WidgetTester tester) async {
      const Color labelColor = Color(0xfff00000);
      const Color unselectedLabelColor = Color(0x95ff0000);
      const TextStyle labelStyle = TextStyle(
        color: Color(0xff0000ff),
        fontStyle: FontStyle.italic,
      );
      const TextStyle unselectedLabelStyle = TextStyle(
        color: Color(0x950000ff),
        fontStyle: FontStyle.italic,
      );

      Widget buildTabBar({TabBarTheme? tabBarTheme}) {
        return MaterialApp(
          theme: ThemeData(tabBarTheme: tabBarTheme),
          home: const Material(
            child: DefaultTabController(
              length: 2,
              child: TabBar(
                labelStyle: labelStyle,
                unselectedLabelStyle: unselectedLabelStyle,
                tabs: <Widget>[
                  Tab(text: _tab1Text),
                  Tab(text: _tab2Text),
                ],
              ),
            ),
          ),
        );
      }

      // Test tab bar with [TabBar.labelStyle] & [TabBar.unselectedLabelStyle].
      await tester.pumpWidget(buildTabBar());

      IconThemeData selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      IconThemeData uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      TextStyle selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text))
        .text.style!;
      TextStyle unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text))
        .text.style!;

      // Selected tab should use the [TabBar.labelStyle] color.
      expect(selectedTabIcon.color, labelStyle.color);
      expect(selectedTextStyle.color, labelStyle.color);
      expect(selectedTextStyle.fontStyle, labelStyle.fontStyle);
      // Unselected tab should use the [TabBar.unselectedLabelStyle] color.
      expect(uselectedTabIcon.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.color, unselectedLabelStyle.color);
      expect(unselectedTextStyle.fontStyle, unselectedLabelStyle.fontStyle);

      // Add TabBarTheme with labelColor & unselectedLabelColor.
      await tester.pumpWidget(buildTabBar(tabBarTheme: const TabBarTheme(
        labelColor: labelColor,
        unselectedLabelColor: unselectedLabelColor,
      )));
      await tester.pumpAndSettle();

      selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text)).text.style!;
      unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text)).text.style!;

      // Selected tab should use the [TabBarTheme.labelColor].
      expect(selectedTabIcon.color, labelColor);
      expect(selectedTextStyle.color, labelColor);
      expect(selectedTextStyle.fontStyle, labelStyle.fontStyle);
      // Unselected tab should use the [TabBarTheme.unselectedLabelColor].
      expect(uselectedTabIcon.color, unselectedLabelColor);
      expect(unselectedTextStyle.color, unselectedLabelColor);
      expect(unselectedTextStyle.fontStyle, unselectedLabelStyle.fontStyle);
  });

  group('Material 2', () {
    // These tests are only relevant for Material 2. Once Material 2
    // support is deprecated and the APIs are removed, these tests
    // can be deleted.

    testWidgetsWithLeakTracking('Tab bar defaults (primary)', (WidgetTester tester) async {
    // Test default label color and label styles.
      await tester.pumpWidget(buildTabBar());

      final ThemeData theme = ThemeData(useMaterial3: false);
      final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
      expect(selectedLabel.text.style!.fontFamily, equals('Roboto'));
      expect(selectedLabel.text.style!.fontSize, equals(14.0));
      expect(selectedLabel.text.style!.color, equals(Colors.white));
      final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
      expect(unselectedLabel.text.style!.fontFamily, equals('Roboto'));
      expect(unselectedLabel.text.style!.fontSize, equals(14.0));
      expect(unselectedLabel.text.style!.color, equals(Colors.white.withAlpha(0xB2)));

      // Test default labelPadding.
      await tester.pumpWidget(buildTabBar(tabs: _sizedTabs, isScrollable: true));

      const double indicatorWeight = 2.0;
      final Rect tabBar = tester.getRect(find.byType(TabBar));
      final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
      final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));

      // Verify tabOne coordinates.
      expect(tabOneRect.left, equals(kTabLabelPadding.left));
      expect(tabOneRect.top, equals(kTabLabelPadding.top));
      expect(tabOneRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

      // Verify tabTwo coordinates.
      expect(tabTwoRect.right, equals(tabBar.width - kTabLabelPadding.right));
      expect(tabTwoRect.top, equals(kTabLabelPadding.top));
      expect(tabTwoRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

      // Verify tabOne and tabTwo is separated by right padding of tabOne and left padding of tabTwo.
      expect(tabOneRect.right, equals(tabTwoRect.left - kTabLabelPadding.left - kTabLabelPadding.right));

      // Test default indicator color.
      final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
      expect(tabBarBox, paints..line(color: theme.indicatorColor));
    });

    testWidgetsWithLeakTracking('Tab bar defaults (secondary)', (WidgetTester tester) async {
      // Test default label color and label styles.
      await tester.pumpWidget(buildTabBar(secondaryTabBar: true));

      final ThemeData theme = ThemeData(useMaterial3: false);
      final RenderParagraph selectedLabel = _getText(tester, _tab1Text);
      expect(selectedLabel.text.style!.fontFamily, equals('Roboto'));
      expect(selectedLabel.text.style!.fontSize, equals(14.0));
      expect(selectedLabel.text.style!.color, equals(Colors.white));
      final RenderParagraph unselectedLabel = _getText(tester, _tab2Text);
      expect(unselectedLabel.text.style!.fontFamily, equals('Roboto'));
      expect(unselectedLabel.text.style!.fontSize, equals(14.0));
      expect(unselectedLabel.text.style!.color, equals(Colors.white.withAlpha(0xB2)));

      // Test default labelPadding.
      await tester.pumpWidget(buildTabBar(tabs: _sizedTabs, isScrollable: true));

      const double indicatorWeight = 2.0;
      final Rect tabBar = tester.getRect(find.byType(TabBar));
      final Rect tabOneRect = tester.getRect(find.byKey(_sizedTabs[0].key!));
      final Rect tabTwoRect = tester.getRect(find.byKey(_sizedTabs[1].key!));

      // Verify tabOne coordinates.
      expect(tabOneRect.left, equals(kTabLabelPadding.left));
      expect(tabOneRect.top, equals(kTabLabelPadding.top));
      expect(tabOneRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

      // Verify tabTwo coordinates.
      expect(tabTwoRect.right, equals(tabBar.width - kTabLabelPadding.right));
      expect(tabTwoRect.top, equals(kTabLabelPadding.top));
      expect(tabTwoRect.bottom, equals(tabBar.bottom - kTabLabelPadding.bottom - indicatorWeight));

      // Verify tabOne and tabTwo are separated by right padding of tabOne and left padding of tabTwo.
      expect(tabOneRect.right, equals(tabTwoRect.left - kTabLabelPadding.left - kTabLabelPadding.right));

      // Test default indicator color.
      final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
      expect(tabBarBox, paints..line(color: theme.indicatorColor));
    });

    testWidgetsWithLeakTracking('Tab bar default tab indicator size', (WidgetTester tester) async {
      await tester.pumpWidget(buildTabBar());

      await expectLater(
        find.byKey(_painterKey),
        matchesGoldenFile('tab_bar.m2.default.tab_indicator_size.png'),
      );
    });

    testWidgetsWithLeakTracking('TabBarTheme.indicatorSize provides correct tab indicator (primary)', (WidgetTester tester) async {
      final ThemeData theme = ThemeData(
        tabBarTheme: const TabBarTheme(indicatorSize: TabBarIndicatorSize.tab),
        useMaterial3: false,
      );
      final List<Widget> tabs = List<Widget>.generate(4, (int index) {
        return Tab(text: 'Tab $index');
      });

      final TabController controller = TabController(
        vsync: const TestVSync(),
        length: tabs.length,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          theme: theme,
          home: Scaffold(
            body: Container(
              alignment: Alignment.topLeft,
              child: TabBar(
                controller: controller,
                tabs: tabs,
              ),
            ),
          ),
        ),
      );

      final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
      expect(tabBarBox.size.height, 48.0);

      const double indicatorWeight = 2.0;
      const double indicatorY = 48 - (indicatorWeight / 2.0);
      const double indicatorLeft =  indicatorWeight / 2.0;
      const double indicatorRight = 200.0 - (indicatorWeight / 2.0);

      expect(
        tabBarBox,
        paints
          // Tab indicator
          ..line(
            color: theme.indicatorColor,
            strokeWidth: indicatorWeight,
            p1: const Offset(indicatorLeft, indicatorY),
            p2: const Offset(indicatorRight, indicatorY),
          ),
      );
    });

    testWidgetsWithLeakTracking('TabBarTheme.indicatorSize provides correct tab indicator (secondary)', (WidgetTester tester) async {
      final ThemeData theme = ThemeData(
        tabBarTheme: const TabBarTheme(indicatorSize: TabBarIndicatorSize.label),
        useMaterial3: false,
      );
      final List<Widget> tabs = List<Widget>.generate(4, (int index) {
        return Tab(text: 'Tab $index');
      });

      final TabController controller = TabController(
        vsync: const TestVSync(),
        length: tabs.length,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          theme: theme,
          home: Scaffold(
            body: Container(
              alignment: Alignment.topLeft,
              child: TabBar.secondary(
                controller: controller,
                tabs: tabs,
              ),
            ),
          ),
        ),
      );

      final RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
      expect(tabBarBox.size.height, 48.0);

      const double indicatorWeight = 2.0;
      const double indicatorY = 48 - (indicatorWeight / 2.0);

      expect(
        tabBarBox,
        paints
          // Tab indicator
          ..line(
            color: theme.indicatorColor,
            strokeWidth: indicatorWeight,
            p1: const Offset(66.0, indicatorY),
            p2: const Offset(134.0, indicatorY),
          ),
      );
    });

    testWidgetsWithLeakTracking('TabBar respects TabBarTheme.tabAlignment', (WidgetTester tester) async {
      final TabController controller = TabController(
        length: 2,
        vsync: const TestVSync(),
      );
      addTearDown(controller.dispose);

      // Test non-scrollable tab bar.
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(
            tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.center),
            useMaterial3: false,
          ),
          home: Scaffold(
            appBar: AppBar(
              bottom: TabBar(
                controller: controller,
                tabs: const <Widget>[
                  Tab(text: 'Tab 1'),
                  Tab(text: 'Tab 3'),
                ],
              ),
            ),
          ),
        ),
      );

      final Rect tabOneRect = tester.getRect(find.byType(Tab).first);
      final Rect tabTwoRect = tester.getRect(find.byType(Tab).last);

      final double tabOneLeft = (800 / 2) - tabOneRect.width - kTabLabelPadding.left;
      expect(tabOneRect.left, equals(tabOneLeft));
      final double tabTwoRight = (800 / 2) + tabTwoRect.width + kTabLabelPadding.right;
      expect(tabTwoRect.right, equals(tabTwoRight));
    });

    testWidgetsWithLeakTracking('TabBar.tabAlignment overrides TabBarTheme.tabAlignment', (WidgetTester tester) async {
      final TabController controller = TabController(
        length: 2,
        vsync: const TestVSync(),
      );
      addTearDown(controller.dispose);

      // Test non-scrollable tab bar.
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(
            tabBarTheme: const TabBarTheme(tabAlignment: TabAlignment.fill),
            useMaterial3: false,
          ),
          home: Scaffold(
            appBar: AppBar(
              bottom: TabBar(
                tabAlignment: TabAlignment.center,
                controller: controller,
                tabs: const <Widget>[
                  Tab(text: 'Tab 1'),
                  Tab(text: 'Tab 3'),
                ],
              ),
            ),
          ),
        ),
      );

      final Rect tabOneRect = tester.getRect(find.byType(Tab).first);
      final Rect tabTwoRect = tester.getRect(find.byType(Tab).last);

      final double tabOneLeft = (800 / 2) - tabOneRect.width - kTabLabelPadding.left;
      expect(tabOneRect.left, equals(tabOneLeft));
      final double tabTwoRight = (800 / 2) + tabTwoRect.width + kTabLabelPadding.right;
      expect(tabTwoRect.right, equals(tabTwoRight));
    });
  });

  testWidgetsWithLeakTracking('Material3 - TabBar indicator respects TabBarTheme.indicatorColor', (WidgetTester tester) async {
    final List<Widget> tabs = List<Widget>.generate(4, (int index) {
      return Tab(text: 'Tab $index');
    });

    final TabController controller = TabController(
      vsync: const TestVSync(),
      length: tabs.length,
    );
    addTearDown(controller.dispose);

    const Color tabBarThemeIndicatorColor = Color(0xffff0000);

    Widget buildTabBar({ required ThemeData theme }) {
      return MaterialApp(
        theme: theme,
        home: Material(
          child: Container(
            alignment: Alignment.topLeft,
            child: TabBar(
              controller: controller,
              tabs: tabs,
            ),
          ),
        ),
      );
    }

    await tester.pumpWidget(buildTabBar(theme: ThemeData(useMaterial3: true)));

    RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox,paints..rrect(color: ThemeData(useMaterial3: true).colorScheme.primary));

    await tester.pumpWidget(buildTabBar(theme: ThemeData(
      useMaterial3: true,
      tabBarTheme: const TabBarTheme(indicatorColor: tabBarThemeIndicatorColor)
    )));
    await tester.pumpAndSettle();

    tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox,paints..rrect(color: tabBarThemeIndicatorColor));
  });

  testWidgetsWithLeakTracking('Material2 - TabBar indicator respects TabBarTheme.indicatorColor', (WidgetTester tester) async {
    final List<Widget> tabs = List<Widget>.generate(4, (int index) {
      return Tab(text: 'Tab $index');
    });

    final TabController controller = TabController(
      vsync: const TestVSync(),
      length: tabs.length,
    );
    addTearDown(controller.dispose);

    const Color themeIndicatorColor = Color(0xffff0000);
    const Color tabBarThemeIndicatorColor = Color(0xffffff00);

    Widget buildTabBar({ Color? themeIndicatorColor, Color? tabBarThemeIndicatorColor }) {
      return MaterialApp(
        theme: ThemeData(
          indicatorColor: themeIndicatorColor,
          tabBarTheme: TabBarTheme(indicatorColor: tabBarThemeIndicatorColor),
          useMaterial3: false,
        ),
        home: Material(
          child: Container(
            alignment: Alignment.topLeft,
            child: TabBar(
              controller: controller,
              tabs: tabs,
            ),
          ),
        ),
      );
    }

    await tester.pumpWidget(buildTabBar(themeIndicatorColor: themeIndicatorColor));

    RenderBox tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox,paints..line(color: themeIndicatorColor));

    await tester.pumpWidget(buildTabBar(tabBarThemeIndicatorColor: tabBarThemeIndicatorColor));
    await tester.pumpAndSettle();

    tabBarBox = tester.firstRenderObject<RenderBox>(find.byType(TabBar));
    expect(tabBarBox,paints..line(color: tabBarThemeIndicatorColor));
  });

   testWidgetsWithLeakTracking('TabBarTheme.labelColor resolves material states', (WidgetTester tester) async {
    const Color selectedColor = Color(0xff00ff00);
    const Color unselectedColor = Color(0xffff0000);
    final MaterialStateColor labelColor = MaterialStateColor.resolveWith((Set<MaterialState> states) {
      if (states.contains(MaterialState.selected)) {
        return selectedColor;
      }
      return unselectedColor;
    });

    final TabBarTheme tabBarTheme = TabBarTheme(labelColor: labelColor);

    // Test labelColor correctly resolves material states.
    await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

    final IconThemeData selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
    final IconThemeData uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
    final TextStyle selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text)).text.style!;
    final TextStyle unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text)).text.style!;

    expect(selectedTabIcon.color, selectedColor);
    expect(uselectedTabIcon.color, unselectedColor);
    expect(selectedTextStyle.color, selectedColor);
    expect(unselectedTextStyle.color, unselectedColor);
  });

  testWidgetsWithLeakTracking('TabBarTheme.labelColor & TabBarTheme.unselectedLabelColor override material state TabBarTheme.labelColor',
    (WidgetTester tester) async {
      const Color selectedStateColor = Color(0xff00ff00);
      const Color unselectedStateColor = Color(0xffff0000);
      final MaterialStateColor labelColor = MaterialStateColor.resolveWith((Set<MaterialState> states) {
        if (states.contains(MaterialState.selected)) {
          return selectedStateColor;
        }
        return unselectedStateColor;
      });
      const Color selectedColor = Color(0xff00ffff);
      const Color unselectedColor = Color(0xffff12ff);

      TabBarTheme tabBarTheme = TabBarTheme(labelColor: labelColor);

      // Test material state label color.
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));

      IconThemeData selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      IconThemeData uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      TextStyle selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text)).text.style!;
      TextStyle unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text)).text.style!;

      expect(selectedTabIcon.color, selectedStateColor);
      expect(uselectedTabIcon.color, unselectedStateColor);
      expect(selectedTextStyle.color, selectedStateColor);
      expect(unselectedTextStyle.color, unselectedStateColor);

      // Test labelColor & unselectedLabelColor override material state labelColor.
      tabBarTheme = const TabBarTheme(
        labelColor: selectedColor,
        unselectedLabelColor: unselectedColor,
      );
      await tester.pumpWidget(buildTabBar(tabBarTheme: tabBarTheme));
      await tester.pumpAndSettle();

      selectedTabIcon = IconTheme.of(tester.element(find.text(_tab1Text)));
      uselectedTabIcon = IconTheme.of(tester.element(find.text(_tab2Text)));
      selectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab1Text)).text.style!;
      unselectedTextStyle = tester.renderObject<RenderParagraph>(find.text(_tab2Text)).text.style!;

      expect(selectedTabIcon.color, selectedColor);
      expect(uselectedTabIcon.color, unselectedColor);
      expect(selectedTextStyle.color, selectedColor);
      expect(unselectedTextStyle.color, unselectedColor);
  });
}
