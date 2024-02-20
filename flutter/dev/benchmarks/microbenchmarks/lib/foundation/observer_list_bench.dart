// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/animation.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

import '../common.dart';

const int _kNumIterationsList = 2 << 14;
const int _kNumIterationsHashed = 2 << 19;
const int _kNumWarmUp = 2 << 6;
const List<int> callbackCounts = <int>[1, 10, 100, 500];

class TestAnimationController extends AnimationController {
  TestAnimationController() : super(vsync: const TestVSync());

  @override
  void notifyListeners() => super.notifyListeners();
}

void main() {
  assert(false,
      "Don't run benchmarks in debug mode! Use 'flutter run --release'.");

  final BenchmarkResultPrinter printer = BenchmarkResultPrinter();

  void runNotifiyListenersLoopWithObserverList(
    int totalIterations, {
    bool failRemoval = false,
    bool addResult = true,
  }) {
    final String suffix = failRemoval ? 'removalFail' : 'removalSuccess';
    final String name = 'notifyListeners:ObserverList:$suffix';

    void miss() {}

    for (final int callbackCount in callbackCounts) {
      final int iterations = totalIterations ~/ callbackCount;

      final ObserverList<VoidCallback> observerList =
          ObserverList<VoidCallback>();
      for (int i = 0; i < callbackCount; ++i) {
        observerList.add(
          switch (failRemoval) {
            false => () {
                final VoidCallback first =
                    (observerList.iterator..moveNext()).current;

                observerList.remove(first);
                observerList.add(first);
              },
            true => () => observerList.remove(miss),
          },
        );
      }

      final Stopwatch watch = Stopwatch()..start();

      for (int i = 0; i < iterations; ++i) {
        final List<VoidCallback> list = observerList.toList(growable: false);
        for (final VoidCallback cb in list) {
          if (observerList.contains(cb)) {
            cb();
          }
        }
      }

      watch.stop();

      if (addResult) {
        printer.addResult(
          description: '$name ($callbackCount callbacks)',
          value: watch.elapsedMicroseconds / iterations,
          unit: 'µs per iteration',
          name: '$name$callbackCount',
        );
      }
    }
  }

  void runNotifiyListenersLoopWithHashedObserverList(
    int totalIterations, {
    bool addResult = true,
  }) {
    const String name = 'notifyListeners:HashedObserverList';

    for (final int callbackCount in callbackCounts) {
      final int iterations = totalIterations ~/ callbackCount;

      final HashedObserverList<VoidCallback> observerList =
          HashedObserverList<VoidCallback>();
      for (int i = 0; i < callbackCount; ++i) {
        observerList.add(() {
          final VoidCallback first =
              (observerList.iterator..moveNext()).current;

          observerList.remove(first);
          observerList.add(first);
        });
      }

      final Stopwatch watch = Stopwatch()..start();

      for (int i = 0; i < iterations; ++i) {
        final List<VoidCallback> list = observerList.toList(growable: false);
        for (final VoidCallback cb in list) {
          if (observerList.contains(cb)) {
            cb();
          }
        }
      }

      watch.stop();

      if (addResult) {
        printer.addResult(
          description: '$name ($callbackCount callbacks)',
          value: watch.elapsedMicroseconds / iterations,
          unit: 'µs per iteration',
          name: '$name$callbackCount',
        );
      }
    }
  }

  void runNotifiyListenersLoopWithAnimationController(
    int totalIterations, {
    bool addResult = true,
  }) {
    const String name = 'notifyListeners:AnimationController';

    for (final int callbackCount in callbackCounts) {
      final int iterations = totalIterations ~/ callbackCount;

      final TestAnimationController controller = TestAnimationController();
      for (int i = 0; i < callbackCount; ++i) {
        late final VoidCallback cb;
        cb = () {
          controller.removeListener(cb);
          controller.addListener(cb);
        };
        controller.addListener(cb);
      }

      final Stopwatch watch = Stopwatch()..start();

      for (int i = 0; i < iterations; ++i) {
        controller.notifyListeners();
      }

      watch.stop();

      if (addResult) {
        printer.addResult(
          description: '$name ($callbackCount callbacks)',
          value: watch.elapsedMicroseconds / iterations,
          unit: 'µs per iteration',
          name: '$name$callbackCount',
        );
      }
    }
  }

  runNotifiyListenersLoopWithObserverList(_kNumWarmUp, addResult: false);
  runNotifiyListenersLoopWithObserverList(_kNumIterationsList);

  runNotifiyListenersLoopWithObserverList(_kNumWarmUp,
      failRemoval: true, addResult: false);
  runNotifiyListenersLoopWithObserverList(_kNumIterationsList,
      failRemoval: true);

  runNotifiyListenersLoopWithHashedObserverList(_kNumWarmUp, addResult: false);
  runNotifiyListenersLoopWithHashedObserverList(_kNumIterationsHashed);

  runNotifiyListenersLoopWithAnimationController(_kNumWarmUp, addResult: false);
  runNotifiyListenersLoopWithAnimationController(_kNumIterationsHashed);

  printer.printToStdout();
}
