// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:leak_tracker_flutter_testing/leak_tracker_flutter_testing.dart';

import 'semantics_tester.dart';

void main() {
  test('OverlayEntry dispatches memory events', () async {
    await expectLater(
      await memoryEvents(
        () => OverlayEntry(
              builder: (BuildContext context) => Container(),
            ).dispose(),
        OverlayEntry,
      ),
      areCreateAndDispose,
    );
  });

  testWidgetsWithLeakTracking('OverflowEntries context contains Overlay', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    bool didBuild = false;
    late final OverlayEntry overlayEntry1;
    addTearDown(() => overlayEntry1..remove()..dispose());
    late final OverlayEntry overlayEntry2;
    addTearDown(() => overlayEntry2..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            overlayEntry1 = OverlayEntry(
              builder: (BuildContext context) {
                didBuild = true;
                final Overlay overlay = context.findAncestorWidgetOfExactType<Overlay>()!;
                expect(overlay.key, equals(overlayKey));
                return Container();
              },
            ),
            overlayEntry2 = OverlayEntry(
              builder: (BuildContext context) => Container(),
            ),
          ],
        ),
      ),
    );
    expect(didBuild, isTrue);
    final RenderObject theater = overlayKey.currentContext!.findRenderObject()!;

    expect(theater, hasAGoodToStringDeep);
    expect(
      theater.toStringDeep(minLevel: DiagnosticLevel.info),
      equalsIgnoringHashCodes(
        '_RenderTheater#744c9\n'
        ' │ parentData: <none>\n'
        ' │ constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' │ size: Size(800.0, 600.0)\n'
        ' │ skipCount: 0\n'
        ' │ textDirection: ltr\n'
        ' │\n'
        ' ├─onstage 1: RenderLimitedBox#bb803\n'
        ' │ │ parentData: not positioned; offset=Offset(0.0, 0.0) (can use\n'
        ' │ │   size)\n'
        ' │ │ constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' │ │ size: Size(800.0, 600.0)\n'
        ' │ │ maxWidth: 0.0\n'
        ' │ │ maxHeight: 0.0\n'
        ' │ │\n'
        ' │ └─child: RenderConstrainedBox#62707\n'
        ' │     parentData: <none> (can use size)\n'
        ' │     constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' │     size: Size(800.0, 600.0)\n'
        ' │     additionalConstraints: BoxConstraints(biggest)\n'
        ' │\n'
        ' ├─onstage 2: RenderLimitedBox#af5f1\n'
        ' ╎ │ parentData: not positioned; offset=Offset(0.0, 0.0) (can use\n'
        ' ╎ │   size)\n'
        ' ╎ │ constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' ╎ │ size: Size(800.0, 600.0)\n'
        ' ╎ │ maxWidth: 0.0\n'
        ' ╎ │ maxHeight: 0.0\n'
        ' ╎ │\n'
        ' ╎ └─child: RenderConstrainedBox#69c48\n'
        ' ╎     parentData: <none> (can use size)\n'
        ' ╎     constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' ╎     size: Size(800.0, 600.0)\n'
        ' ╎     additionalConstraints: BoxConstraints(biggest)\n'
        ' ╎\n'
        ' └╌no offstage children\n',
      ),
    );
  });

  testWidgetsWithLeakTracking('Offstage overlay', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    late final OverlayEntry overlayEntry1;
    addTearDown(() => overlayEntry1..remove()..dispose());
    late final OverlayEntry overlayEntry2;
    addTearDown(() => overlayEntry2..remove()..dispose());
    late final OverlayEntry overlayEntry3;
    addTearDown(() => overlayEntry3..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            overlayEntry1 = OverlayEntry(
              opaque: true,
              maintainState: true,
              builder: (BuildContext context) => Container(),
            ),
            overlayEntry2 = OverlayEntry(
              opaque: true,
              maintainState: true,
              builder: (BuildContext context) => Container(),
            ),
            overlayEntry3 = OverlayEntry(
              opaque: true,
              maintainState: true,
              builder: (BuildContext context) => Container(),
            ),
          ],
        ),
      ),
    );
    final RenderObject theater = overlayKey.currentContext!.findRenderObject()!;

    expect(theater, hasAGoodToStringDeep);
    expect(
      theater.toStringDeep(minLevel: DiagnosticLevel.info),
      equalsIgnoringHashCodes(
        '_RenderTheater#385b3\n'
        ' │ parentData: <none>\n'
        ' │ constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' │ size: Size(800.0, 600.0)\n'
        ' │ skipCount: 2\n'
        ' │ textDirection: ltr\n'
        ' │\n'
        ' ├─onstage 1: RenderLimitedBox#0a77a\n'
        ' ╎ │ parentData: not positioned; offset=Offset(0.0, 0.0) (can use\n'
        ' ╎ │   size)\n'
        ' ╎ │ constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' ╎ │ size: Size(800.0, 600.0)\n'
        ' ╎ │ maxWidth: 0.0\n'
        ' ╎ │ maxHeight: 0.0\n'
        ' ╎ │\n'
        ' ╎ └─child: RenderConstrainedBox#21f3a\n'
        ' ╎     parentData: <none> (can use size)\n'
        ' ╎     constraints: BoxConstraints(w=800.0, h=600.0)\n'
        ' ╎     size: Size(800.0, 600.0)\n'
        ' ╎     additionalConstraints: BoxConstraints(biggest)\n'
        ' ╎\n'
        ' ╎╌offstage 1: RenderLimitedBox#62c8c NEEDS-LAYOUT NEEDS-PAINT\n'
        ' ╎ │ parentData: not positioned; offset=Offset(0.0, 0.0)\n'
        ' ╎ │ constraints: MISSING\n'
        ' ╎ │ size: MISSING\n'
        ' ╎ │ maxWidth: 0.0\n'
        ' ╎ │ maxHeight: 0.0\n'
        ' ╎ │\n'
        ' ╎ └─child: RenderConstrainedBox#425fa NEEDS-LAYOUT NEEDS-PAINT\n'
        ' ╎     parentData: <none>\n'
        ' ╎     constraints: MISSING\n'
        ' ╎     size: MISSING\n'
        ' ╎     additionalConstraints: BoxConstraints(biggest)\n'
        ' ╎\n'
        ' └╌offstage 2: RenderLimitedBox#03ae2 NEEDS-LAYOUT NEEDS-PAINT\n'
        '   │ parentData: not positioned; offset=Offset(0.0, 0.0)\n'
        '   │ constraints: MISSING\n'
        '   │ size: MISSING\n'
        '   │ maxWidth: 0.0\n'
        '   │ maxHeight: 0.0\n'
        '   │\n'
        '   └─child: RenderConstrainedBox#b4d48 NEEDS-LAYOUT NEEDS-PAINT\n'
        '       parentData: <none>\n'
        '       constraints: MISSING\n'
        '       size: MISSING\n'
        '       additionalConstraints: BoxConstraints(biggest)\n',
      ),
    );
  });

  testWidgetsWithLeakTracking('insert top', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<String> buildOrder = <String>[];
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base']);

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    overlay.insert(
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New');
          return Container();
        },
      ),
    );
    await tester.pump();

    expect(buildOrder, <String>['Base', 'New']);
  });

  testWidgetsWithLeakTracking('insert below', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());
    final List<String> buildOrder = <String>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base']);

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    overlay.insert(
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New');
          return Container();
        },
      ),
      below: baseEntry,
    );
    await tester.pump();

    expect(buildOrder, <String>['New', 'Base']);
  });

  testWidgetsWithLeakTracking('insert above', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());
    late final OverlayEntry topEntry;
    addTearDown(() => topEntry..remove()..dispose());
    final List<String> buildOrder = <String>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
            topEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Top');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base', 'Top']);

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    overlay.insert(
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New');
          return Container();
        },
      ),
      above: baseEntry,
    );
    await tester.pump();

    expect(buildOrder, <String>['Base', 'New', 'Top']);
  });

  testWidgetsWithLeakTracking('insertAll top', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<String> buildOrder = <String>[];
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base']);

    final List<OverlayEntry> entries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New1');
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New2');
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in entries) {
        entry..remove()..dispose();
      }
    });

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.insertAll(entries);
    await tester.pump();

    expect(buildOrder, <String>['Base', 'New1', 'New2']);
  });

  testWidgetsWithLeakTracking('insertAll below', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());
    final List<String> buildOrder = <String>[];

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base']);

    final List<OverlayEntry> entries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New1');
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New2');
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in entries) {
        entry..remove()..dispose();
      }
    });

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.insertAll(entries, below: baseEntry);
    await tester.pump();

    expect(buildOrder, <String>['New1', 'New2','Base']);
  });

  testWidgetsWithLeakTracking('insertAll above', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<String> buildOrder = <String>[];
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());
    late final OverlayEntry topEntry;
    addTearDown(() => topEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Base');
                return Container();
              },
            ),
            topEntry = OverlayEntry(
              builder: (BuildContext context) {
                buildOrder.add('Top');
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(buildOrder, <String>['Base', 'Top']);

    final List<OverlayEntry> entries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New1');
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add('New2');
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in entries) {
        entry..remove()..dispose();
      }
    });

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.insertAll(entries, above: baseEntry);
    await tester.pump();

    expect(buildOrder, <String>['Base', 'New1', 'New2', 'Top']);
  });

  testWidgetsWithLeakTracking('rearrange', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<int> buildOrder = <int>[];
    final List<OverlayEntry> initialEntries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(0);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(1);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(2);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(3);
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in initialEntries) {
        entry..remove()..dispose();
      }
    });

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: initialEntries,
        ),
      ),
    );

    expect(buildOrder, <int>[0, 1, 2, 3]);

    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    final List<OverlayEntry> rearranged = <OverlayEntry>[
      initialEntries[3],
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(4);
          return Container();
        },
      ),
      initialEntries[2],
      // 1 intentionally missing, will end up on top
      initialEntries[0],
    ];

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.rearrange(rearranged);
    await tester.pump();

    expect(buildOrder, <int>[3, 4, 2, 0, 1]);
  });

  testWidgetsWithLeakTracking('rearrange above', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<int> buildOrder = <int>[];

    final List<OverlayEntry> initialEntries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(0);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(1);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(2);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(3);
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in initialEntries) {
        entry..remove()..dispose();
      }
    });

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: initialEntries,
        ),
      ),
    );

    expect(buildOrder, <int>[0, 1, 2, 3]);

    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    final List<OverlayEntry> rearranged = <OverlayEntry>[
      initialEntries[3],
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(4);
          return Container();
        },
      ),
      initialEntries[2],
      // 1 intentionally missing
      initialEntries[0],
    ];

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.rearrange(rearranged, above: initialEntries[2]);
    await tester.pump();

    expect(buildOrder, <int>[3, 4, 2, 1, 0]);
  });

  testWidgetsWithLeakTracking('rearrange below', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    final List<int> buildOrder = <int>[];
    final List<OverlayEntry> initialEntries = <OverlayEntry>[
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(0);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(1);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(2);
          return Container();
        },
      ),
      OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(3);
          return Container();
        },
      ),
    ];
    addTearDown(() {
      for (final OverlayEntry entry in initialEntries) {
        entry..remove()..dispose();
      }
    });

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: initialEntries,
        ),
      ),
    );

    expect(buildOrder, <int>[0, 1, 2, 3]);

    late final OverlayEntry newEntry;
    addTearDown(() => newEntry..remove()..dispose());
    final List<OverlayEntry> rearranged = <OverlayEntry>[
      initialEntries[3],
      newEntry = OverlayEntry(
        builder: (BuildContext context) {
          buildOrder.add(4);
          return Container();
        },
      ),
      initialEntries[2],
      // 1 intentionally missing
      initialEntries[0],
    ];

    buildOrder.clear();
    final OverlayState overlay = overlayKey.currentState! as OverlayState;
    overlay.rearrange(rearranged, below: initialEntries[2]);
    await tester.pump();

    expect(buildOrder, <int>[3, 4, 1, 2, 0]);
  });

  testWidgets('debugVerifyInsertPosition', (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    OverlayEntry base;

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            base = OverlayEntry(
              builder: (BuildContext context) {
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    final OverlayState overlay = overlayKey.currentState! as OverlayState;

    try {
      overlay.insert(
        OverlayEntry(builder: (BuildContext context) {
          return Container();
        }),
        above: OverlayEntry(
          builder: (BuildContext context) {
            return Container();
          },
        ),
        below: OverlayEntry(
          builder: (BuildContext context) {
            return Container();
          },
        ),
      );
    } on AssertionError catch (e) {
      expect(e.message, 'Only one of `above` and `below` may be specified.');
    }

    expect(() => overlay.insert(
      OverlayEntry(builder: (BuildContext context) {
        return Container();
      }),
      above: base,
    ), isNot(throwsAssertionError));

    try {
      overlay.insert(
        OverlayEntry(builder: (BuildContext context) {
          return Container();
        }),
        above: OverlayEntry(
          builder: (BuildContext context) {
            return Container();
          },
        ),
      );
    } on AssertionError catch (e) {
      expect(e.message, 'The provided entry used for `above` must be present in the Overlay.');
    }

    try {
      overlay.rearrange(<OverlayEntry>[base], above: OverlayEntry(
        builder: (BuildContext context) {
          return Container();
        },
      ));

    } on AssertionError catch (e) {
      expect(e.message, 'The provided entry used for `above` must be present in the Overlay and in the `newEntriesList`.');
    }

    await tester.pump();
  });

  testWidgetsWithLeakTracking('OverlayState.of() throws when called if an Overlay does not exist', (WidgetTester tester) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Builder(
          builder: (BuildContext context) {
            late FlutterError error;
            final Widget debugRequiredFor = Container();
            try {
              Overlay.of(context, debugRequiredFor: debugRequiredFor);
            } on FlutterError catch (e) {
              error = e;
            } finally {
              expect(error, isNotNull);
              expect(error.diagnostics.length, 5);
              expect(error.diagnostics[2].level, DiagnosticLevel.hint);
              expect(error.diagnostics[2].toStringDeep(), equalsIgnoringHashCodes(
                'The most common way to add an Overlay to an application is to\n'
                'include a MaterialApp, CupertinoApp or Navigator widget in the\n'
                'runApp() call.\n'
              ));
              expect(error.diagnostics[3], isA<DiagnosticsProperty<Widget>>());
              expect(error.diagnostics[3].value, debugRequiredFor);
              expect(error.diagnostics[4], isA<DiagnosticsProperty<Element>>());
              expect(error.toStringDeep(), equalsIgnoringHashCodes(
                'FlutterError\n'
                '   No Overlay widget found.\n'
                '   Container widgets require an Overlay widget ancestor for correct\n'
                '   operation.\n'
                '   The most common way to add an Overlay to an application is to\n'
                '   include a MaterialApp, CupertinoApp or Navigator widget in the\n'
                '   runApp() call.\n'
                '   The specific widget that failed to find an overlay was:\n'
                '     Container\n'
                '   The context from which that widget was searching for an overlay\n'
                '   was:\n'
                '     Builder\n'
              ));
            }
            return Container();
          },
        ),
      ),
    );
  });

  testWidgetsWithLeakTracking("OverlayState.maybeOf() works when an Overlay does and doesn't exist", (WidgetTester tester) async {
    final GlobalKey overlayKey = GlobalKey();
    OverlayState? foundState;
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                foundState = Overlay.maybeOf(context);
                return Container();
              },
            ),
          ],
        ),
      ),
    );

    expect(tester.takeException(), isNull);
    expect(foundState, isNotNull);
    foundState = null;

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Builder(
          builder: (BuildContext context) {
            foundState = Overlay.maybeOf(context);
            return const SizedBox();
          },
        ),
      ),
    );

    expect(tester.takeException(), isNull);
    expect(foundState, isNull);
  });

  testWidgetsWithLeakTracking('OverlayEntry.opaque can be changed when OverlayEntry is not part of an Overlay (yet)', (WidgetTester tester) async {
    final GlobalKey<OverlayState> overlayKey = GlobalKey<OverlayState>();
    final Key root = UniqueKey();
    final Key top = UniqueKey();
    final OverlayEntry rootEntry = OverlayEntry(
      builder: (BuildContext context) {
        return Container(key: root);
      },
    );
    addTearDown(() => rootEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            rootEntry,
          ],
        ),
      ),
    );

    expect(find.byKey(root), findsOneWidget);

    final OverlayEntry newEntry = OverlayEntry(
      builder: (BuildContext context) {
        return Container(key: top);
      },
    );
    addTearDown(() => newEntry..remove()..dispose());
    expect(newEntry.opaque, isFalse);
    newEntry.opaque = true; // Does neither trigger an assert nor throw.
    expect(newEntry.opaque, isTrue);

    // The new opaqueness is honored when inserted into an overlay.
    overlayKey.currentState!.insert(newEntry);
    await tester.pumpAndSettle();

    expect(find.byKey(root), findsNothing);
    expect(find.byKey(top), findsOneWidget);
  });

  testWidgetsWithLeakTracking('OverlayEntries do not rebuild when opaqueness changes', (WidgetTester tester) async {
    // Regression test for https://github.com/flutter/flutter/issues/45797.

    final GlobalKey<OverlayState> overlayKey = GlobalKey<OverlayState>();
    final Key bottom = UniqueKey();
    final Key middle = UniqueKey();
    final Key top = UniqueKey();
    final Widget bottomWidget = StatefulTestWidget(key: bottom);
    final Widget middleWidget = StatefulTestWidget(key: middle);
    final Widget topWidget = StatefulTestWidget(key: top);

    final OverlayEntry bottomEntry = OverlayEntry(
      maintainState: true,
      builder: (BuildContext context) {
        return bottomWidget;
      },
    );
    addTearDown(() => bottomEntry..remove()..dispose());
    final OverlayEntry middleEntry = OverlayEntry(
      maintainState: true,
      builder: (BuildContext context) {
        return middleWidget;
      },
    );
    addTearDown(() => middleEntry..remove()..dispose());
    final OverlayEntry topEntry = OverlayEntry(
      maintainState: true,
      builder: (BuildContext context) {
        return topWidget;
      },
    );
    addTearDown(() => topEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            bottomEntry,
            middleEntry,
            topEntry,
          ],
        ),
      ),
    );

    // All widgets are onstage.
    expect(tester.state<StatefulTestState>(find.byKey(bottom)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(middle)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(top)).rebuildCount, 1);

    middleEntry.opaque = true;
    await tester.pump();

    // Bottom widget is offstage and did not rebuild.
    expect(find.byKey(bottom), findsNothing);
    expect(tester.state<StatefulTestState>(find.byKey(bottom, skipOffstage: false)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(middle)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(top)).rebuildCount, 1);
  });

  testWidgetsWithLeakTracking('OverlayEntries do not rebuild when opaque entry is added', (WidgetTester tester) async {
    // Regression test for https://github.com/flutter/flutter/issues/45797.

    final GlobalKey<OverlayState> overlayKey = GlobalKey<OverlayState>();
    final Key bottom = UniqueKey();
    final Key middle = UniqueKey();
    final Key top = UniqueKey();
    final Widget bottomWidget = StatefulTestWidget(key: bottom);
    final Widget middleWidget = StatefulTestWidget(key: middle);
    final Widget topWidget = StatefulTestWidget(key: top);

    final OverlayEntry bottomEntry = OverlayEntry(
      maintainState: true,
      builder: (BuildContext context) {
        return bottomWidget;
      },
    );
    addTearDown(() => bottomEntry..remove()..dispose());
    final OverlayEntry middleEntry = OverlayEntry(
      opaque: true,
      maintainState: true,
      builder: (BuildContext context) {
        return middleWidget;
      },
    );
    addTearDown(() => middleEntry..remove()..dispose());
    final OverlayEntry topEntry = OverlayEntry(
      maintainState: true,
      builder: (BuildContext context) {
        return topWidget;
      },
    );
    addTearDown(() => topEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            bottomEntry,
            topEntry,
          ],
        ),
      ),
    );

    // Both widgets are onstage.
    expect(tester.state<StatefulTestState>(find.byKey(bottom)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(top)).rebuildCount, 1);

    overlayKey.currentState!.rearrange(<OverlayEntry>[
      bottomEntry, middleEntry, topEntry,
    ]);
    await tester.pump();

    // Bottom widget is offstage and did not rebuild.
    expect(find.byKey(bottom), findsNothing);
    expect(tester.state<StatefulTestState>(find.byKey(bottom, skipOffstage: false)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(middle)).rebuildCount, 1);
    expect(tester.state<StatefulTestState>(find.byKey(top)).rebuildCount, 1);
  });

  testWidgetsWithLeakTracking('entries below opaque entries are ignored for hit testing', (WidgetTester tester) async {
    final GlobalKey<OverlayState> overlayKey = GlobalKey<OverlayState>();
    int bottomTapCount = 0;
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              maintainState: true,
              builder: (BuildContext context) {
                return GestureDetector(
                  onTap: () {
                    bottomTapCount++;
                  },
                );
              },
            ),
          ],
        ),
      ),
    );

    expect(bottomTapCount, 0);
    await tester.tap(find.byKey(overlayKey), warnIfMissed: false); // gesture detector is translucent; no hit is registered between it and the render view
    expect(bottomTapCount, 1);

    late final OverlayEntry newEntry1;
    addTearDown(() => newEntry1..remove()..dispose());
    overlayKey.currentState!.insert(
      newEntry1 = OverlayEntry(
        maintainState: true,
        opaque: true,
        builder: (BuildContext context) {
          return Container();
        },
      ),
    );
    await tester.pump();

    // Bottom is offstage and does not receive tap events.
    expect(find.byType(GestureDetector), findsNothing);
    expect(find.byType(GestureDetector, skipOffstage: false), findsOneWidget);
    await tester.tap(find.byKey(overlayKey), warnIfMissed: false); // gesture detector is translucent; no hit is registered between it and the render view
    expect(bottomTapCount, 1);

    int topTapCount = 0;
    late final OverlayEntry newEntry2;
    addTearDown(() => newEntry2..remove()..dispose());
    overlayKey.currentState!.insert(
      newEntry2 = OverlayEntry(
        maintainState: true,
        opaque: true,
        builder: (BuildContext context) {
          return GestureDetector(
            onTap: () {
              topTapCount++;
            },
          );
        },
      ),
    );
    await tester.pump();

    expect(topTapCount, 0);
    await tester.tap(find.byKey(overlayKey), warnIfMissed: false); // gesture detector is translucent; no hit is registered between it and the render view
    expect(topTapCount, 1);
    expect(bottomTapCount, 1);
  });

  testWidgetsWithLeakTracking('Semantics of entries below opaque entries are ignored', (WidgetTester tester) async {
    final SemanticsTester semantics = SemanticsTester(tester);
    final GlobalKey<OverlayState> overlayKey = GlobalKey<OverlayState>();
    late final OverlayEntry bottomEntry;
    addTearDown(() => bottomEntry..remove()..dispose());
    late final OverlayEntry topEntry;
    addTearDown(() => topEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          key: overlayKey,
          initialEntries: <OverlayEntry>[
            bottomEntry = OverlayEntry(
              maintainState: true,
              builder: (BuildContext context) {
                return const Text('bottom');
              },
            ),
            topEntry = OverlayEntry(
              maintainState: true,
              opaque: true,
              builder: (BuildContext context) {
                return const Text('top');
              },
            ),
          ],
        ),
      ),
    );
    expect(find.text('bottom'), findsNothing);
    expect(find.text('bottom', skipOffstage: false), findsOneWidget);
    expect(find.text('top'), findsOneWidget);
    expect(semantics, includesNodeWith(label: 'top'));
    expect(semantics, isNot(includesNodeWith(label: 'bottom')));

    semantics.dispose();
  });

  testWidgetsWithLeakTracking('Can use Positioned within OverlayEntry', (WidgetTester tester) async {
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) {
                return const Positioned(
                  left: 145,
                  top: 123,
                  child: Text('positioned child'),
                );
              },
            ),
          ],
        ),
      ),
    );

    expect(tester.getTopLeft(find.text('positioned child')), const Offset(145, 123));
  });

  testWidgets('Overlay can set and update clipBehavior', (WidgetTester tester) async {
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: <OverlayEntry>[
            OverlayEntry(
              builder: (BuildContext context) => Positioned(left: 2000, right: 2500, child: Container()),
            ),
          ],
        ),
      ),
    );

    // By default, clipBehavior should be Clip.hardEdge
    final RenderObject renderObject = tester.renderObject(find.byType(Overlay));
    // ignore: avoid_dynamic_calls
    expect((renderObject as dynamic).clipBehavior, equals(Clip.hardEdge));

    for (final Clip clip in Clip.values) {
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: <OverlayEntry>[
              OverlayEntry(
                builder: (BuildContext context) => Container(),
              ),
            ],
            clipBehavior: clip,
          ),
        ),
      );

      // ignore: avoid_dynamic_calls
      expect((renderObject as dynamic).clipBehavior, clip);
      bool visited = false;
      renderObject.visitChildren((RenderObject child) {
        visited = true;
        switch (clip) {
          case Clip.none:
            expect(renderObject.describeApproximatePaintClip(child), null);
          case Clip.hardEdge:
          case Clip.antiAlias:
          case Clip.antiAliasWithSaveLayer:
            expect(
              renderObject.describeApproximatePaintClip(child),
              const Rect.fromLTRB(0, 0, 800, 600),
            );
        }
      });
      expect(visited, true);
    }
  });

  testWidgetsWithLeakTracking('Overlay always applies clip', (WidgetTester tester) async {
    late final OverlayEntry baseEntry;
    addTearDown(() => baseEntry..remove()..dispose());

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(
          initialEntries: <OverlayEntry>[
            baseEntry = OverlayEntry(
              builder: (BuildContext context) => Positioned(left: 10, right: 10, child: Container()),
            ),
          ],
        ),
      ),
    );
    final RenderObject renderObject = tester.renderObject(find.byType(Overlay));
    // ignore: avoid_dynamic_calls
    expect((renderObject as dynamic).paint, paints
      ..save()
      ..clipRect(rect: const Rect.fromLTWH(0.0, 0.0, 800.0, 600.0))
      ..restore(),
    );
  });

  testWidgetsWithLeakTracking('OverlayEntry throws if inserted to an invalid Overlay', (WidgetTester tester) async {
    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: Overlay(),
      ),
    );
    final OverlayState overlay = tester.state(find.byType(Overlay));
    final OverlayEntry entry = OverlayEntry(builder: (BuildContext context) => const SizedBox());
    addTearDown(() => entry..remove()..dispose());
    expect(
      () => overlay.insert(entry),
      returnsNormally,
    );

    // Throws when inserted to the same Overlay.
    expect(
      () => overlay.insert(entry),
      throwsA(isA<FlutterError>().having(
        (FlutterError error) => error.toString(),
        'toString()',
        allOf(
          contains('The specified entry is already present in the target Overlay.'),
          contains('The OverlayEntry was'),
          contains('The Overlay the OverlayEntry was trying to insert to was'),
        ),
      )),
    );

    await tester.pumpWidget(
      const Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(child: Overlay()),
      ),
    );

    // Throws if inserted to an already disposed Overlay.
    expect(
      () => overlay.insert(entry),
      throwsA(isA<FlutterError>().having(
        (FlutterError error) => error.toString(),
        'toString()',
        allOf(
          contains('Attempted to insert an OverlayEntry to an already disposed Overlay.'),
          contains('The OverlayEntry was'),
          contains('The Overlay the OverlayEntry was trying to insert to was'),
        ),
      )),
    );

    final OverlayState newOverlay = tester.state(find.byType(Overlay));
    // Throws when inserted to a different Overlay without calling remove.
    expect(
      () => newOverlay.insert(entry),
      throwsA(isA<FlutterError>().having(
        (FlutterError error) => error.toString(),
        'toString()',
        allOf(
          contains('The specified entry is already present in a different Overlay.'),
          contains('The OverlayEntry was'),
          contains('The Overlay the OverlayEntry was trying to insert to was'),
          contains("The OverlayEntry's current Overlay was"),
        ),
      )),
    );
  });

  group('OverlayEntry listenable', () {
    final GlobalKey overlayKey = GlobalKey();
    final Widget emptyOverlay = Directionality(
      textDirection: TextDirection.ltr,
      child: Overlay(key: overlayKey),
    );

    testWidgetsWithLeakTracking('mounted state can be listened', (WidgetTester tester) async {
      await tester.pumpWidget(emptyOverlay);
      final OverlayState overlay = overlayKey.currentState! as OverlayState;
      final List<bool> mountedLog = <bool>[];
      final OverlayEntry entry = OverlayEntry(
        builder: (BuildContext context) => Container(),
      );
      addTearDown(entry.dispose);

      entry.addListener(() {
        mountedLog.add(entry.mounted);
      });

      overlay.insert(entry);
      expect(mountedLog, isEmpty);

      // Pump a frame. The Overlay entry will be mounted.
      await tester.pump();
      expect(mountedLog, <bool>[true]);

      entry.remove();
      expect(mountedLog, <bool>[true]);
      await tester.pump();
      expect(mountedLog, <bool>[true, false]);

      // Insert & remove again.
      overlay.insert(entry);
      await tester.pump();
      entry.remove();
      await tester.pump();

      expect(mountedLog, <bool>[true, false, true, false]);
    });

    testWidgetsWithLeakTracking('throw if disposed before removal', (WidgetTester tester) async {
      await tester.pumpWidget(emptyOverlay);
      final OverlayState overlay = overlayKey.currentState! as OverlayState;
      final OverlayEntry entry = OverlayEntry(
        builder: (BuildContext context) => Container(),
      );
      addTearDown(() => entry..remove()..dispose());

      overlay.insert(entry);
      Object? error;
      try {
        entry.dispose();
      } catch (e) {
        error = e;
      }

      expect(error, isAssertionError);
    });

    test('dispose works', () {
      final OverlayEntry entry = OverlayEntry(
        builder: (BuildContext context) => Container(),
      );

      entry.dispose();

      Object? error;
      try {
        entry.addListener(() {  });
      } catch (e) {
        error = e;
      }
      expect(error, isAssertionError);
    });

    testWidgetsWithLeakTracking('delayed dispose', (WidgetTester tester) async {
      await tester.pumpWidget(emptyOverlay);
      final OverlayState overlay = overlayKey.currentState! as OverlayState;
      final List<bool> mountedLog = <bool>[];
      final OverlayEntry entry = OverlayEntry(
        builder: (BuildContext context) => Container(),
      );
      entry.addListener(() {
        mountedLog.add(entry.mounted);
      });

      overlay.insert(entry);
      await tester.pump();
      expect(mountedLog, <bool>[true]);

      entry.remove();
      // Call dispose on the entry. The listeners should be notified for one
      // last time after this.
      entry.dispose();
      expect(mountedLog, <bool>[true]);
      await tester.pump();
      expect(mountedLog, <bool>[true, false]);
      expect(tester.takeException(), isNull);

      // The entry is no longer usable.
      Object? error;
      try {
        entry.addListener(() {  });
      } catch (e) {
        error = e;
      }
      expect(error, isAssertionError);
    });
  });

  group('LookupBoundary', () {
    testWidgetsWithLeakTracking('hides Overlay from Overlay.maybeOf', (WidgetTester tester) async {
      OverlayState? overlay;
      late final OverlayEntry baseEntry;
      addTearDown(() => baseEntry..remove()..dispose());

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: <OverlayEntry>[
              baseEntry = OverlayEntry(
                builder: (BuildContext context) {
                  return LookupBoundary(
                    child: Builder(
                      builder: (BuildContext context) {
                        overlay = Overlay.maybeOf(context);
                        return Container();
                      },
                    ),
                  );
                },
              ),
            ],
          ),
        ),
      );

      expect(overlay, isNull);
    });

    testWidgetsWithLeakTracking('hides Overlay from Overlay.of', (WidgetTester tester) async {
      late final OverlayEntry baseEntry;
      addTearDown(() => baseEntry..remove()..dispose());

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: <OverlayEntry>[
              baseEntry = OverlayEntry(
                builder: (BuildContext context) {
                  return LookupBoundary(
                    child: Builder(
                      builder: (BuildContext context) {
                        Overlay.of(context);
                        return Container();
                      },
                    ),
                  );
                },
              ),
            ],
          ),
        ),
      );
      final Object? exception = tester.takeException();
      expect(exception, isFlutterError);
      final FlutterError error = exception! as FlutterError;

      expect(
        error.toStringDeep(),
        'FlutterError\n'
        '   No Overlay widget found within the closest LookupBoundary.\n'
        '   There is an ancestor Overlay widget, but it is hidden by a\n'
        '   LookupBoundary.\n'
        '   Some widgets require an Overlay widget ancestor for correct\n'
        '   operation.\n'
        '   The most common way to add an Overlay to an application is to\n'
        '   include a MaterialApp, CupertinoApp or Navigator widget in the\n'
        '   runApp() call.\n'
        '   The context from which that widget was searching for an overlay\n'
        '   was:\n'
        '     Builder\n'
      );
    });

    testWidgetsWithLeakTracking('hides Overlay from debugCheckHasOverlay', (WidgetTester tester) async {
      late final OverlayEntry baseEntry;
      addTearDown(() => baseEntry..remove()..dispose());

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Overlay(
            initialEntries: <OverlayEntry>[
              baseEntry = OverlayEntry(
                builder: (BuildContext context) {
                  return LookupBoundary(
                    child: Builder(
                      builder: (BuildContext context) {
                        debugCheckHasOverlay(context);
                        return Container();
                      },
                    ),
                  );
                },
              ),
            ],
          ),
        ),
      );
      final Object? exception = tester.takeException();
      expect(exception, isFlutterError);
      final FlutterError error = exception! as FlutterError;

      expect(
        error.toStringDeep(), startsWith(
          'FlutterError\n'
          '   No Overlay widget found within the closest LookupBoundary.\n'
          '   There is an ancestor Overlay widget, but it is hidden by a\n'
          '   LookupBoundary.\n'
          '   Builder widgets require an Overlay widget ancestor within the\n'
          '   closest LookupBoundary.\n'
          '   An overlay lets widgets float on top of other widget children.\n'
          '   To introduce an Overlay widget, you can either directly include\n'
          '   one, or use a widget that contains an Overlay itself, such as a\n'
          '   Navigator, WidgetApp, MaterialApp, or CupertinoApp.\n'
          '   The specific widget that could not find a Overlay ancestor was:\n'
          '     Builder\n'
          '   The ancestors of this widget were:\n'
          '     LookupBoundary\n'
        ),
      );
    });
  });
}

class StatefulTestWidget extends StatefulWidget {
  const StatefulTestWidget({super.key});

  @override
  State<StatefulTestWidget> createState() => StatefulTestState();
}

class StatefulTestState extends State<StatefulTestWidget> {
  int rebuildCount = 0;

  @override
  Widget build(BuildContext context) {
    rebuildCount += 1;
    return Container();
  }
}
