// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/foundation.dart';

import 'system_channels.dart';

/// A data structure describing text processing actions.
@immutable
class ProcessTextAction {
  /// Creates text processing actions based on those returned by the engine.
  const ProcessTextAction(this.id, this.label);

  /// The action unique id.
  final String id;

  /// The action localized label.
  final String label;

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) {
      return true;
    }

    return other is ProcessTextAction &&
      other.id == id &&
      other.label == label;
  }

  @override
  int get hashCode => Object.hash(id, label);
}

/// Determines how to interact with the text processing feature.
abstract class ProcessTextService {
  /// Returns a [Future] that resolves to a [List] of [ProcessTextAction]s
  /// containing all text processing actions available.
  ///
  /// If there are no actions available, an empty list will be returned.
  Future<List<ProcessTextAction>> queryTextActions();

  /// Returns a [Future] that resolves to a [String] when the text action
  /// returns a transformed text or null when the text action did not return
  /// a transformed text.
  ///
  /// The `id` parameter is the text action unique identifier returned by
  /// [queryTextActions].
  ///
  /// The `text` parameter is the text to be processed.
  ///
  /// The `readOnly` parameter indicates that the transformed text, if it exists,
  /// will be used as read-only.
  Future<String?> processTextAction(String id, String text, bool readOnly);
}

/// The service used by default for the text processing feature.
///
/// Any widget may use this service to get a list of text processing actions
/// and send requests to activate these text actions.
///
/// This is currently only supported by Android.
///
/// See also:
///
///  * [ProcessTextService], the service that this implements.
class DefaultProcessTextService implements ProcessTextService {
  /// Creates the default service to interact with the platform text processing
  /// feature via communication over the text processing [MethodChannel].
  DefaultProcessTextService() {
    _processTextChannel = SystemChannels.processText;
  }

  /// The channel used to communicate with the engine side.
  late MethodChannel _processTextChannel;

  /// Set the [MethodChannel] used to communicate with the engine text processing
  /// feature.
  ///
  /// This is only meant for testing within the Flutter SDK.
  @visibleForTesting
  void setChannel(MethodChannel newChannel) {
    assert(() {
      _processTextChannel = newChannel;
      return true;
    }());
  }

  @override
  Future<List<ProcessTextAction>> queryTextActions() async {
    final List<ProcessTextAction> textActions = <ProcessTextAction>[];
    final Map<Object?, Object?>? rawResults;

    try {
      rawResults = await _processTextChannel.invokeMethod(
        'ProcessText.queryTextActions',
      ) as Map<Object?, Object?>;
    } catch (e) {
      return textActions;
    }

    for (final Object? id in rawResults.keys) {
      textActions.add(ProcessTextAction(id! as String, rawResults[id]! as String));
    }

    return textActions;
  }

  @override
  /// On Android, the readOnly parameter might be used by the targeted activity, see:
  /// https://developer.android.com/reference/android/content/Intent#EXTRA_PROCESS_TEXT_READONLY.
  Future<String?> processTextAction(String id, String text, bool readOnly) async {
    final String? processedText = await _processTextChannel.invokeMethod(
      'ProcessText.processTextAction',
      <dynamic>[id, text, readOnly],
    ) as String?;

    return processedText;
  }
}
