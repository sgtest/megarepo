// Copyright 2014 The Flutter Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// To run this, from the root of the Flutter repository:
//   bin/cache/dart-sdk/bin/dart --enable-asserts dev/bots/check_code_sample_links.dart

import 'dart:io';

import 'package:args/args.dart';
import 'package:file/file.dart';
import 'package:file/local.dart';
import 'package:path/path.dart' as path;

import 'utils.dart';

final String _scriptLocation = path.fromUri(Platform.script);
final String _flutterRoot = path.dirname(path.dirname(path.dirname(_scriptLocation)));
final String _exampleDirectoryPath = path.join(_flutterRoot, 'examples', 'api');
final String _packageDirectoryPath = path.join(_flutterRoot, 'packages');
final String _dartUIDirectoryPath = path.join(_flutterRoot, 'bin', 'cache', 'pkg', 'sky_engine', 'lib');

final List<String> _knownUnlinkedExamples = <String>[
  // These are template files that aren't expected to be linked.
  'examples/api/lib/sample_templates/cupertino.0.dart',
  'examples/api/lib/sample_templates/widgets.0.dart',
  'examples/api/lib/sample_templates/material.0.dart',
];

void main(List<String> args) {
  final ArgParser argParser = ArgParser();
  argParser.addFlag(
    'help',
    negatable: false,
    help: 'Print help for this command.',
  );
  argParser.addOption(
    'examples',
    valueHelp: 'path',
    defaultsTo: _exampleDirectoryPath,
    help: 'A location where the API doc examples are found.',
  );
  argParser.addOption(
    'packages',
    valueHelp: 'path',
    defaultsTo: _packageDirectoryPath,
    help: 'A location where the source code that should link the API doc examples is found.',
  );
  argParser.addOption(
    'dart-ui',
    valueHelp: 'path',
    defaultsTo: _dartUIDirectoryPath,
    help: 'A location where the source code that should link the API doc examples is found.',
  );
  argParser.addOption(
    'flutter-root',
    valueHelp: 'path',
    defaultsTo: _flutterRoot,
    help: 'The path to the root of the Flutter repo.',
  );
  final ArgResults parsedArgs;

  void usage() {
    print('dart --enable-asserts ${path.basename(_scriptLocation)} [options]');
    print(argParser.usage);
  }

  try {
    parsedArgs = argParser.parse(args);
  } on FormatException catch (e) {
    print(e.message);
    usage();
    exit(1);
  }

  if (parsedArgs['help'] as bool) {
    usage();
    exit(0);
  }

  const FileSystem filesystem = LocalFileSystem();
  final Directory examples = filesystem.directory(parsedArgs['examples']! as String);
  final Directory packages = filesystem.directory(parsedArgs['packages']! as String);
  final Directory dartUIPath = filesystem.directory(parsedArgs['dart-ui']! as String);
  final Directory flutterRoot = filesystem.directory(parsedArgs['flutter-root']! as String);

  final SampleChecker checker = SampleChecker(
    examples: examples,
    packages: packages,
    dartUIPath: dartUIPath,
    flutterRoot: flutterRoot,
  );

  if (!checker.checkCodeSamples()) {
    reportErrorsAndExit('Some errors were found in the API docs code samples.');
  }
  reportSuccessAndExit('All examples are linked and have tests.');
}

class SampleChecker {
  SampleChecker({
    required this.examples,
    required this.packages,
    required this.dartUIPath,
    required this.flutterRoot,
    this.filesystem = const LocalFileSystem(),
  });

  final Directory examples;
  final Directory packages;
  final Directory dartUIPath;
  final Directory flutterRoot;
  final FileSystem filesystem;

  bool checkCodeSamples() {
    filesystem.currentDirectory = flutterRoot;

    // Get a list of all the filenames in the source directory that end in "[0-9]+.dart".
    final List<File> exampleFilenames = getExampleFilenames(examples);

    // Get a list of all the example link paths that appear in the source files.
    final Set<String> exampleLinks = getExampleLinks(packages);

    // Also add in any that might be found in the dart:ui directory.
    exampleLinks.addAll(getExampleLinks(dartUIPath));

    // Get a list of the filenames that were not found in the source files.
    final List<String> missingFilenames = checkForMissingLinks(exampleFilenames, exampleLinks);

    // Get a list of any tests that are missing, as well as any that used to be
    // missing, but have been implemented.
    final (List<File> missingTests, List<File> noLongerMissing) = checkForMissingTests(exampleFilenames);

    // Remove any that we know are exceptions (examples that aren't expected to be
    // linked into any source files). These are typically template files used to
    // generate new examples.
    missingFilenames.removeWhere((String file) => _knownUnlinkedExamples.contains(file));

    if (missingFilenames.isEmpty && missingTests.isEmpty && noLongerMissing.isEmpty) {
      return true;
    }

    if (noLongerMissing.isNotEmpty) {
      final StringBuffer buffer = StringBuffer('The following tests have been implemented! Huzzah!:\n');
      for (final File name in noLongerMissing) {
        buffer.writeln('  ${getRelativePath(name)}');
      }
      buffer.writeln('However, they now need to be removed from the _knownMissingTests');
      buffer.write('list in the script $_scriptLocation.');
      foundError(buffer.toString().split('\n'));
    }

    if (missingTests.isNotEmpty) {
      final StringBuffer buffer = StringBuffer('The following example test files are missing:\n');
      for (final File name in missingTests) {
        buffer.writeln('  ${getRelativePath(name)}');
      }
      foundError(buffer.toString().trimRight().split('\n'));
    }

    if (missingFilenames.isNotEmpty) {
      final StringBuffer buffer =
          StringBuffer('The following examples are not linked from any source file API doc comments:\n');
      for (final String name in missingFilenames) {
        buffer.writeln('  $name');
      }
      buffer.write('Either link them to a source file API doc comment, or remove them.');
      foundError(buffer.toString().split('\n'));
    }
    return false;
  }

  String getRelativePath(File file, [Directory? root]) {
    root ??= flutterRoot;
    return path.relative(file.absolute.path, from: root.absolute.path);
  }

  List<File> getFiles(Directory directory, [Pattern? filenamePattern]) {
    final List<File> filenames = directory
        .listSync(recursive: true)
        .map((FileSystemEntity entity) {
          if (entity is File) {
            return entity;
          } else {
            return null;
          }
        })
        .where((File? filename) =>
            filename != null && (filenamePattern == null || filename.absolute.path.contains(filenamePattern)))
        .map<File>((File? s) => s!)
        .toList();
    return filenames;
  }

  List<File> getExampleFilenames(Directory directory) {
    return getFiles(
      directory.childDirectory('lib'),
      RegExp(r'\d+\.dart$'),
    );
  }

  Set<String> getExampleLinks(Directory searchDirectory) {
    final List<File> files = getFiles(searchDirectory, RegExp(r'\.dart$'));
    final Set<String> searchStrings = <String>{};
    final RegExp exampleRe = RegExp(r'\*\* See code in (?<path>.*) \*\*');
    for (final File file in files) {
      final String contents = file.readAsStringSync();
      searchStrings.addAll(
        contents.split('\n').where((String s) => s.contains(exampleRe)).map<String>(
          (String e) {
            return exampleRe.firstMatch(e)!.namedGroup('path')!;
          },
        ),
      );
    }
    return searchStrings;
  }

  List<String> checkForMissingLinks(List<File> exampleFilenames, Set<String> searchStrings) {
    final List<String> missingFilenames = <String>[];
    for (final File example in exampleFilenames) {
      final String relativePath = getRelativePath(example);
      if (!searchStrings.contains(relativePath)) {
        missingFilenames.add(relativePath);
      }
    }
    return missingFilenames;
  }

  String getTestNameForExample(File example, Directory examples) {
    final String testPath = path.dirname(
      path.join(
        examples.absolute.path,
        'test',
        getRelativePath(example, examples.childDirectory('lib')),
      ),
    );
    return '${path.join(testPath, path.basenameWithoutExtension(example.path))}_test.dart';
  }

  (List<File>, List<File>) checkForMissingTests(List<File> exampleFilenames) {
    final List<File> missingTests = <File>[];
    final List<File> noLongerMissingTests = <File>[];
    for (final File example in exampleFilenames) {
      final File testFile = filesystem.file(getTestNameForExample(example, examples));
      final String name = path.relative(testFile.absolute.path, from: flutterRoot.absolute.path);
      if (!testFile.existsSync()) {
        missingTests.add(testFile);
      } else if (_knownMissingTests.contains(name.replaceAll(r'\', '/'))) {
        noLongerMissingTests.add(testFile);
      }
    }
    // Skip any that we know are missing.
    missingTests.removeWhere(
      (File test) {
        final String name = path.relative(test.absolute.path, from: flutterRoot.absolute.path).replaceAll(r'\', '/');
        return _knownMissingTests.contains(name);
      },
    );
    return (missingTests, noLongerMissingTests);
  }
}

// These tests are known to be missing.  They should all eventually be
// implemented, but until they are we allow them, so that we can catch any new
// examples that are added without tests.
//
// TODO(gspencergoog): implement the missing tests.
// See https://github.com/flutter/flutter/issues/130459
final Set<String> _knownMissingTests = <String>{
  'examples/api/test/cupertino/text_field/cupertino_text_field.0_test.dart',
  'examples/api/test/material/bottom_app_bar/bottom_app_bar.2_test.dart',
  'examples/api/test/material/bottom_app_bar/bottom_app_bar.1_test.dart',
  'examples/api/test/material/theme/theme_extension.1_test.dart',
  'examples/api/test/material/elevated_button/elevated_button.0_test.dart',
  'examples/api/test/material/material_state/material_state_border_side.0_test.dart',
  'examples/api/test/material/material_state/material_state_mouse_cursor.0_test.dart',
  'examples/api/test/material/material_state/material_state_outlined_border.0_test.dart',
  'examples/api/test/material/material_state/material_state_property.0_test.dart',
  'examples/api/test/material/selectable_region/selectable_region.0_test.dart',
  'examples/api/test/material/text_field/text_field.2_test.dart',
  'examples/api/test/material/text_field/text_field.1_test.dart',
  'examples/api/test/material/button_style/button_style.0_test.dart',
  'examples/api/test/material/range_slider/range_slider.0_test.dart',
  'examples/api/test/material/card/card.2_test.dart',
  'examples/api/test/material/card/card.0_test.dart',
  'examples/api/test/material/selection_container/selection_container_disabled.0_test.dart',
  'examples/api/test/material/selection_container/selection_container.0_test.dart',
  'examples/api/test/material/color_scheme/dynamic_content_color.0_test.dart',
  'examples/api/test/material/platform_menu_bar/platform_menu_bar.0_test.dart',
  'examples/api/test/material/menu_anchor/menu_anchor.2_test.dart',
  'examples/api/test/material/stepper/stepper.controls_builder.0_test.dart',
  'examples/api/test/material/stepper/stepper.0_test.dart',
  'examples/api/test/material/flexible_space_bar/flexible_space_bar.0_test.dart',
  'examples/api/test/material/data_table/data_table.1_test.dart',
  'examples/api/test/material/data_table/data_table.0_test.dart',
  'examples/api/test/material/floating_action_button_location/standard_fab_location.0_test.dart',
  'examples/api/test/material/chip/deletable_chip_attributes.on_deleted.0_test.dart',
  'examples/api/test/material/snack_bar/snack_bar.0_test.dart',
  'examples/api/test/material/snack_bar/snack_bar.2_test.dart',
  'examples/api/test/material/snack_bar/snack_bar.1_test.dart',
  'examples/api/test/material/bottom_navigation_bar/bottom_navigation_bar.0_test.dart',
  'examples/api/test/material/bottom_navigation_bar/bottom_navigation_bar.1_test.dart',
  'examples/api/test/material/outlined_button/outlined_button.0_test.dart',
  'examples/api/test/material/icon_button/icon_button.2_test.dart',
  'examples/api/test/material/icon_button/icon_button.3_test.dart',
  'examples/api/test/material/icon_button/icon_button.0_test.dart',
  'examples/api/test/material/icon_button/icon_button.1_test.dart',
  'examples/api/test/material/expansion_panel/expansion_panel_list.0_test.dart',
  'examples/api/test/material/expansion_panel/expansion_panel_list.expansion_panel_list_radio.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.1_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.prefix_icon_constraints.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.material_state.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.2_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.label.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.suffix_icon_constraints.0_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.3_test.dart',
  'examples/api/test/material/input_decorator/input_decoration.material_state.1_test.dart',
  'examples/api/test/material/filled_button/filled_button.0_test.dart',
  'examples/api/test/material/text_form_field/text_form_field.1_test.dart',
  'examples/api/test/material/scrollbar/scrollbar.1_test.dart',
  'examples/api/test/material/scrollbar/scrollbar.0_test.dart',
  'examples/api/test/material/dropdown_menu/dropdown_menu.1_test.dart',
  'examples/api/test/material/dropdown_menu/dropdown_menu.0_test.dart',
  'examples/api/test/material/radio/radio.toggleable.0_test.dart',
  'examples/api/test/material/radio/radio.0_test.dart',
  'examples/api/test/material/search_anchor/search_anchor.0_test.dart',
  'examples/api/test/material/search_anchor/search_anchor.1_test.dart',
  'examples/api/test/material/search_anchor/search_anchor.2_test.dart',
  'examples/api/test/material/about/about_list_tile.0_test.dart',
  'examples/api/test/material/tab_controller/tab_controller.1_test.dart',
  'examples/api/test/material/selection_area/selection_area.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.end_drawer.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.drawer.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.1_test.dart',
  'examples/api/test/material/scaffold/scaffold.of.0_test.dart',
  'examples/api/test/material/scaffold/scaffold_messenger.of.0_test.dart',
  'examples/api/test/material/scaffold/scaffold_messenger.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.0_test.dart',
  'examples/api/test/material/scaffold/scaffold_state.show_bottom_sheet.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.2_test.dart',
  'examples/api/test/material/scaffold/scaffold_messenger_state.show_material_banner.0_test.dart',
  'examples/api/test/material/scaffold/scaffold.of.1_test.dart',
  'examples/api/test/material/scaffold/scaffold_messenger.of.1_test.dart',
  'examples/api/test/material/scaffold/scaffold_messenger_state.show_snack_bar.0_test.dart',
  'examples/api/test/material/segmented_button/segmented_button.0_test.dart',
  'examples/api/test/material/app_bar/app_bar.2_test.dart',
  'examples/api/test/material/app_bar/sliver_app_bar.1_test.dart',
  'examples/api/test/material/app_bar/sliver_app_bar.2_test.dart',
  'examples/api/test/material/app_bar/sliver_app_bar.3_test.dart',
  'examples/api/test/material/app_bar/app_bar.1_test.dart',
  'examples/api/test/material/app_bar/sliver_app_bar.4_test.dart',
  'examples/api/test/material/app_bar/app_bar.3_test.dart',
  'examples/api/test/material/app_bar/app_bar.0_test.dart',
  'examples/api/test/material/ink_well/ink_well.0_test.dart',
  'examples/api/test/material/banner/material_banner.1_test.dart',
  'examples/api/test/material/banner/material_banner.0_test.dart',
  'examples/api/test/material/checkbox/checkbox.1_test.dart',
  'examples/api/test/material/checkbox/checkbox.0_test.dart',
  'examples/api/test/material/navigation_rail/navigation_rail.extended_animation.0_test.dart',
  'examples/api/test/material/text_button/text_button.0_test.dart',
  'examples/api/test/rendering/growth_direction/growth_direction.0_test.dart',
  'examples/api/test/rendering/sliver_grid/sliver_grid_delegate_with_fixed_cross_axis_count.0_test.dart',
  'examples/api/test/rendering/sliver_grid/sliver_grid_delegate_with_fixed_cross_axis_count.1_test.dart',
  'examples/api/test/rendering/scroll_direction/scroll_direction.0_test.dart',
  'examples/api/test/painting/axis_direction/axis_direction.0_test.dart',
  'examples/api/test/painting/linear_border/linear_border.0_test.dart',
  'examples/api/test/painting/gradient/linear_gradient.0_test.dart',
  'examples/api/test/painting/star_border/star_border.0_test.dart',
  'examples/api/test/painting/borders/border_side.stroke_align.0_test.dart',
  'examples/api/test/widgets/autocomplete/raw_autocomplete.focus_node.0_test.dart',
  'examples/api/test/widgets/autocomplete/raw_autocomplete.2_test.dart',
  'examples/api/test/widgets/autocomplete/raw_autocomplete.1_test.dart',
  'examples/api/test/widgets/autocomplete/raw_autocomplete.0_test.dart',
  'examples/api/test/widgets/navigator/navigator.restorable_push_and_remove_until.0_test.dart',
  'examples/api/test/widgets/navigator/navigator.0_test.dart',
  'examples/api/test/widgets/navigator/navigator.restorable_push.0_test.dart',
  'examples/api/test/widgets/navigator/navigator_state.restorable_push_replacement.0_test.dart',
  'examples/api/test/widgets/navigator/navigator_state.restorable_push_and_remove_until.0_test.dart',
  'examples/api/test/widgets/navigator/navigator.restorable_push_replacement.0_test.dart',
  'examples/api/test/widgets/navigator/restorable_route_future.0_test.dart',
  'examples/api/test/widgets/navigator/navigator_state.restorable_push.0_test.dart',
  'examples/api/test/widgets/focus_manager/focus_node.unfocus.0_test.dart',
  'examples/api/test/widgets/focus_manager/focus_node.0_test.dart',
  'examples/api/test/widgets/framework/build_owner.0_test.dart',
  'examples/api/test/widgets/framework/error_widget.0_test.dart',
  'examples/api/test/widgets/inherited_theme/inherited_theme.0_test.dart',
  'examples/api/test/widgets/sliver/decorated_sliver.0_test.dart',
  'examples/api/test/widgets/autofill/autofill_group.0_test.dart',
  'examples/api/test/widgets/drag_target/draggable.0_test.dart',
  'examples/api/test/widgets/shared_app_data/shared_app_data.1_test.dart',
  'examples/api/test/widgets/shared_app_data/shared_app_data.0_test.dart',
  'examples/api/test/widgets/form/form.0_test.dart',
  'examples/api/test/widgets/nested_scroll_view/nested_scroll_view_state.0_test.dart',
  'examples/api/test/widgets/nested_scroll_view/nested_scroll_view.2_test.dart',
  'examples/api/test/widgets/nested_scroll_view/nested_scroll_view.1_test.dart',
  'examples/api/test/widgets/nested_scroll_view/nested_scroll_view.0_test.dart',
  'examples/api/test/widgets/page_view/page_view.0_test.dart',
  'examples/api/test/widgets/scroll_position/scroll_metrics_notification.0_test.dart',
  'examples/api/test/widgets/media_query/media_query_data.system_gesture_insets.0_test.dart',
  'examples/api/test/widgets/async/stream_builder.0_test.dart',
  'examples/api/test/widgets/async/future_builder.0_test.dart',
  'examples/api/test/widgets/restoration_properties/restorable_value.0_test.dart',
  'examples/api/test/widgets/animated_size/animated_size.0_test.dart',
  'examples/api/test/widgets/table/table.0_test.dart',
  'examples/api/test/widgets/animated_switcher/animated_switcher.0_test.dart',
  'examples/api/test/widgets/transitions/relative_positioned_transition.0_test.dart',
  'examples/api/test/widgets/transitions/positioned_transition.0_test.dart',
  'examples/api/test/widgets/transitions/listenable_builder.3_test.dart',
  'examples/api/test/widgets/transitions/sliver_fade_transition.0_test.dart',
  'examples/api/test/widgets/transitions/align_transition.0_test.dart',
  'examples/api/test/widgets/transitions/fade_transition.0_test.dart',
  'examples/api/test/widgets/transitions/animated_builder.0_test.dart',
  'examples/api/test/widgets/transitions/rotation_transition.0_test.dart',
  'examples/api/test/widgets/transitions/animated_widget.0_test.dart',
  'examples/api/test/widgets/transitions/slide_transition.0_test.dart',
  'examples/api/test/widgets/transitions/listenable_builder.2_test.dart',
  'examples/api/test/widgets/transitions/scale_transition.0_test.dart',
  'examples/api/test/widgets/transitions/default_text_style_transition.0_test.dart',
  'examples/api/test/widgets/transitions/decorated_box_transition.0_test.dart',
  'examples/api/test/widgets/transitions/size_transition.0_test.dart',
  'examples/api/test/widgets/animated_list/animated_list.0_test.dart',
  'examples/api/test/widgets/focus_traversal/focus_traversal_group.0_test.dart',
  'examples/api/test/widgets/focus_traversal/ordered_traversal_policy.0_test.dart',
  'examples/api/test/widgets/image/image.error_builder.0_test.dart',
  'examples/api/test/widgets/image/image.frame_builder.0_test.dart',
  'examples/api/test/widgets/image/image.loading_builder.0_test.dart',
  'examples/api/test/widgets/shortcuts/logical_key_set.0_test.dart',
  'examples/api/test/widgets/shortcuts/shortcuts.0_test.dart',
  'examples/api/test/widgets/shortcuts/single_activator.single_activator.0_test.dart',
  'examples/api/test/widgets/shortcuts/shortcuts.1_test.dart',
  'examples/api/test/widgets/shortcuts/character_activator.0_test.dart',
  'examples/api/test/widgets/shortcuts/callback_shortcuts.0_test.dart',
  'examples/api/test/widgets/page_storage/page_storage.0_test.dart',
  'examples/api/test/widgets/scrollbar/raw_scrollbar.1_test.dart',
  'examples/api/test/widgets/scrollbar/raw_scrollbar.2_test.dart',
  'examples/api/test/widgets/scrollbar/raw_scrollbar.desktop.0_test.dart',
  'examples/api/test/widgets/scrollbar/raw_scrollbar.shape.0_test.dart',
  'examples/api/test/widgets/scrollbar/raw_scrollbar.0_test.dart',
  'examples/api/test/widgets/sliver_fill/sliver_fill_remaining.2_test.dart',
  'examples/api/test/widgets/sliver_fill/sliver_fill_remaining.1_test.dart',
  'examples/api/test/widgets/sliver_fill/sliver_fill_remaining.3_test.dart',
  'examples/api/test/widgets/sliver_fill/sliver_fill_remaining.0_test.dart',
  'examples/api/test/widgets/interactive_viewer/interactive_viewer.constrained.0_test.dart',
  'examples/api/test/widgets/interactive_viewer/interactive_viewer.transformation_controller.0_test.dart',
  'examples/api/test/widgets/interactive_viewer/interactive_viewer.0_test.dart',
  'examples/api/test/widgets/notification_listener/notification.0_test.dart',
  'examples/api/test/widgets/gesture_detector/gesture_detector.1_test.dart',
  'examples/api/test/widgets/gesture_detector/gesture_detector.0_test.dart',
  'examples/api/test/widgets/editable_text/text_editing_controller.0_test.dart',
  'examples/api/test/widgets/editable_text/editable_text.on_changed.0_test.dart',
  'examples/api/test/widgets/undo_history/undo_history_controller.0_test.dart',
  'examples/api/test/widgets/overscroll_indicator/glowing_overscroll_indicator.1_test.dart',
  'examples/api/test/widgets/overscroll_indicator/glowing_overscroll_indicator.0_test.dart',
  'examples/api/test/widgets/tween_animation_builder/tween_animation_builder.0_test.dart',
  'examples/api/test/widgets/single_child_scroll_view/single_child_scroll_view.1_test.dart',
  'examples/api/test/widgets/single_child_scroll_view/single_child_scroll_view.0_test.dart',
  'examples/api/test/widgets/overflow_bar/overflow_bar.0_test.dart',
  'examples/api/test/widgets/restoration/restoration_mixin.0_test.dart',
  'examples/api/test/widgets/actions/actions.0_test.dart',
  'examples/api/test/widgets/actions/action_listener.0_test.dart',
  'examples/api/test/widgets/actions/focusable_action_detector.0_test.dart',
  'examples/api/test/widgets/color_filter/color_filtered.0_test.dart',
  'examples/api/test/widgets/focus_scope/focus.2_test.dart',
  'examples/api/test/widgets/focus_scope/focus.0_test.dart',
  'examples/api/test/widgets/focus_scope/focus.1_test.dart',
  'examples/api/test/widgets/focus_scope/focus_scope.0_test.dart',
  'examples/api/test/widgets/implicit_animations/animated_fractionally_sized_box.0_test.dart',
  'examples/api/test/widgets/implicit_animations/animated_align.0_test.dart',
  'examples/api/test/widgets/implicit_animations/animated_positioned.0_test.dart',
  'examples/api/test/widgets/implicit_animations/animated_padding.0_test.dart',
  'examples/api/test/widgets/implicit_animations/sliver_animated_opacity.0_test.dart',
  'examples/api/test/widgets/implicit_animations/animated_container.0_test.dart',
  'examples/api/test/widgets/dismissible/dismissible.0_test.dart',
  'examples/api/test/widgets/scroll_view/custom_scroll_view.1_test.dart',
  'examples/api/test/widgets/preferred_size/preferred_size.0_test.dart',
  'examples/api/test/widgets/inherited_notifier/inherited_notifier.0_test.dart',
  'examples/api/test/animation/curves/curve2_d.0_test.dart',
  'examples/api/test/gestures/pointer_signal_resolver/pointer_signal_resolver.0_test.dart',
};
