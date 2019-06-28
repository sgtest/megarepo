/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.dataframe.extractor;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.fieldcaps.FieldCapabilities;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesResponse;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsConfig;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsDest;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsSource;
import org.elasticsearch.xpack.core.ml.dataframe.analyses.OutlierDetection;
import org.elasticsearch.xpack.ml.datafeed.extractor.fields.ExtractedField;
import org.elasticsearch.xpack.ml.datafeed.extractor.fields.ExtractedFields;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.contains;
import static org.hamcrest.Matchers.containsInAnyOrder;
import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class ExtractedFieldsDetectorTests extends ESTestCase {

    private static final String[] SOURCE_INDEX = new String[] { "source_index" };
    private static final String DEST_INDEX = "dest_index";
    private static final String RESULTS_FIELD = "ml";

    public void testDetect_GivenFloatField() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("some_float", "float").build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<ExtractedField> allFields = extractedFields.getAllFields();
        assertThat(allFields.size(), equalTo(1));
        assertThat(allFields.get(0).getName(), equalTo("some_float"));
        assertThat(allFields.get(0).getExtractionMethod(), equalTo(ExtractedField.ExtractionMethod.DOC_VALUE));
    }

    public void testDetect_GivenNumericFieldWithMultipleTypes() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("some_number", "long", "integer", "short", "byte", "double", "float", "half_float", "scaled_float")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<ExtractedField> allFields = extractedFields.getAllFields();
        assertThat(allFields.size(), equalTo(1));
        assertThat(allFields.get(0).getName(), equalTo("some_number"));
        assertThat(allFields.get(0).getExtractionMethod(), equalTo(ExtractedField.ExtractionMethod.DOC_VALUE));
    }

    public void testDetect_GivenNonNumericField() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("some_keyword", "keyword").build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());

        assertThat(e.getMessage(), equalTo("No compatible fields could be detected in index [source_index]"));
    }

    public void testDetect_GivenFieldWithNumericAndNonNumericTypes() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("indecisive_field", "float", "keyword").build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());

        assertThat(e.getMessage(), equalTo("No compatible fields could be detected in index [source_index]"));
    }

    public void testDetect_GivenMultipleFields() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("some_float", "float")
            .addAggregatableField("some_long", "long")
            .addAggregatableField("some_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<ExtractedField> allFields = extractedFields.getAllFields();
        assertThat(allFields.size(), equalTo(2));
        assertThat(allFields.stream().map(ExtractedField::getName).collect(Collectors.toSet()),
            containsInAnyOrder("some_float", "some_long"));
        assertThat(allFields.stream().map(ExtractedField::getExtractionMethod).collect(Collectors.toSet()),
            contains(equalTo(ExtractedField.ExtractionMethod.DOC_VALUE)));
    }

    public void testDetect_GivenIgnoredField() {
        FieldCapabilitiesResponse fieldCapabilities= new MockFieldCapsResponseBuilder()
            .addAggregatableField("_id", "float").build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());

        assertThat(e.getMessage(), equalTo("No compatible fields could be detected in index [source_index]"));
    }

    public void testDetect_ShouldSortFieldsAlphabetically() {
        int fieldCount = randomIntBetween(10, 20);
        List<String> fields = new ArrayList<>();
        for (int i = 0; i < fieldCount; i++) {
            fields.add(randomAlphaOfLength(20));
        }
        List<String> sortedFields = new ArrayList<>(fields);
        Collections.sort(sortedFields);

        MockFieldCapsResponseBuilder mockFieldCapsResponseBuilder = new MockFieldCapsResponseBuilder();
        for (String field : fields) {
            mockFieldCapsResponseBuilder.addAggregatableField(field, "float");
        }
        FieldCapabilitiesResponse fieldCapabilities = mockFieldCapsResponseBuilder.build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(sortedFields));
    }

    public void testDetectedExtractedFields_GivenIncludeWithMissingField() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("my_field1", "float")
            .addAggregatableField("my_field2", "float")
            .build();

        FetchSourceContext desiredFields = new FetchSourceContext(true, new String[]{"your_field1", "my*"}, new String[0]);

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(desiredFields), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());

        assertThat(e.getMessage(), equalTo("No field [your_field1] could be detected"));
    }

    public void testDetectedExtractedFields_GivenExcludeAllValidFields() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("my_field1", "float")
            .addAggregatableField("my_field2", "float")
            .build();

        FetchSourceContext desiredFields = new FetchSourceContext(true, new String[0], new String[]{"my_*"});

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(desiredFields), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());
        assertThat(e.getMessage(), equalTo("No compatible fields could be detected in index [source_index]"));
    }

    public void testDetectedExtractedFields_GivenInclusionsAndExclusions() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("my_field1_nope", "float")
            .addAggregatableField("my_field1", "float")
            .addAggregatableField("your_field2", "float")
            .addAggregatableField("your_keyword", "keyword")
            .build();

        FetchSourceContext desiredFields = new FetchSourceContext(true, new String[]{"your*", "my_*"}, new String[]{"*nope"});

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(desiredFields), false, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(Arrays.asList("my_field1", "your_field2")));
    }

    public void testDetectedExtractedFields_GivenIndexContainsResultsField() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField(RESULTS_FIELD, "float")
            .addAggregatableField("my_field1", "float")
            .addAggregatableField("your_field2", "float")
            .addAggregatableField("your_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), false, 100, fieldCapabilities);
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class, () -> extractedFieldsDetector.detect());

        assertThat(e.getMessage(), equalTo("A field that matches the dest.results_field [ml] already exists; " +
            "please set a different results_field"));
    }

    public void testDetectedExtractedFields_GivenIndexContainsResultsFieldAndTaskIsRestarting() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField(RESULTS_FIELD + ".outlier_score", "float")
            .addAggregatableField("my_field1", "float")
            .addAggregatableField("your_field2", "float")
            .addAggregatableField("your_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), true, 100, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(Arrays.asList("my_field1", "your_field2")));
    }

    public void testDetectedExtractedFields_GivenLessFieldsThanDocValuesLimit() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("field_1", "float")
            .addAggregatableField("field_2", "float")
            .addAggregatableField("field_3", "float")
            .addAggregatableField("a_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), true, 4, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(Arrays.asList("field_1", "field_2", "field_3")));
        assertThat(extractedFields.getAllFields().stream().map(ExtractedField::getExtractionMethod).collect(Collectors.toSet()),
            contains(equalTo(ExtractedField.ExtractionMethod.DOC_VALUE)));
    }

    public void testDetectedExtractedFields_GivenEqualFieldsToDocValuesLimit() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("field_1", "float")
            .addAggregatableField("field_2", "float")
            .addAggregatableField("field_3", "float")
            .addAggregatableField("a_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), true, 3, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(Arrays.asList("field_1", "field_2", "field_3")));
        assertThat(extractedFields.getAllFields().stream().map(ExtractedField::getExtractionMethod).collect(Collectors.toSet()),
            contains(equalTo(ExtractedField.ExtractionMethod.DOC_VALUE)));
    }

    public void testDetectedExtractedFields_GivenMoreFieldsThanDocValuesLimit() {
        FieldCapabilitiesResponse fieldCapabilities = new MockFieldCapsResponseBuilder()
            .addAggregatableField("field_1", "float")
            .addAggregatableField("field_2", "float")
            .addAggregatableField("field_3", "float")
            .addAggregatableField("a_keyword", "keyword")
            .build();

        ExtractedFieldsDetector extractedFieldsDetector = new ExtractedFieldsDetector(
            SOURCE_INDEX, buildAnalyticsConfig(), true, 2, fieldCapabilities);
        ExtractedFields extractedFields = extractedFieldsDetector.detect();

        List<String> extractedFieldNames = extractedFields.getAllFields().stream().map(ExtractedField::getName)
            .collect(Collectors.toList());
        assertThat(extractedFieldNames, equalTo(Arrays.asList("field_1", "field_2", "field_3")));
        assertThat(extractedFields.getAllFields().stream().map(ExtractedField::getExtractionMethod).collect(Collectors.toSet()),
            contains(equalTo(ExtractedField.ExtractionMethod.SOURCE)));
    }

    private static DataFrameAnalyticsConfig buildAnalyticsConfig() {
        return buildAnalyticsConfig(null);
    }

    private static DataFrameAnalyticsConfig buildAnalyticsConfig(FetchSourceContext analyzedFields) {
        return new DataFrameAnalyticsConfig.Builder("foo")
            .setSource(new DataFrameAnalyticsSource(SOURCE_INDEX, null))
            .setDest(new DataFrameAnalyticsDest(DEST_INDEX, null))
            .setAnalyzedFields(analyzedFields)
            .setAnalysis(new OutlierDetection())
            .build();
    }

    private static class MockFieldCapsResponseBuilder {

        private final Map<String, Map<String, FieldCapabilities>> fieldCaps = new HashMap<>();

        private MockFieldCapsResponseBuilder addAggregatableField(String field, String... types) {
            Map<String, FieldCapabilities> caps = new HashMap<>();
            for (String type : types) {
                caps.put(type, new FieldCapabilities(field, type, true, true));
            }
            fieldCaps.put(field, caps);
            return this;
        }

        private FieldCapabilitiesResponse build() {
            FieldCapabilitiesResponse response = mock(FieldCapabilitiesResponse.class);
            when(response.get()).thenReturn(fieldCaps);

            for (String field : fieldCaps.keySet()) {
                when(response.getField(field)).thenReturn(fieldCaps.get(field));
            }
            return response;
        }
    }
}
