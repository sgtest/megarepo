/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.gradle.internal.release;

import com.google.common.annotations.VisibleForTesting;

import java.io.File;
import java.io.FileWriter;
import java.io.IOException;
import java.nio.file.Files;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;

import static java.util.Comparator.comparing;
import static java.util.stream.Collectors.groupingBy;
import static java.util.stream.Collectors.toList;

/**
 * Generates the release notes i.e. list of changes that have gone into this release. They are grouped by the
 * type of change, then by team area.
 */
public class ReleaseNotesGenerator {
    /**
     * These mappings translate change types into the headings as they should appears in the release notes.
     */
    private static final Map<String, String> TYPE_LABELS = new HashMap<>();

    static {
        TYPE_LABELS.put("breaking", "Breaking changes");
        TYPE_LABELS.put("breaking-java", "Breaking Java changes");
        TYPE_LABELS.put("bug", "Bug fixes");
        TYPE_LABELS.put("deprecation", "Deprecations");
        TYPE_LABELS.put("enhancement", "Enhancements");
        TYPE_LABELS.put("feature", "New features");
        TYPE_LABELS.put("new-aggregation", "New aggregation");
        TYPE_LABELS.put("regression", "Regressions");
        TYPE_LABELS.put("upgrade", "Upgrades");
    }

    static void update(File templateFile, File outputFile, Map<QualifiedVersion, Set<ChangelogEntry>> changelogs) throws IOException {
        final String templateString = Files.readString(templateFile.toPath());

        try (FileWriter output = new FileWriter(outputFile)) {
            output.write(generateFile(templateString, changelogs));
        }
    }

    @VisibleForTesting
    static String generateFile(String template, Map<QualifiedVersion, Set<ChangelogEntry>> changelogs) throws IOException {
        final var changelogsByVersionByTypeByArea = buildChangelogBreakdown(changelogs);

        final Map<String, Object> bindings = new HashMap<>();
        bindings.put("changelogsByVersionByTypeByArea", changelogsByVersionByTypeByArea);
        bindings.put("TYPE_LABELS", TYPE_LABELS);

        return TemplateUtils.render(template, bindings);
    }

    private static Map<QualifiedVersion, Map<String, Map<String, List<ChangelogEntry>>>> buildChangelogBreakdown(
        Map<QualifiedVersion, Set<ChangelogEntry>> changelogsByVersion
    ) {
        Map<QualifiedVersion, Map<String, Map<String, List<ChangelogEntry>>>> changelogsByVersionByTypeByArea = new TreeMap<>(
            Comparator.reverseOrder()
        );

        changelogsByVersion.forEach((version, changelogs) -> {
            Map<String, Map<String, List<ChangelogEntry>>> changelogsByTypeByArea = changelogs.stream()
                .collect(
                    groupingBy(
                        // Entries with breaking info are always put in the breaking section
                        entry -> entry.getBreaking() == null ? entry.getType() : "breaking",
                        TreeMap::new,
                        // Group changelogs for each type by their team area
                        groupingBy(
                            // `security` and `known-issue` areas don't need to supply an area
                            entry -> entry.getType().equals("known-issue") || entry.getType().equals("security")
                                ? "_all_"
                                : entry.getArea(),
                            TreeMap::new,
                            toList()
                        )
                    )
                );

            changelogsByVersionByTypeByArea.put(version, changelogsByTypeByArea);
        });

        // Sort per-area changelogs by their summary text. Assumes that the underlying list is sortable
        changelogsByVersionByTypeByArea.forEach(
            (_version, byVersion) -> byVersion.forEach(
                (_type, byTeam) -> byTeam.forEach(
                    (_team, changelogsForTeam) -> changelogsForTeam.sort(comparing(ChangelogEntry::getSummary))
                )
            )
        );

        return changelogsByVersionByTypeByArea;
    }
}
