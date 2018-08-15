/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.logstructurefinder;

import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.xpack.ml.logstructurefinder.TimestampFormatFinder.TimestampMatch;
import org.supercsv.exception.SuperCsvException;
import org.supercsv.io.CsvListReader;
import org.supercsv.prefs.CsvPreference;
import org.supercsv.util.Util;

import java.io.IOException;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.DoubleSummaryStatistics;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.SortedMap;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

public class SeparatedValuesLogStructureFinder implements LogStructureFinder {

    private static final int MAX_LEVENSHTEIN_COMPARISONS = 100;

    private final List<String> sampleMessages;
    private final LogStructure structure;

    static SeparatedValuesLogStructureFinder makeSeparatedValuesLogStructureFinder(List<String> explanation, String sample,
                                                                                   String charsetName, Boolean hasByteOrderMarker,
                                                                                   CsvPreference csvPreference, boolean trimFields)
        throws IOException {

        Tuple<List<List<String>>, List<Integer>> parsed = readRows(sample, csvPreference);
        List<List<String>> rows = parsed.v1();
        List<Integer> lineNumbers = parsed.v2();

        Tuple<Boolean, String[]> headerInfo = findHeaderFromSample(explanation, rows);
        boolean isHeaderInFile = headerInfo.v1();
        String[] header = headerInfo.v2();
        String[] headerWithNamedBlanks = new String[header.length];
        for (int i = 0; i < header.length; ++i) {
            String rawHeader = header[i].isEmpty() ? "column" + (i + 1) : header[i];
            headerWithNamedBlanks[i] = trimFields ? rawHeader.trim() : rawHeader;
        }

        List<String> sampleLines = Arrays.asList(sample.split("\n"));
        List<String> sampleMessages = new ArrayList<>();
        List<Map<String, ?>> sampleRecords = new ArrayList<>();
        int prevMessageEndLineNumber = isHeaderInFile ? lineNumbers.get(0) : -1;
        for (int index = isHeaderInFile ? 1 : 0; index < rows.size(); ++index) {
            List<String> row = rows.get(index);
            int lineNumber = lineNumbers.get(index);
            Map<String, String> sampleRecord = new LinkedHashMap<>();
            Util.filterListToMap(sampleRecord, headerWithNamedBlanks,
                trimFields ? row.stream().map(String::trim).collect(Collectors.toList()) : row);
            sampleRecords.add(sampleRecord);
            sampleMessages.add(
                sampleLines.subList(prevMessageEndLineNumber + 1, lineNumbers.get(index)).stream().collect(Collectors.joining("\n")));
            prevMessageEndLineNumber = lineNumber;
        }

        String preamble = Pattern.compile("\n").splitAsStream(sample).limit(lineNumbers.get(1)).collect(Collectors.joining("\n", "", "\n"));

        char delimiter = (char) csvPreference.getDelimiterChar();
        LogStructure.Builder structureBuilder = new LogStructure.Builder(LogStructure.Format.fromSeparator(delimiter))
            .setCharset(charsetName)
            .setHasByteOrderMarker(hasByteOrderMarker)
            .setSampleStart(preamble)
            .setNumLinesAnalyzed(lineNumbers.get(lineNumbers.size() - 1))
            .setNumMessagesAnalyzed(sampleRecords.size())
            .setHasHeaderRow(isHeaderInFile)
            .setInputFields(Arrays.stream(headerWithNamedBlanks).collect(Collectors.toList()));

        if (trimFields) {
            structureBuilder.setShouldTrimFields(true);
        }

        Tuple<String, TimestampMatch> timeField = LogStructureUtils.guessTimestampField(explanation, sampleRecords);
        if (timeField != null) {
            String timeLineRegex = null;
            StringBuilder builder = new StringBuilder("^");
            // We make the assumption that the timestamp will be on the first line of each record.  Therefore, if the
            // timestamp is the last column then either our assumption is wrong (and the approach will completely
            // break down) or else every record is on a single line and there's no point creating a multiline config.
            // This is why the loop excludes the last column.
            for (String column : Arrays.asList(header).subList(0, header.length - 1)) {
                if (timeField.v1().equals(column)) {
                    builder.append("\"?");
                    String simpleTimePattern = timeField.v2().simplePattern.pattern();
                    builder.append(simpleTimePattern.startsWith("\\b") ? simpleTimePattern.substring(2) : simpleTimePattern);
                    timeLineRegex = builder.toString();
                    break;
                } else {
                    builder.append(".*?");
                    if (delimiter == '\t') {
                        builder.append("\\t");
                    } else {
                        builder.append(delimiter);
                    }
                }
            }

            if (isHeaderInFile) {
                structureBuilder.setExcludeLinesPattern("^" + Arrays.stream(header)
                    .map(column -> "\"?" + column.replace("\"", "\"\"").replaceAll("([\\\\|()\\[\\]{}^$*?])", "\\\\$1") + "\"?")
                    .collect(Collectors.joining(",")));
            }

            structureBuilder.setTimestampField(timeField.v1())
                .setTimestampFormats(timeField.v2().dateFormats)
                .setNeedClientTimezone(timeField.v2().hasTimezoneDependentParsing())
                .setMultilineStartPattern(timeLineRegex);
        }

        SortedMap<String, Object> mappings = LogStructureUtils.guessMappings(explanation, sampleRecords);
        mappings.put(LogStructureUtils.DEFAULT_TIMESTAMP_FIELD, Collections.singletonMap(LogStructureUtils.MAPPING_TYPE_SETTING, "date"));

        LogStructure structure = structureBuilder
            .setMappings(mappings)
            .setExplanation(explanation)
            .build();

        return new SeparatedValuesLogStructureFinder(sampleMessages, structure);
    }

    private SeparatedValuesLogStructureFinder(List<String> sampleMessages, LogStructure structure) {
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

    static Tuple<List<List<String>>, List<Integer>> readRows(String sample, CsvPreference csvPreference) throws IOException {

        int fieldsInFirstRow = -1;

        List<List<String>> rows = new ArrayList<>();
        List<Integer> lineNumbers = new ArrayList<>();

        try (CsvListReader csvReader = new CsvListReader(new StringReader(sample), csvPreference)) {

            try {
                List<String> row;
                while ((row = csvReader.read()) != null) {
                    if (fieldsInFirstRow < 0) {
                        fieldsInFirstRow = row.size();
                    } else {
                        // Tolerate extra columns if and only if they're empty
                        while (row.size() > fieldsInFirstRow && row.get(row.size() - 1) == null) {
                            row.remove(row.size() - 1);
                        }
                    }
                    rows.add(row);
                    lineNumbers.add(csvReader.getLineNumber());
                }
            } catch (SuperCsvException e) {
                // Tolerate an incomplete last row
                if (notUnexpectedEndOfFile(e)) {
                    throw e;
                }
            }
        }

        assert rows.isEmpty() == false;
        assert lineNumbers.size() == rows.size();

        if (rows.get(0).size() != rows.get(rows.size() - 1).size()) {
            rows.remove(rows.size() - 1);
            lineNumbers.remove(lineNumbers.size() - 1);
        }

        // This should have been enforced by canCreateFromSample()
        assert rows.size() > 1;

        return new Tuple<>(rows, lineNumbers);
    }

    static Tuple<Boolean, String[]> findHeaderFromSample(List<String> explanation, List<List<String>> rows) {

        assert rows.isEmpty() == false;

        List<String> firstRow = rows.get(0);

        boolean isHeaderInFile = true;
        if (rowContainsDuplicateNonEmptyValues(firstRow)) {
            isHeaderInFile = false;
            explanation.add("First row contains duplicate values, so assuming it's not a header");
        } else {
            if (rows.size() < 3) {
                explanation.add("Too little data to accurately assess whether header is in sample - guessing it is");
            } else {
                isHeaderInFile = isFirstRowUnusual(explanation, rows);
            }
        }

        if (isHeaderInFile) {
            // SuperCSV will put nulls in the header if any columns don't have names, but empty strings are better for us
            return new Tuple<>(true, firstRow.stream().map(field -> (field == null) ? "" : field).toArray(String[]::new));
        } else {
            return new Tuple<>(false, IntStream.rangeClosed(1, firstRow.size()).mapToObj(num -> "column" + num).toArray(String[]::new));
        }
    }

    static boolean rowContainsDuplicateNonEmptyValues(List<String> row) {

        HashSet<String> values = new HashSet<>();

        for (String value : row) {
            if (value != null && value.isEmpty() == false && values.add(value) == false) {
                return true;
            }
        }

        return false;
    }

    private static boolean isFirstRowUnusual(List<String> explanation, List<List<String>> rows) {

        assert rows.size() >= 3;

        List<String> firstRow = rows.get(0);
        String firstRowStr = firstRow.stream().map(field -> (field == null) ? "" : field).collect(Collectors.joining(""));
        List<List<String>> otherRows = rows.subList(1, rows.size());
        List<String> otherRowStrs = new ArrayList<>();
        for (List<String> row : otherRows) {
            otherRowStrs.add(row.stream().map(str -> (str == null) ? "" : str).collect(Collectors.joining("")));
        }

        // Check lengths

        double firstRowLength = firstRowStr.length();
        DoubleSummaryStatistics otherRowStats = otherRowStrs.stream().mapToDouble(otherRow -> (double) otherRow.length())
            .collect(DoubleSummaryStatistics::new, DoubleSummaryStatistics::accept, DoubleSummaryStatistics::combine);

        double otherLengthRange = otherRowStats.getMax() - otherRowStats.getMin();
        if (firstRowLength < otherRowStats.getMin() - otherLengthRange / 10.0 ||
            firstRowLength > otherRowStats.getMax() + otherLengthRange / 10.0) {
            explanation.add("First row is unusual based on length test: [" + firstRowLength + "] and [" +
                toNiceString(otherRowStats) + "]");
            return true;
        }

        explanation.add("First row is not unusual based on length test: [" + firstRowLength + "] and [" +
            toNiceString(otherRowStats) + "]");

        // Check edit distances

        DoubleSummaryStatistics firstRowStats = otherRows.stream().limit(MAX_LEVENSHTEIN_COMPARISONS)
            .mapToDouble(otherRow -> (double) levenshteinFieldwiseCompareRows(firstRow, otherRow))
            .collect(DoubleSummaryStatistics::new, DoubleSummaryStatistics::accept, DoubleSummaryStatistics::combine);

        otherRowStats = new DoubleSummaryStatistics();
        int numComparisons = 0;
        int proportion = otherRowStrs.size() / MAX_LEVENSHTEIN_COMPARISONS;
        int innerIncrement = 1 + proportion * proportion;
        Random random = new Random(firstRow.hashCode());
        for (int i = 0; numComparisons < MAX_LEVENSHTEIN_COMPARISONS && i < otherRowStrs.size(); ++i) {
            for (int j = i + 1 + random.nextInt(innerIncrement); numComparisons < MAX_LEVENSHTEIN_COMPARISONS && j < otherRowStrs.size();
                 j += innerIncrement) {
                otherRowStats.accept((double) levenshteinFieldwiseCompareRows(otherRows.get(i), otherRows.get(j)));
                ++numComparisons;
            }
        }

        if (firstRowStats.getAverage() > otherRowStats.getAverage() * 1.2) {
            explanation.add("First row is unusual based on Levenshtein test [" + toNiceString(firstRowStats) +
                "] and [" + toNiceString(otherRowStats) + "]");
            return true;
        }

        explanation.add("First row is not unusual based on Levenshtein test [" + toNiceString(firstRowStats) +
            "] and [" + toNiceString(otherRowStats) + "]");

        return false;
    }

    private static String toNiceString(DoubleSummaryStatistics stats) {
        return String.format(Locale.ROOT, "count=%d, min=%f, average=%f, max=%f", stats.getCount(), stats.getMin(), stats.getAverage(),
            stats.getMax());
    }

    /**
     * Sum of the Levenshtein distances between corresponding elements
     * in the two supplied lists _excluding_ the biggest difference.
     * The reason the biggest difference is excluded is that sometimes
     * there's a "message" field that is much longer than any of the other
     * fields, varies enormously between rows, and skews the comparison.
     */
    static int levenshteinFieldwiseCompareRows(List<String> firstRow, List<String> secondRow) {

        int largestSize = Math.max(firstRow.size(), secondRow.size());
        if (largestSize <= 1) {
            return 0;
        }

        int[] distances = new int[largestSize];

        for (int index = 0; index < largestSize; ++index) {
            distances[index] = levenshteinDistance((index < firstRow.size()) ? firstRow.get(index) : "",
                (index < secondRow.size()) ? secondRow.get(index) : "");
        }

        Arrays.sort(distances);

        return IntStream.of(distances).limit(distances.length - 1).sum();
    }

    /**
     * This method implements the simple algorithm for calculating Levenshtein distance.
     */
    static int levenshteinDistance(String first, String second) {

        // There are some examples with pretty pictures of the matrix on Wikipedia here:
        // http://en.wikipedia.org/wiki/Levenshtein_distance

        int firstLen = (first == null) ? 0 : first.length();
        int secondLen = (second == null) ? 0 : second.length();
        if (firstLen == 0) {
            return secondLen;
        }
        if (secondLen == 0) {
            return firstLen;
        }

        int[] currentCol = new int[secondLen + 1];
        int[] prevCol = new int[secondLen + 1];

        // Populate the left column
        for (int down = 0; down <= secondLen; ++down) {
            currentCol[down] = down;
        }

        // Calculate the other entries in the matrix
        for (int across = 1; across <= firstLen; ++across) {
            int[] tmp = prevCol;
            prevCol = currentCol;
            // We could allocate a new array for currentCol here, but it's more efficient to reuse the one that's now redundant
            currentCol = tmp;

            currentCol[0] = across;

            for (int down = 1; down <= secondLen; ++down) {

                // Do the strings differ at the point we've reached?
                if (first.charAt(across - 1) == second.charAt(down - 1)) {

                    // No, they're the same => no extra cost
                    currentCol[down] = prevCol[down - 1];
                } else {
                    // Yes, they differ, so there are 3 options:

                    // 1) Deletion => cell to the left's value plus 1
                    int option1 = prevCol[down];

                    // 2) Insertion => cell above's value plus 1
                    int option2 = currentCol[down - 1];

                    // 3) Substitution => cell above left's value plus 1
                    int option3 = prevCol[down - 1];

                    // Take the cheapest option of the 3
                    currentCol[down] = Math.min(Math.min(option1, option2), option3) + 1;
                }
            }
        }

        // Result is the value in the bottom right hand corner of the matrix
        return currentCol[secondLen];
    }

    static boolean lineHasUnescapedQuote(String line, CsvPreference csvPreference) {
        char quote = csvPreference.getQuoteChar();
        String lineWithEscapedQuotesRemoved = line.replace(String.valueOf(quote) + quote, "");
        for (int index = 1; index < lineWithEscapedQuotesRemoved.length() - 1; ++index) {
            if (lineWithEscapedQuotesRemoved.charAt(index) == quote &&
                lineWithEscapedQuotesRemoved.codePointAt(index - 1) != csvPreference.getDelimiterChar() &&
                lineWithEscapedQuotesRemoved.codePointAt(index + 1) != csvPreference.getDelimiterChar()) {
                return true;
            }
        }
        return false;
    }

    static boolean canCreateFromSample(List<String> explanation, String sample, int minFieldsPerRow, CsvPreference csvPreference,
                                       String formatName) {

        // Logstash's CSV parser won't tolerate fields where just part of the
        // value is quoted, whereas SuperCSV will, hence this extra check
        String[] sampleLines = sample.split("\n");
        for (String sampleLine : sampleLines) {
            if (lineHasUnescapedQuote(sampleLine, csvPreference)) {
                explanation.add("Not " + formatName +
                    " because a line has an unescaped quote that is not at the beginning or end of a field: [" + sampleLine + "]");
                return false;
            }
        }

        try (CsvListReader csvReader = new CsvListReader(new StringReader(sample), csvPreference)) {

            int fieldsInFirstRow = -1;
            int fieldsInLastRow = -1;

            int numberOfRows = 0;
            try {
                List<String> row;
                while ((row = csvReader.read()) != null) {

                    int fieldsInThisRow = row.size();
                    ++numberOfRows;
                    if (fieldsInFirstRow < 0) {
                        fieldsInFirstRow = fieldsInThisRow;
                        if (fieldsInFirstRow < minFieldsPerRow) {
                            explanation.add("Not " + formatName + " because the first row has fewer than [" + minFieldsPerRow +
                                "] fields: [" + fieldsInFirstRow + "]");
                            return false;
                        }
                        fieldsInLastRow = fieldsInFirstRow;
                        continue;
                    }

                    // Tolerate extra columns if and only if they're empty
                    while (fieldsInThisRow > fieldsInFirstRow && row.get(fieldsInThisRow - 1) == null) {
                        --fieldsInThisRow;
                    }

                    if (fieldsInLastRow != fieldsInFirstRow) {
                        explanation.add("Not " + formatName + " because row [" + (numberOfRows - 1) +
                            "] has a different number of fields to the first row: [" + fieldsInFirstRow + "] and [" +
                            fieldsInLastRow + "]");
                        return false;
                    }

                    fieldsInLastRow = fieldsInThisRow;
                }

                if (fieldsInLastRow > fieldsInFirstRow) {
                    explanation.add("Not " + formatName + " because last row has more fields than first row: [" + fieldsInFirstRow +
                        "] and [" + fieldsInLastRow + "]");
                    return false;
                }
                if (fieldsInLastRow < fieldsInFirstRow) {
                    --numberOfRows;
                }
            } catch (SuperCsvException e) {
                // Tolerate an incomplete last row
                if (notUnexpectedEndOfFile(e)) {
                    explanation.add("Not " + formatName + " because there was a parsing exception: [" + e.getMessage() + "]");
                    return false;
                }
            }
            if (numberOfRows <= 1) {
                explanation.add("Not " + formatName + " because fewer than 2 complete records in sample: [" + numberOfRows + "]");
                return false;
            }
            explanation.add("Deciding sample is " + formatName);
            return true;

        } catch (IOException e) {
            explanation.add("Not " + formatName + " because there was a parsing exception: [" + e.getMessage() + "]");
            return false;
        }
    }

    private static boolean notUnexpectedEndOfFile(SuperCsvException e) {
        return e.getMessage().startsWith("unexpected end of file while reading quoted column") == false;
    }
}
