// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

const double VIEWPORT_HEIGHT = 600;
const double VIEWPORT_WIDTH = 300;

void main() {
  testWidgets('SliverMainAxisGroup is laid out properly', (WidgetTester tester) async {
    final List<int> items = List<int>.generate(20, (int i) => i);
    final ScrollController controller = ScrollController();

    await tester.pumpWidget(
      _buildSliverMainAxisGroup(
        controller: controller,
        slivers: <Widget>[
          _buildSliverList(itemMainAxisExtent: 300, items: items, label: (int item) => Text('Group 0 Tile $item')),
          _buildSliverList(itemMainAxisExtent: 200, items: items, label: (int item) => Text('Group 1 Tile $item')),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(controller.offset, 0);

    expect(find.text('Group 0 Tile 0'), findsOneWidget);
    expect(find.text('Group 0 Tile 1'), findsOneWidget);
    expect(find.text('Group 0 Tile 2'), findsNothing);

    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset = 19 * 300.0;
    controller.jumpTo(scrollOffset);
    await tester.pumpAndSettle();

    expect(controller.offset, scrollOffset);
    expect(find.text('Group 0 Tile 18'), findsNothing);
    expect(find.text('Group 0 Tile 19'), findsOneWidget);
    expect(find.text('Group 1 Tile 0'), findsOneWidget);

    final List<RenderSliverList> renderSlivers = tester.renderObjectList<RenderSliverList>(find.byType(SliverList)).toList();
    final RenderSliverList first = renderSlivers[0];
    final RenderSliverList second = renderSlivers[1];

    expect(first.geometry!.layoutExtent, equals(300.0));
    expect(second.geometry!.layoutExtent, equals(300.0));
    expect(first.geometry!.scrollExtent, equals(20 * 300.0));
    expect(second.geometry!.scrollExtent, equals(20 * 200.0));

    expect((first.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
    expect(first.constraints.scrollOffset, equals(19 * 300.0));
    expect((second.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(1 * 300.0));

    final RenderSliverMainAxisGroup renderGroup =
        tester.renderObject<RenderSliverMainAxisGroup>(find.byType(SliverMainAxisGroup));
    expect(renderGroup.geometry!.scrollExtent, equals(300 * 20 + 200 * 20));
  });

  testWidgets('SliverMainAxisGroup is laid out properly when reversed', (WidgetTester tester) async {
    final List<int> items = List<int>.generate(20, (int i) => i);
    final ScrollController controller = ScrollController();

    await tester.pumpWidget(
      _buildSliverMainAxisGroup(
        controller: controller,
        reverse: true,
        slivers: <Widget>[
          _buildSliverList(itemMainAxisExtent: 300, items: items, label: (int item) => Text('Group 0 Tile $item')),
          _buildSliverList(itemMainAxisExtent: 200, items: items, label: (int item) => Text('Group 1 Tile $item')),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(controller.offset, 0);

    expect(find.text('Group 0 Tile 0'), findsOneWidget);
    expect(find.text('Group 0 Tile 1'), findsOneWidget);
    expect(find.text('Group 0 Tile 2'), findsNothing);

    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset = 19 * 300.0;
    controller.jumpTo(scrollOffset);
    await tester.pumpAndSettle();

    expect(controller.offset, scrollOffset);
    expect(find.text('Group 0 Tile 18'), findsNothing);
    expect(find.text('Group 0 Tile 19'), findsOneWidget);
    expect(find.text('Group 1 Tile 0'), findsOneWidget);

    final List<RenderSliverList> renderSlivers = tester.renderObjectList<RenderSliverList>(find.byType(SliverList)).toList();
    final RenderSliverList first = renderSlivers[0];
    final RenderSliverList second = renderSlivers[1];

    expect(first.geometry!.layoutExtent, equals(300.0));
    expect(second.geometry!.layoutExtent, equals(300.0));
    expect(first.geometry!.scrollExtent, equals(20 * 300.0));
    expect(second.geometry!.scrollExtent, equals(20 * 200.0));

    expect((first.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
    expect(first.constraints.scrollOffset, equals(19 * 300.0));
    expect((second.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(1 * 300.0));

    final RenderSliverMainAxisGroup renderGroup =
        tester.renderObject<RenderSliverMainAxisGroup>(find.byType(SliverMainAxisGroup));
    expect(renderGroup.geometry!.scrollExtent, equals(300 * 20 + 200 * 20));
  });

  testWidgets('SliverMainAxisGroup is laid out properly when horizontal', (WidgetTester tester) async {
    final List<int> items = List<int>.generate(20, (int i) => i);
    final ScrollController controller = ScrollController();

    await tester.pumpWidget(
      _buildSliverMainAxisGroup(
        controller: controller,
        scrollDirection: Axis.horizontal,
        slivers: <Widget>[
          _buildSliverList(itemMainAxisExtent: 300, items: items, label: (int item) => Text('Group 0 Tile $item'), scrollDirection: Axis.horizontal),
          _buildSliverList(itemMainAxisExtent: 200, items: items, label: (int item) => Text('Group 1 Tile $item'), scrollDirection: Axis.horizontal),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(controller.offset, 0);

    expect(find.text('Group 0 Tile 0'), findsOneWidget);
    expect(find.text('Group 0 Tile 1'), findsNothing);

    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset = 19 * 300.0;
    controller.jumpTo(scrollOffset);
    await tester.pumpAndSettle();

    expect(controller.offset, scrollOffset);
    expect(find.text('Group 0 Tile 18'), findsNothing);
    expect(find.text('Group 0 Tile 19'), findsOneWidget);
    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset2 = 20 * 300.0;
    controller.jumpTo(scrollOffset2);
    await tester.pumpAndSettle();
    expect(find.text('Group 0 Tile 19'), findsNothing);
    expect(find.text('Group 1 Tile 0'), findsOneWidget);

    final List<RenderSliverList> renderSlivers = tester.renderObjectList<RenderSliverList>(find.byType(SliverList)).toList();
    final RenderSliverList first = renderSlivers[0];
    final RenderSliverList second = renderSlivers[1];

    expect(first.geometry!.layoutExtent, equals(0.0));
    expect(second.geometry!.layoutExtent, equals(300.0));
    expect(first.geometry!.scrollExtent, equals(20 * 300.0));
    expect(second.geometry!.scrollExtent, equals(20 * 200.0));

    expect((first.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
    expect(first.constraints.scrollOffset, equals(20 * 300.0));
    expect((second.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));

    final RenderSliverMainAxisGroup renderGroup =
        tester.renderObject<RenderSliverMainAxisGroup>(find.byType(SliverMainAxisGroup));
    expect(renderGroup.geometry!.scrollExtent, equals(300 * 20 + 200 * 20));
  });

  testWidgets('SliverMainAxisGroup is laid out properly when horizontal, reversed', (WidgetTester tester) async {
    final List<int> items = List<int>.generate(20, (int i) => i);
    final ScrollController controller = ScrollController();

    await tester.pumpWidget(
      _buildSliverMainAxisGroup(
        controller: controller,
        scrollDirection: Axis.horizontal,
        reverse: true,
        slivers: <Widget>[
          _buildSliverList(itemMainAxisExtent: 300, items: items, label: (int item) => Text('Group 0 Tile $item'), scrollDirection: Axis.horizontal),
          _buildSliverList(itemMainAxisExtent: 200, items: items, label: (int item) => Text('Group 1 Tile $item'), scrollDirection: Axis.horizontal),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(controller.offset, 0);

    expect(find.text('Group 0 Tile 0'), findsOneWidget);
    expect(find.text('Group 0 Tile 1'), findsNothing);

    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset = 19 * 300.0;
    controller.jumpTo(scrollOffset);
    await tester.pumpAndSettle();

    expect(controller.offset, scrollOffset);
    expect(find.text('Group 0 Tile 18'), findsNothing);
    expect(find.text('Group 0 Tile 19'), findsOneWidget);
    expect(find.text('Group 1 Tile 0'), findsNothing);

    const double scrollOffset2 = 20 * 300.0;
    controller.jumpTo(scrollOffset2);
    await tester.pumpAndSettle();
    expect(find.text('Group 0 Tile 19'), findsNothing);
    expect(find.text('Group 1 Tile 0'), findsOneWidget);

    final List<RenderSliverList> renderSlivers = tester.renderObjectList<RenderSliverList>(find.byType(SliverList)).toList();
    final RenderSliverList first = renderSlivers[0];
    final RenderSliverList second = renderSlivers[1];

    expect(first.geometry!.layoutExtent, equals(0.0));
    expect(second.geometry!.layoutExtent, equals(300.0));
    expect(first.geometry!.scrollExtent, equals(20 * 300.0));
    expect(second.geometry!.scrollExtent, equals(20 * 200.0));

    expect((first.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
    expect(first.constraints.scrollOffset, equals(20 * 300.0));
    expect((second.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));

    final RenderSliverMainAxisGroup renderGroup =
        tester.renderObject<RenderSliverMainAxisGroup>(find.byType(SliverMainAxisGroup));
    expect(renderGroup.geometry!.scrollExtent, equals(300 * 20 + 200 * 20));
  });

  testWidgets('Hit test works properly on various parts of SliverMainAxisGroup', (WidgetTester tester) async {
    final List<int> items = List<int>.generate(20, (int i) => i);
    final ScrollController controller = ScrollController();

    String? clickedTile;

    int group = 0;
    int tile = 0;

    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        _buildSliverList(
          itemMainAxisExtent: 300,
          items: items,
          label: (int item) => tile == item && group == 0
            ? TextButton(
              onPressed: () => clickedTile = 'Group 0 Tile $item',
              child: Text('Group 0 Tile $item'),
            )
            : Text('Group 0 Tile $item'),
        ),
        _buildSliverList(
          items: items,
          label: (int item) => tile == item && group == 1
            ? TextButton(
              onPressed: () => clickedTile = 'Group 1 Tile $item',
              child: Text('Group 1 Tile $item'),
            )
            : Text('Group 1 Tile $item'),
        ),
      ]),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byType(TextButton));
    await tester.pumpAndSettle();
    expect(clickedTile, equals('Group 0 Tile 0'));

    clickedTile = null;
    group = 1;
    tile = 2;
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        _buildSliverList(
          itemMainAxisExtent: 300,
          items: items,
          label: (int item) => tile == item && group == 0
            ? TextButton(
              onPressed: () => clickedTile = 'Group 0 Tile $item',
              child: Text('Group 0 Tile $item'),
            )
            : Text('Group 0 Tile $item'),
        ),
        _buildSliverList(
          items: items,
          label: (int item) => tile == item && group == 1
            ? TextButton(
              onPressed: () => clickedTile = 'Group 1 Tile $item',
              child: Text('Group 1 Tile $item'),
            )
            : Text('Group 1 Tile $item'),
        ),
      ]),
    );
    controller.jumpTo(300.0 * 20);
    await tester.pumpAndSettle();
    await tester.tap(find.byType(TextButton));
    await tester.pumpAndSettle();
    expect(clickedTile, equals('Group 1 Tile 2'));
  });

  testWidgets('applyPaintTransform is implemented properly', (WidgetTester tester) async {
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      slivers: <Widget>[
        const SliverToBoxAdapter(child: Text('first box')),
        const SliverToBoxAdapter(child: Text('second box')),
      ]),
    );
    await tester.pumpAndSettle();

    // localToGlobal calculates offset via applyPaintTransform
    final RenderBox first = tester.renderObject(find.text('first box')) as RenderBox;
    final RenderBox second = tester.renderObject(find.text('second box'));
    expect(first.localToGlobal(Offset.zero), Offset.zero);
    expect(second.localToGlobal(Offset.zero), Offset(0, first.size.height));
  });

  testWidgets('visitChildrenForSemantics visits children in the correct order', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: const <Widget>[
        SliverToBoxAdapter(child: SizedBox(height: 200)),
        SliverToBoxAdapter(child: SizedBox(height: 300)),
        SliverToBoxAdapter(child: SizedBox(height: 500)),
        SliverToBoxAdapter(child: SizedBox(height: 400)),
      ]),
    );
    controller.jumpTo(300);
    await tester.pumpAndSettle();

    final List<RenderSliver> visitedChildren = <RenderSliver>[];
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject<RenderSliverMainAxisGroup>(find.byType(SliverMainAxisGroup));
    void visitor(RenderObject child) {
      visitedChildren.add(child as RenderSliver);
    }
    renderGroup.visitChildrenForSemantics(visitor);
    expect(visitedChildren.length, equals(2));
    expect(visitedChildren[0].geometry!.scrollExtent, equals(300));
    expect(visitedChildren[1].geometry!.scrollExtent, equals(500));
  });

  testWidgets('SliverPinnedPersistentHeader is painted within bounds of SliverMainAxisGroup', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        SliverPersistentHeader(
          delegate: TestDelegate(),
          pinned: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    // Scroll extent is the total of the box sliver and the sliver persistent header.
    expect(renderGroup.geometry!.scrollExtent, equals(600.0 + 60.0));
    controller.jumpTo(620);
    await tester.pumpAndSettle();
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    // Paint extent after header's layout is 60.0, so we must offset by -20.0 to fit within the 40.0 remaining extent.
    expect(renderHeader.geometry!.paintExtent, equals(60.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-20.0));
  });


  testWidgets('SliverFloatingPersistentHeader is painted within bounds of SliverMainAxisGroup', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        SliverPersistentHeader(
          delegate: TestDelegate(),
          floating: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    expect(renderGroup.geometry!.scrollExtent, equals(660));
    controller.jumpTo(660.0);
    await tester.pumpAndSettle();
    final TestGesture gesture = await tester.startGesture(const Offset(150.0, 300.0));
    await gesture.moveBy(const Offset(0.0, 40));
    await tester.pump();
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    // Paint extent after header's layout is 40.0, so no need to correct the paintOffset.
    expect(renderHeader.geometry!.paintExtent, equals(40.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
  });

  testWidgets('SliverPinnedPersistentHeader is painted within bounds of SliverMainAxisGroup with different minExtent/maxExtent', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        SliverPersistentHeader(
          delegate: TestDelegate(minExtent: 40.0),
          pinned: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderGroup.geometry!.scrollExtent, equals(660));
    controller.jumpTo(630);
    await tester.pumpAndSettle();
    // Paint extent of the header is 40.0, so we must provide an offset of -10.0 to make it fit in the 30.0 remaining paint extent of the group.
    expect(renderHeader.geometry!.paintExtent, equals(40.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-10.0));
    controller.jumpTo(610);
    await tester.pumpAndSettle();
    expect(renderHeader.geometry!.paintExtent, equals(40.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
  });

  testWidgets('SliverFloatingPersistentHeader is painted within bounds of SliverMainAxisGroup with different minExtent/maxExtent', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        SliverPersistentHeader(
          delegate: TestDelegate(minExtent: 40.0),
          floating: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderGroup.geometry!.scrollExtent, equals(660));

    controller.jumpTo(660);
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(const Offset(150.0, 300.0));
    await gesture.moveBy(const Offset(0.0, 30.0));
    await tester.pump();
    // Paint extent after header's layout is 30.0, so no need to correct the paintOffset.
    expect(renderHeader.geometry!.paintExtent, equals(30.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
    // Floating headers should expand to maximum extent as we continue scrolling.
    await gesture.moveBy(const Offset(0.0, 20.0));
    await tester.pump();
    expect(renderHeader.geometry!.paintExtent, equals(50.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
  });

  testWidgets('SliverPinnedFloatingPersistentHeader is painted within bounds of SliverMainAxisGroup with different minExtent/maxExtent', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        SliverPersistentHeader(
          delegate: TestDelegate(minExtent: 40.0),
          pinned: true,
          floating: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderGroup.geometry!.scrollExtent, equals(660));

    controller.jumpTo(660);
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(const Offset(150.0, 300.0));
    await gesture.moveBy(const Offset(0.0, 30.0));
    await tester.pump();
    // Paint extent after header's layout is 40.0, so we need to adjust by -10.0.
    expect(renderHeader.geometry!.paintExtent, equals(40.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-10.0));
    // Pinned floating headers should expand to maximum extent as we continue scrolling.
    await gesture.moveBy(const Offset(0.0, 20.0));
    await tester.pump();
    expect(renderHeader.geometry!.paintExtent, equals(50.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));
  });

  testWidgets('SliverAppBar with floating: false, pinned: false, snap: false is painted within bounds of SliverMainAxisGroup', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        const SliverAppBar(
          toolbarHeight: 30,
          expandedHeight: 60,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    expect(renderGroup.geometry!.scrollExtent, equals(660));

    controller.jumpTo(660);
    await tester.pumpAndSettle();
    controller.jumpTo(630);
    await tester.pumpAndSettle();

    // At a scroll offset of 630, a normal scrolling header should be out of view.
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderHeader.constraints.scrollOffset, equals(630));
    expect(renderHeader.geometry!.layoutExtent, equals(0.0));
  });

    testWidgets('SliverAppBar with floating: true, pinned: false, snap: true is painted within bounds of SliverMainAxisGroup', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        const SliverAppBar(
          toolbarHeight: 30,
          expandedHeight: 60,
          floating: true,
          snap: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderGroup.geometry!.scrollExtent, equals(660));

    controller.jumpTo(660);
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(const Offset(150.0, 300.0));
    await gesture.moveBy(const Offset(0.0, 10));
    await tester.pump();

    // The snap animation does not go through until the gesture is released.
    expect(renderHeader.geometry!.paintExtent, equals(10));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(0.0));

    // Once it is released, the header's paint extent becomes the maximum and the group sets an offset of -50.0.
    await gesture.up();
    await tester.pumpAndSettle();
    expect(renderHeader.geometry!.paintExtent, equals(60));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-50.0));
  });

  testWidgets('SliverAppBar with floating: true, pinned: true, snap: true is painted within bounds of SliverMainAxisGroup', (WidgetTester tester) async {
    final ScrollController controller = ScrollController();
    await tester.pumpWidget(_buildSliverMainAxisGroup(
      controller: controller,
      slivers: <Widget>[
        const SliverAppBar(
          toolbarHeight: 30,
          expandedHeight: 60,
          floating: true,
          pinned: true,
          snap: true,
        ),
        const SliverToBoxAdapter(child: SizedBox(height: 600)),
      ],
      otherSlivers: <Widget>[
        const SliverToBoxAdapter(child: SizedBox(height: 2400)),
      ],
    ));
    await tester.pumpAndSettle();
    final RenderSliverMainAxisGroup renderGroup = tester.renderObject(find.byType(SliverMainAxisGroup)) as RenderSliverMainAxisGroup;
    final RenderSliverPersistentHeader renderHeader = tester.renderObject(find.byType(SliverPersistentHeader)) as RenderSliverPersistentHeader;
    expect(renderGroup.geometry!.scrollExtent, equals(660));

    controller.jumpTo(660);
    await tester.pumpAndSettle();

    final TestGesture gesture = await tester.startGesture(const Offset(150.0, 300.0));
    await gesture.moveBy(const Offset(0.0, 10));
    await tester.pump();

    expect(renderHeader.geometry!.paintExtent, equals(30.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-20.0));

    // Once we lift the gesture up, the animation should finish.
    await gesture.up();
    await tester.pumpAndSettle();
    expect(renderHeader.geometry!.paintExtent, equals(60.0));
    expect((renderHeader.parentData! as SliverPhysicalParentData).paintOffset.dy, equals(-50.0));
  });
}

Widget _buildSliverList({
  double itemMainAxisExtent = 100,
  List<int> items = const <int>[],
  required Widget Function(int) label,
  Axis scrollDirection = Axis.vertical,
}) {
  return SliverList(
    delegate: SliverChildBuilderDelegate(
      (BuildContext context, int i) {
        return scrollDirection == Axis.vertical
          ? SizedBox(
            key: ValueKey<int>(items[i]),
            height: itemMainAxisExtent,
            child: label(items[i]),
          )
          : SizedBox(
            key: ValueKey<int>(items[i]),
            width: itemMainAxisExtent,
            child: label(items[i]));
      },
      findChildIndexCallback: (Key key) {
        final ValueKey<int> valueKey = key as ValueKey<int>;
        final int index = items.indexOf(valueKey.value);
        return index == -1 ? null : index;
      },
      childCount: items.length,
    ),
  );
}

Widget _buildSliverMainAxisGroup({
  required List<Widget> slivers,
  ScrollController? controller,
  double viewportHeight = VIEWPORT_HEIGHT,
  double viewportWidth = VIEWPORT_WIDTH,
  Axis scrollDirection = Axis.vertical,
  bool reverse = false,
  List<Widget> otherSlivers = const <Widget>[],
}) {
  return MaterialApp(
    home: Directionality(
      textDirection: TextDirection.ltr,
      child: Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          height: viewportHeight,
          width: viewportWidth,
          child: CustomScrollView(
            scrollDirection: scrollDirection,
            reverse: reverse,
            controller: controller,
            slivers: <Widget>[SliverMainAxisGroup(slivers: slivers), ...otherSlivers],
          ),
        ),
      ),
      ),
  );
}

class TestDelegate extends SliverPersistentHeaderDelegate {
  TestDelegate({ this.maxExtent = 60.0, this.minExtent = 60.0 });

  @override
  final double maxExtent;

  @override
  final double minExtent;

  @override
  Widget build(BuildContext context, double shrinkOffset, bool overlapsContent) {
    return Container(height: maxExtent);
  }

  @override
  bool shouldRebuild(TestDelegate oldDelegate) => true;
}
