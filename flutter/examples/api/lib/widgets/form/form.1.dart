// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// This sample demonstrates showing a confirmation dialog when the user
/// attempts to navigate away from a page with unsaved [Form] data.

void main() => runApp(const FormApp());

class FormApp extends StatelessWidget {
  const FormApp({
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(
          title: const Text('Confirmation Dialog Example'),
        ),
        body: Center(
          child: _SaveableForm(),
        ),
      ),
    );
  }
}

class _SaveableForm extends StatefulWidget {
  @override
  State<_SaveableForm> createState() => _SaveableFormState();
}

class _SaveableFormState extends State<_SaveableForm> {
  final TextEditingController _controller = TextEditingController();
  String _savedValue = '';
  bool _isDirty = false;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onChanged);
  }

  @override
  void dispose() {
    _controller.removeListener(_onChanged);
    super.dispose();
  }

  void _onChanged() {
    final bool nextIsDirty = _savedValue != _controller.text;
    if (nextIsDirty == _isDirty) {
      return;
    }
    setState(() {
      _isDirty = nextIsDirty;
    });
  }

  Future<void> _showDialog() async {
    final bool? shouldDiscard = await showDialog<bool>(
      context: context,
      builder: (BuildContext context) {
        return AlertDialog(
          title: const Text('Are you sure?'),
          content: const Text('Any unsaved changes will be lost!'),
          actions: <Widget>[
            TextButton(
              child: const Text('Yes, discard my changes'),
              onPressed: () {
                Navigator.pop(context, true);
              },
            ),
            TextButton(
              child: const Text('No, continue editing'),
              onPressed: () {
                Navigator.pop(context, false);
              },
            ),
          ],
        );
      },
    );

    if (shouldDiscard ?? false) {
      // Since this is the root route, quit the app where possible by invoking
      // the SystemNavigator. If this wasn't the root route, then
      // Navigator.maybePop could be used instead.
      // See https://github.com/flutter/flutter/issues/11490
      SystemNavigator.pop();
    }
  }

  void _save(String? value) {
    setState(() {
      _savedValue = value ?? '';
    });
  }

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: <Widget>[
          const Text('If the field below is unsaved, a confirmation dialog will be shown on back.'),
          const SizedBox(height: 20.0),
          Form(
            canPop: !_isDirty,
            onPopInvoked: (bool didPop) {
              if (didPop) {
                return;
              }
              _showDialog();
            },
            autovalidateMode: AutovalidateMode.always,
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: <Widget>[
                TextFormField(
                  controller: _controller,
                  onFieldSubmitted: (String? value) {
                    _save(value);
                  },
                ),
                TextButton(
                  onPressed: () {
                    _save(_controller.text);
                  },
                  child: Row(
                    children: <Widget>[
                      const Text('Save'),
                      if (_controller.text.isNotEmpty)
                        Icon(
                          _isDirty ? Icons.warning : Icons.check,
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
          TextButton(
            onPressed: () {
              if (_isDirty) {
                _showDialog();
                return;
              }
              // Since this is the root route, quit the app where possible by
              // invoking the SystemNavigator. If this wasn't the root route,
              // then Navigator.maybePop could be used instead.
              // See https://github.com/flutter/flutter/issues/11490
              SystemNavigator.pop();
            },
            child: const Text('Go back'),
          ),
        ],
      ),
    );
  }
}
