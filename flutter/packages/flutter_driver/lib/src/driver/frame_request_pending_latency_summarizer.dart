// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'percentile_utils.dart';
import 'timeline.dart';

/// Event name for frame request pending timeline events.
const String kFrameRequestPendingEvent = 'Frame Request Pending';

/// Summarizes [TimelineEvents]s corresponding to [kFrameRequestPendingEvent] events.
///
/// `FrameRequestPendingLatency` is the time between `Animator::RequestFrame`
/// and `Animator::BeginFrame` for each frame built by the Flutter engine.
class FrameRequestPendingLatencySummarizer {
  /// Creates a FrameRequestPendingLatencySummarizer given the timeline events.
  FrameRequestPendingLatencySummarizer(this.frameRequestPendingEvents);

  /// Timeline events with names in [kFrameRequestPendingTimelineEventNames].
  final List<TimelineEvent> frameRequestPendingEvents;

  /// Computes the average `FrameRequestPendingLatency` over the period of the timeline.
  double computeAverageFrameRequestPendingLatency() {
    final List<double> frameRequestPendingLatencies =
        _computeFrameRequestPendingLatencies();
    if (frameRequestPendingLatencies.isEmpty) {
      return 0;
    }

    final double total = frameRequestPendingLatencies.reduce((double a, double b) => a + b);
    return total / frameRequestPendingLatencies.length;
  }

  /// Computes the [percentile]-th percentile `FrameRequestPendingLatency` over the
  /// period of the timeline.
  double computePercentileFrameRequestPendingLatency(double percentile) {
    final List<double> frameRequestPendingLatencies =
        _computeFrameRequestPendingLatencies();
    if (frameRequestPendingLatencies.isEmpty) {
      return 0;
    }
    return findPercentile(frameRequestPendingLatencies, percentile);
  }

  List<double> _computeFrameRequestPendingLatencies() {
    final List<double> result = <double>[];
    final Map<String, int> starts = <String, int>{};
    for (int i = 0; i < frameRequestPendingEvents.length; i++) {
      final TimelineEvent event = frameRequestPendingEvents[i];
      if (event.phase == 'b') {
        final String? id = event.json['id'] as String?;
        if (id != null) {
          starts[id] = event.timestampMicros!;
        }
      } else if (event.phase == 'e') {
        final int? start = starts[event.json['id']];
        if (start != null) {
          result.add((event.timestampMicros! - start).toDouble());
        }
      }
    }
    return result;
  }
}
