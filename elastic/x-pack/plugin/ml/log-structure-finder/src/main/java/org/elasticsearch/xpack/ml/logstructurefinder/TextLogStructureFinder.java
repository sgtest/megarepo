/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.logstructurefinder;

import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.xpack.ml.logstructurefinder.TimestampFormatFinder.TimestampMatch;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.SortedMap;
import java.util.TreeMap;
import java.util.regex.Pattern;

public class TextLogStructureFinder implements LogStructureFinder {

    private final List<String> sampleMessages;
    private final LogStructure structure;

    static TextLogStructureFinder makeTextLogStructureFinder(List<String> explanation, String sample, String charsetName,
                                                             Boolean hasByteOrderMarker) {

        String[] sampleLines = sample.split("\n");
        Tuple<TimestampMatch, Set<String>> bestTimestamp = mostLikelyTimestamp(sampleLines);
        if (bestTimestamp == null) {
            // Is it appropriate to treat a file that is neither structured nor has
            // a regular pattern of timestamps as a log file?  Probably not...
            throw new IllegalArgumentException("Could not find a timestamp in the log sample provided");
        }

        explanation.add("Most likely timestamp format is [" + bestTimestamp.v1() + "]");

        List<String> sampleMessages = new ArrayList<>();
        StringBuilder preamble = new StringBuilder();
        int linesConsumed = 0;
        StringBuilder message = null;
        int linesInMessage = 0;
        String multiLineRegex = createMultiLineMessageStartRegex(bestTimestamp.v2(), bestTimestamp.v1().simplePattern.pattern());
        Pattern multiLinePattern = Pattern.compile(multiLineRegex);
        for (String sampleLine : sampleLines) {
            if (multiLinePattern.matcher(sampleLine).find()) {
                if (message != null) {
                    sampleMessages.add(message.toString());
                    linesConsumed += linesInMessage;
                }
                message = new StringBuilder(sampleLine);
                linesInMessage = 1;
            } else {
                // If message is null here then the sample probably began with the incomplete ending of a previous message
                if (message == null) {
                    // We count lines before the first message as consumed (just like we would
                    // for the CSV header or lines before the first XML document starts)
                    ++linesConsumed;
                } else {
                    message.append('\n').append(sampleLine);
                    ++linesInMessage;
                }
            }
            if (sampleMessages.size() < 2) {
                preamble.append(sampleLine).append('\n');
            }
        }
        // Don't add the last message, as it might be partial and mess up subsequent pattern finding

        LogStructure.Builder structureBuilder = new LogStructure.Builder(LogStructure.Format.SEMI_STRUCTURED_TEXT)
            .setCharset(charsetName)
            .setHasByteOrderMarker(hasByteOrderMarker)
            .setSampleStart(preamble.toString())
            .setNumLinesAnalyzed(linesConsumed)
            .setNumMessagesAnalyzed(sampleMessages.size())
            .setMultilineStartPattern(multiLineRegex);

        SortedMap<String, Object> mappings = new TreeMap<>();
        mappings.put("message", Collections.singletonMap(LogStructureUtils.MAPPING_TYPE_SETTING, "text"));
        mappings.put(LogStructureUtils.DEFAULT_TIMESTAMP_FIELD, Collections.singletonMap(LogStructureUtils.MAPPING_TYPE_SETTING, "date"));

        // We can't parse directly into @timestamp using Grok, so parse to some other time field, which the date filter will then remove
        String interimTimestampField;
        String grokPattern;
        GrokPatternCreator grokPatternCreator = new GrokPatternCreator(explanation, sampleMessages, mappings);
        Tuple<String, String> timestampFieldAndFullMatchGrokPattern = grokPatternCreator.findFullLineGrokPattern();
        if (timestampFieldAndFullMatchGrokPattern != null) {
            interimTimestampField = timestampFieldAndFullMatchGrokPattern.v1();
            grokPattern = timestampFieldAndFullMatchGrokPattern.v2();
        } else {
            interimTimestampField = "timestamp";
            grokPattern = grokPatternCreator.createGrokPatternFromExamples(bestTimestamp.v1().grokPatternName, interimTimestampField);
        }

        LogStructure structure = structureBuilder
            .setTimestampField(interimTimestampField)
            .setTimestampFormats(bestTimestamp.v1().dateFormats)
            .setNeedClientTimezone(bestTimestamp.v1().hasTimezoneDependentParsing())
            .setGrokPattern(grokPattern)
            .setMappings(mappings)
            .setExplanation(explanation)
            .build();

        return new TextLogStructureFinder(sampleMessages, structure);
    }

    private TextLogStructureFinder(List<String> sampleMessages, LogStructure structure) {
        this.sampleMessages = Collections.unmodifiableList(sampleMessages);
        this.structure = structure;
    }

    @Override
    public List<String> getSampleMessages() {
        return sampleMessages;
    }

    @Override
    public LogStructure getStructure() {
        return structure;
    }

    static Tuple<TimestampMatch, Set<String>> mostLikelyTimestamp(String[] sampleLines) {

        Map<TimestampMatch, Tuple<Double, Set<String>>> timestampMatches = new LinkedHashMap<>();

        int remainingLines = sampleLines.length;
        double differenceBetweenTwoHighestWeights = 0.0;
        for (String sampleLine : sampleLines) {
            TimestampMatch match = TimestampFormatFinder.findFirstMatch(sampleLine);
            if (match != null) {
                TimestampMatch pureMatch = new TimestampMatch(match.candidateIndex, "", match.dateFormats, match.simplePattern,
                    match.grokPatternName, "");
                timestampMatches.compute(pureMatch, (k, v) -> {
                    if (v == null) {
                        return new Tuple<>(weightForMatch(match.preface), new HashSet<>(Collections.singletonList(match.preface)));
                    } else {
                        v.v2().add(match.preface);
                        return new Tuple<>(v.v1() + weightForMatch(match.preface), v.v2());
                    }
                });
                differenceBetweenTwoHighestWeights = findDifferenceBetweenTwoHighestWeights(timestampMatches.values());
            }
            // The highest possible weight is 1, so if the difference between the two highest weights
            // is less than the number of lines remaining then the leader cannot possibly be overtaken
            if (differenceBetweenTwoHighestWeights > --remainingLines) {
                break;
            }
        }

        double highestWeight = 0.0;
        Tuple<TimestampMatch, Set<String>> highestWeightMatch = null;
        for (Map.Entry<TimestampMatch, Tuple<Double, Set<String>>> entry : timestampMatches.entrySet()) {
            double weight = entry.getValue().v1();
            if (weight > highestWeight) {
                highestWeight = weight;
                highestWeightMatch = new Tuple<>(entry.getKey(), entry.getValue().v2());
            }
        }
        return highestWeightMatch;
    }

    /**
     * Used to weight a timestamp match according to how far along the line it is found.
     * Timestamps at the very beginning of the line are given a weight of 1.  The weight
     * progressively decreases the more text there is preceding the timestamp match, but
     * is always greater than 0.
     * @return A weight in the range (0, 1].
     */
    private static double weightForMatch(String preface) {
        return Math.pow(1.0 + preface.length() / 15.0, -1.1);
    }

    private static double findDifferenceBetweenTwoHighestWeights(Collection<Tuple<Double, Set<String>>> timestampMatches) {
        double highestWeight = 0.0;
        double secondHighestWeight = 0.0;
        for (Tuple<Double, Set<String>> timestampMatch : timestampMatches) {
            double weight = timestampMatch.v1();
            if (weight > highestWeight) {
                secondHighestWeight = highestWeight;
                highestWeight = weight;
            } else if (weight > secondHighestWeight) {
                secondHighestWeight = weight;
            }
        }
        return highestWeight - secondHighestWeight;
    }

    static String createMultiLineMessageStartRegex(Collection<String> prefaces, String timestampRegex) {

        StringBuilder builder = new StringBuilder("^");
        GrokPatternCreator.addIntermediateRegex(builder, prefaces);
        builder.append(timestampRegex);
        if (builder.substring(0, 3).equals("^\\b")) {
            builder.delete(1, 3);
        }
        return builder.toString();
    }
}
