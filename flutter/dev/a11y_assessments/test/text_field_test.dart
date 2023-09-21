// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:a11y_assessments/use_cases/text_field.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'test_utils.dart';

void main() {
  testWidgets('text field can run', (WidgetTester tester) async {
    await pumpsUseCase(tester, TextFieldUseCase());
    expect(find.byType(TextField), findsExactly(2));

    // Test the enabled text field
    {
      final Finder finder = find.byKey(const Key('enabled text field'));
      await tester.tap(finder);
      await tester.pumpAndSettle();
      await tester.enterText(finder, 'abc');
      await tester.pumpAndSettle();
      expect(find.text('abc'), findsOneWidget);
    }

    // Test the disabled text field
    {
      final Finder finder = find.byKey(const Key('disabled text field'));
      final TextField textField = tester.widget<TextField>(finder);
      expect(textField.enabled, isFalse);
    }
  });
}
