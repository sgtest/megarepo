// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/material.dart';

import 'use_cases.dart';

class CheckBoxListTile extends UseCase {

  @override
  String get name => 'CheckBoxListTile';

  @override
  String get route => '/check-box-list-tile';

  @override
  Widget build(BuildContext context) => _MainWidget();
}

class _MainWidget extends StatefulWidget {
  @override
  State<_MainWidget> createState() => _MainWidgetState();
}

class _MainWidgetState extends State<_MainWidget> {
  bool _checked = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('CheckBoxListTile')),
      body: Center(
        child: CheckboxListTile(
          value: _checked,
          onChanged: (bool? value) {
            setState(() {
              _checked = value!;
            });
          },
          title: const Text('a check box list title'),
        ),
      ),
    );
  }
}
