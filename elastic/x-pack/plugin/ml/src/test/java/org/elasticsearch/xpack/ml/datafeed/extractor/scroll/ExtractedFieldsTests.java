/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.datafeed.extractor.scroll;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.action.fieldcaps.FieldCapabilities;
import org.elasticsearch.action.fieldcaps.FieldCapabilitiesResponse;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.datafeed.DatafeedConfig;
import org.elasticsearch.xpack.core.ml.job.config.AnalysisConfig;
import org.elasticsearch.xpack.core.ml.job.config.DataDescription;
import org.elasticsearch.xpack.core.ml.job.config.Detector;
import org.elasticsearch.xpack.core.ml.job.config.Job;
import org.elasticsearch.xpack.ml.test.SearchHitBuilder;
import org.joda.time.DateTime;

import java.util.Arrays;
import java.util.Collections;
import java.util.Date;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class ExtractedFieldsTests extends ESTestCase {

    private ExtractedField timeField = ExtractedField.newTimeField("time", ExtractedField.ExtractionMethod.DOC_VALUE);

    public void testInvalidConstruction() {
        expectThrows(IllegalArgumentException.class, () -> new ExtractedFields(timeField, Collections.emptyList()));
    }

    public void testTimeFieldOnly() {
        ExtractedFields extractedFields = new ExtractedFields(timeField, Arrays.asList(timeField));

        assertThat(extractedFields.getAllFields(), equalTo(Arrays.asList(timeField)));
        assertThat(extractedFields.timeField(), equalTo("time"));
        assertThat(extractedFields.getDocValueFields(), equalTo(new String[] { timeField.getName() }));
        assertThat(extractedFields.getSourceFields().length, equalTo(0));
    }

    public void testAllTypesOfFields() {
        ExtractedField docValue1 = ExtractedField.newField("doc1", ExtractedField.ExtractionMethod.DOC_VALUE);
        ExtractedField docValue2 = ExtractedField.newField("doc2", ExtractedField.ExtractionMethod.DOC_VALUE);
        ExtractedField scriptField1 = ExtractedField.newField("scripted1", ExtractedField.ExtractionMethod.SCRIPT_FIELD);
        ExtractedField scriptField2 = ExtractedField.newField("scripted2", ExtractedField.ExtractionMethod.SCRIPT_FIELD);
        ExtractedField sourceField1 = ExtractedField.newField("src1", ExtractedField.ExtractionMethod.SOURCE);
        ExtractedField sourceField2 = ExtractedField.newField("src2", ExtractedField.ExtractionMethod.SOURCE);
        ExtractedFields extractedFields = new ExtractedFields(timeField, Arrays.asList(timeField,
                docValue1, docValue2, scriptField1, scriptField2, sourceField1, sourceField2));

        assertThat(extractedFields.getAllFields().size(), equalTo(7));
        assertThat(extractedFields.timeField(), equalTo("time"));
        assertThat(extractedFields.getDocValueFields(), equalTo(new String[] {"time", "doc1", "doc2"}));
        assertThat(extractedFields.getSourceFields(), equalTo(new String[] {"src1", "src2"}));
    }

    public void testTimeFieldValue() {
        final long millis = randomLong();
        final SearchHit hit = new SearchHitBuilder(randomInt()).addField("time", new DateTime(millis)).build();
        final ExtractedFields extractedFields = new ExtractedFields(timeField, Collections.singletonList(timeField));
        assertThat(extractedFields.timeFieldValue(hit), equalTo(millis));
    }

    public void testStringTimeFieldValue() {
        final long millis = randomLong();
        final SearchHit hit = new SearchHitBuilder(randomInt()).addField("time", Long.toString(millis)).build();
        final ExtractedFields extractedFields = new ExtractedFields(timeField, Collections.singletonList(timeField));
        assertThat(extractedFields.timeFieldValue(hit), equalTo(millis));
    }

    public void testPre6xTimeFieldValue() {
        // Prior to 6.x, timestamps were simply `long` milliseconds-past-the-epoch values
        final long millis = randomLong();
        final SearchHit hit = new SearchHitBuilder(randomInt()).addField("time", millis).build();
        final ExtractedFields extractedFields = new ExtractedFields(timeField, Collections.singletonList(timeField));
        assertThat(extractedFields.timeFieldValue(hit), equalTo(millis));
    }

    public void testTimeFieldValueGivenEmptyArray() {
        SearchHit hit = new SearchHitBuilder(1).addField("time", Collections.emptyList()).build();

        ExtractedFields extractedFields = new ExtractedFields(timeField, Arrays.asList(timeField));

        expectThrows(RuntimeException.class, () -> extractedFields.timeFieldValue(hit));
    }

    public void testTimeFieldValueGivenValueHasTwoElements() {
        SearchHit hit = new SearchHitBuilder(1).addField("time", Arrays.asList(1L, 2L)).build();

        ExtractedFields extractedFields = new ExtractedFields(timeField, Arrays.asList(timeField));

        expectThrows(RuntimeException.class, () -> extractedFields.timeFieldValue(hit));
    }

    public void testTimeFieldValueGivenValueIsString() {
        SearchHit hit = new SearchHitBuilder(1).addField("time", "a string").build();

        ExtractedFields extractedFields = new ExtractedFields(timeField, Arrays.asList(timeField));

        expectThrows(RuntimeException.class, () -> extractedFields.timeFieldValue(hit));
    }

    public void testBuildGivenMixtureOfTypes() {
        Job.Builder jobBuilder = new Job.Builder("foo");
        jobBuilder.setDataDescription(new DataDescription.Builder());
        Detector.Builder detector = new Detector.Builder("mean", "value");
        detector.setByFieldName("airline");
        detector.setOverFieldName("airport");
        jobBuilder.setAnalysisConfig(new AnalysisConfig.Builder(Collections.singletonList(detector.build())));

        DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder("feed", jobBuilder.getId());
        datafeedBuilder.setIndices(Collections.singletonList("foo"));
        datafeedBuilder.setTypes(Collections.singletonList("doc"));
        datafeedBuilder.setScriptFields(Collections.singletonList(new SearchSourceBuilder.ScriptField("airport", null, false)));

        Map<String, FieldCapabilities> timeCaps = new HashMap<>();
        timeCaps.put("date", createFieldCaps(true));
        Map<String, FieldCapabilities> valueCaps = new HashMap<>();
        valueCaps.put("float", createFieldCaps(true));
        valueCaps.put("keyword", createFieldCaps(true));
        Map<String, FieldCapabilities> airlineCaps = new HashMap<>();
        airlineCaps.put("text", createFieldCaps(false));
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getField("time")).thenReturn(timeCaps);
        when(fieldCapabilitiesResponse.getField("value")).thenReturn(valueCaps);
        when(fieldCapabilitiesResponse.getField("airline")).thenReturn(airlineCaps);

        ExtractedFields extractedFields = ExtractedFields.build(jobBuilder.build(new Date()), datafeedBuilder.build(),
                fieldCapabilitiesResponse);

        assertThat(extractedFields.timeField(), equalTo("time"));
        assertThat(extractedFields.getDocValueFields().length, equalTo(2));
        assertThat(extractedFields.getDocValueFields()[0], equalTo("time"));
        assertThat(extractedFields.getDocValueFields()[1], equalTo("value"));
        assertThat(extractedFields.getSourceFields().length, equalTo(1));
        assertThat(extractedFields.getSourceFields()[0], equalTo("airline"));
        assertThat(extractedFields.getAllFields().size(), equalTo(4));
    }

    public void testBuildGivenMultiFields() {
        Job.Builder jobBuilder = new Job.Builder("foo");
        jobBuilder.setDataDescription(new DataDescription.Builder());
        Detector.Builder detector = new Detector.Builder("count", null);
        detector.setByFieldName("airline.text");
        detector.setOverFieldName("airport.keyword");
        jobBuilder.setAnalysisConfig(new AnalysisConfig.Builder(Collections.singletonList(detector.build())));

        DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder("feed", jobBuilder.getId());
        datafeedBuilder.setIndices(Collections.singletonList("foo"));

        Map<String, FieldCapabilities> timeCaps = new HashMap<>();
        timeCaps.put("date", createFieldCaps(true));
        Map<String, FieldCapabilities> text = new HashMap<>();
        text.put("text", createFieldCaps(false));
        Map<String, FieldCapabilities> keyword = new HashMap<>();
        keyword.put("keyword", createFieldCaps(true));
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getField("time")).thenReturn(timeCaps);
        when(fieldCapabilitiesResponse.getField("airline")).thenReturn(text);
        when(fieldCapabilitiesResponse.getField("airline.text")).thenReturn(text);
        when(fieldCapabilitiesResponse.getField("airport")).thenReturn(text);
        when(fieldCapabilitiesResponse.getField("airport.keyword")).thenReturn(keyword);

        ExtractedFields extractedFields = ExtractedFields.build(jobBuilder.build(new Date()), datafeedBuilder.build(),
                fieldCapabilitiesResponse);

        assertThat(extractedFields.timeField(), equalTo("time"));
        assertThat(extractedFields.getDocValueFields().length, equalTo(2));
        assertThat(extractedFields.getDocValueFields()[0], equalTo("time"));
        assertThat(extractedFields.getDocValueFields()[1], equalTo("airport.keyword"));
        assertThat(extractedFields.getSourceFields().length, equalTo(1));
        assertThat(extractedFields.getSourceFields()[0], equalTo("airline"));
        assertThat(extractedFields.getAllFields().size(), equalTo(3));

        assertThat(extractedFields.getAllFields().stream().filter(f -> f.getName().equals("time")).findFirst().get().getAlias(),
                equalTo("time"));
        assertThat(extractedFields.getAllFields().stream().filter(f -> f.getName().equals("airport.keyword")).findFirst().get().getAlias(),
                equalTo("airport.keyword"));
        assertThat(extractedFields.getAllFields().stream().filter(f -> f.getName().equals("airline")).findFirst().get().getAlias(),
                equalTo("airline.text"));
    }

    public void testBuildGivenTimeFieldIsNotAggregatable() {
        Job.Builder jobBuilder = new Job.Builder("foo");
        jobBuilder.setDataDescription(new DataDescription.Builder());
        Detector.Builder detector = new Detector.Builder("count", null);
        jobBuilder.setAnalysisConfig(new AnalysisConfig.Builder(Collections.singletonList(detector.build())));

        DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder("feed", jobBuilder.getId());
        datafeedBuilder.setIndices(Collections.singletonList("foo"));
        datafeedBuilder.setTypes(Collections.singletonList("doc"));

        Map<String, FieldCapabilities> timeCaps = new HashMap<>();
        timeCaps.put("date", createFieldCaps(false));
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getField("time")).thenReturn(timeCaps);

        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class,
                () -> ExtractedFields.build(jobBuilder.build(new Date()), datafeedBuilder.build(), fieldCapabilitiesResponse));
        assertThat(e.status(), equalTo(RestStatus.BAD_REQUEST));
        assertThat(e.getMessage(), equalTo("datafeed [feed] cannot retrieve time field [time] because it is not aggregatable"));
    }

    public void testBuildGivenTimeFieldIsNotAggregatableInSomeIndices() {
        Job.Builder jobBuilder = new Job.Builder("foo");
        jobBuilder.setDataDescription(new DataDescription.Builder());
        Detector.Builder detector = new Detector.Builder("count", null);
        jobBuilder.setAnalysisConfig(new AnalysisConfig.Builder(Collections.singletonList(detector.build())));

        DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder("feed", jobBuilder.getId());
        datafeedBuilder.setIndices(Collections.singletonList("foo"));
        datafeedBuilder.setTypes(Collections.singletonList("doc"));

        Map<String, FieldCapabilities> timeCaps = new HashMap<>();
        timeCaps.put("date", createFieldCaps(true));
        timeCaps.put("text", createFieldCaps(false));
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getField("time")).thenReturn(timeCaps);

        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class,
                () -> ExtractedFields.build(jobBuilder.build(new Date()), datafeedBuilder.build(), fieldCapabilitiesResponse));
        assertThat(e.status(), equalTo(RestStatus.BAD_REQUEST));
        assertThat(e.getMessage(), equalTo("datafeed [feed] cannot retrieve time field [time] because it is not aggregatable"));
    }

    public void testBuildGivenFieldWithoutMappings() {
        Job.Builder jobBuilder = new Job.Builder("foo");
        jobBuilder.setDataDescription(new DataDescription.Builder());
        Detector.Builder detector = new Detector.Builder("max", "value");
        jobBuilder.setAnalysisConfig(new AnalysisConfig.Builder(Collections.singletonList(detector.build())));

        DatafeedConfig.Builder datafeedBuilder = new DatafeedConfig.Builder("feed", jobBuilder.getId());
        datafeedBuilder.setIndices(Collections.singletonList("foo"));
        datafeedBuilder.setTypes(Collections.singletonList("doc"));

        Map<String, FieldCapabilities> timeCaps = new HashMap<>();
        timeCaps.put("date", createFieldCaps(true));
        FieldCapabilitiesResponse fieldCapabilitiesResponse = mock(FieldCapabilitiesResponse.class);
        when(fieldCapabilitiesResponse.getField("time")).thenReturn(timeCaps);

        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class,
                () -> ExtractedFields.build(jobBuilder.build(new Date()), datafeedBuilder.build(), fieldCapabilitiesResponse));
        assertThat(e.status(), equalTo(RestStatus.BAD_REQUEST));
        assertThat(e.getMessage(), equalTo("datafeed [feed] cannot retrieve field [value] because it has no mappings"));
    }

    private static FieldCapabilities createFieldCaps(boolean isAggregatable) {
        FieldCapabilities fieldCaps = mock(FieldCapabilities.class);
        when(fieldCaps.isAggregatable()).thenReturn(isAggregatable);
        return fieldCaps;
    }
}
