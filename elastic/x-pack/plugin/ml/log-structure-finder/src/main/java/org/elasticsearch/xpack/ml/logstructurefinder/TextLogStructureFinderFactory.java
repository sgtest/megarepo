/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.logstructurefinder;

import java.util.List;
import java.util.regex.Pattern;

public class TextLogStructureFinderFactory implements LogStructureFinderFactory {

    // This works because, by default, dot doesn't match newlines
    private static final Pattern TWO_NON_BLANK_LINES_PATTERN = Pattern.compile(".\n+.");

    /**
     * This format matches if the sample contains at least one newline and at least two
     * non-blank lines.
     */
    @Override
    public boolean canCreateFromSample(List<String> explanation, String sample) {
        if (sample.indexOf('\n') < 0) {
            explanation.add("Not text because sample contains no newlines");
            return false;
        }
        if (TWO_NON_BLANK_LINES_PATTERN.matcher(sample).find() == false) {
            explanation.add("Not text because sample contains fewer than two non-blank lines");
            return false;
        }

        explanation.add("Deciding sample is text");
        return true;
    }

    @Override
    public LogStructureFinder createFromSample(List<String> explanation, String sample, String charsetName, Boolean hasByteOrderMarker) {
        return TextLogStructureFinder.makeTextLogStructureFinder(explanation, sample, charsetName, hasByteOrderMarker);
    }
}
