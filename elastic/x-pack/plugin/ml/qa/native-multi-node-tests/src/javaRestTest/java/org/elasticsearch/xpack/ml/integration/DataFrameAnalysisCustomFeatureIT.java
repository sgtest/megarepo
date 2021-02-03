/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */
package org.elasticsearch.xpack.ml.integration;

import org.elasticsearch.ElasticsearchException;
import org.elasticsearch.action.DocWriteRequest;
import org.elasticsearch.action.admin.indices.refresh.RefreshRequest;
import org.elasticsearch.action.bulk.BulkRequestBuilder;
import org.elasticsearch.action.bulk.BulkResponse;
import org.elasticsearch.action.get.GetResponse;
import org.elasticsearch.action.index.IndexRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.WriteRequest;
import org.elasticsearch.common.collect.MapBuilder;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.fetch.subphase.FetchSourceContext;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsConfig;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsDest;
import org.elasticsearch.xpack.core.ml.dataframe.DataFrameAnalyticsSource;
import org.elasticsearch.xpack.core.ml.dataframe.analyses.BoostedTreeParams;
import org.elasticsearch.xpack.core.ml.dataframe.analyses.MlDataFrameAnalysisNamedXContentProvider;
import org.elasticsearch.xpack.core.ml.dataframe.analyses.Regression;
import org.elasticsearch.xpack.core.ml.inference.MlInferenceNamedXContentProvider;
import org.elasticsearch.xpack.core.ml.inference.preprocessing.FrequencyEncoding;
import org.elasticsearch.xpack.core.ml.inference.preprocessing.Multi;
import org.elasticsearch.xpack.core.ml.inference.preprocessing.NGram;
import org.elasticsearch.xpack.core.ml.inference.preprocessing.OneHotEncoding;
import org.elasticsearch.xpack.core.ml.inference.preprocessing.PreProcessor;
import org.elasticsearch.xpack.core.ml.utils.QueryProvider;
import org.junit.After;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.anyOf;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.everyItem;
import static org.hamcrest.Matchers.hasKey;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.startsWith;

public class DataFrameAnalysisCustomFeatureIT extends MlNativeDataFrameAnalyticsIntegTestCase {

    private static final String BOOLEAN_FIELD = "boolean-field";
    private static final String NUMERICAL_FIELD = "numerical-field";
    private static final String DISCRETE_NUMERICAL_FIELD = "discrete-numerical-field";
    private static final String TEXT_FIELD = "text-field";
    private static final String KEYWORD_FIELD = "keyword-field";
    private static final String NESTED_FIELD = "outer-field.inner-field";
    private static final String ALIAS_TO_KEYWORD_FIELD = "alias-to-keyword-field";
    private static final String ALIAS_TO_NESTED_FIELD = "alias-to-nested-field";
    private static final List<Boolean> BOOLEAN_FIELD_VALUES = List.of(false, true);
    private static final List<Double> NUMERICAL_FIELD_VALUES = List.of(1.0, 2.0);
    private static final List<Integer> DISCRETE_NUMERICAL_FIELD_VALUES = List.of(10, 20);
    private static final List<String> KEYWORD_FIELD_VALUES = List.of("cat", "dog");

    private String jobId;
    private String sourceIndex;
    private String destIndex;

    @Before
    public void setupLogging() {
        client().admin().cluster()
            .prepareUpdateSettings()
            .setTransientSettings(Settings.builder()
                .put("logger.org.elasticsearch.xpack.ml.dataframe", "DEBUG")
                .put("logger.org.elasticsearch.xpack.core.ml.inference", "DEBUG"))
            .get();
    }

    @After
    public void cleanup() {
        cleanUp();
        client().admin().cluster()
            .prepareUpdateSettings()
            .setTransientSettings(Settings.builder()
                .putNull("logger.org.elasticsearch.xpack.ml.dataframe")
                .putNull("logger.org.elasticsearch.xpack.core.ml.inference"))
            .get();
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, Collections.emptyList());
        List<NamedXContentRegistry.Entry> entries = new ArrayList<>(searchModule.getNamedXContents());
        entries.addAll(new MlInferenceNamedXContentProvider().getNamedXContentParsers());
        entries.addAll(new MlDataFrameAnalysisNamedXContentProvider().getNamedXContentParsers());
        return new NamedXContentRegistry(entries);
    }

    public void testNGramCustomFeature() throws Exception {
        initialize("test_ngram_feature_processor");
        String predictedClassField = NUMERICAL_FIELD + "_prediction";
        indexData(sourceIndex, 300, 50, NUMERICAL_FIELD);

        DataFrameAnalyticsConfig config = new DataFrameAnalyticsConfig.Builder()
            .setId(jobId)
            .setSource(new DataFrameAnalyticsSource(new String[] { sourceIndex },
                QueryProvider.fromParsedQuery(QueryBuilders.matchAllQuery()), null))
            .setDest(new DataFrameAnalyticsDest(destIndex, null))
            .setAnalysis(new Regression(NUMERICAL_FIELD,
                BoostedTreeParams.builder().setNumTopFeatureImportanceValues(6).build(),
                null,
                null,
                42L,
                null,
                null,
                Arrays.asList(
                    new NGram(TEXT_FIELD, "f", new int[]{1}, 0, 2, true),
                    new Multi(new PreProcessor[]{
                        new NGram(TEXT_FIELD, "ngram", new int[]{2}, 0, 3, true),
                        new FrequencyEncoding("ngram.20",
                            "frequency",
                            MapBuilder.<String, Double>newMapBuilder().put("ca", 5.0).put("do", 1.0).map(), true),
                        new OneHotEncoding("ngram.21", MapBuilder.<String, String>newMapBuilder().put("at", "is_cat").map(), true)
                    },
                        true)
                    ),
                    null))
            .setAnalyzedFields(new FetchSourceContext(true, new String[]{TEXT_FIELD, NUMERICAL_FIELD}, new String[]{}))
            .build();
        putAnalytics(config);

        assertIsStopped(jobId);
        assertProgressIsZero(jobId);

        startAnalytics(jobId);
        waitUntilAnalyticsIsStopped(jobId);

        client().admin().indices().refresh(new RefreshRequest(destIndex));
        SearchResponse sourceData = client().prepareSearch(sourceIndex).setTrackTotalHits(true).setSize(1000).get();
        for (SearchHit hit : sourceData.getHits()) {
            Map<String, Object> destDoc = getDestDoc(config, hit);
            Map<String, Object> resultsObject = getFieldValue(destDoc, "ml");
            @SuppressWarnings("unchecked")
            List<Map<String, Object>> importanceArray = (List<Map<String, Object>>)resultsObject.get("feature_importance");
            assertThat(importanceArray.stream().map(m -> m.get("feature_name").toString()).collect(Collectors.toSet()),
                everyItem(anyOf(startsWith("f."), startsWith("ngram"), equalTo("is_cat"), equalTo("frequency"))));
        }

        assertProgressComplete(jobId);
        assertThat(searchStoredProgress(jobId).getHits().getTotalHits().value, equalTo(1L));
        assertExactlyOneInferenceModelPersisted(jobId);
        assertModelStatePersisted(stateDocId());
    }

    private void initialize(String jobId) {
        initialize(jobId, false);
    }

    private void initialize(String jobId, boolean isDatastream) {
        this.jobId = jobId;
        this.sourceIndex = jobId + "_source_index";
        this.destIndex = sourceIndex + "_results";
        boolean analysisUsesExistingDestIndex = randomBoolean();
        createIndex(sourceIndex, isDatastream);
        if (analysisUsesExistingDestIndex) {
            createIndex(destIndex, false);
        }
    }

    private static void createIndex(String index, boolean isDatastream) {
        String mapping = "{\n" +
            "      \"properties\": {\n" +
            "        \"@timestamp\": {\n" +
            "          \"type\": \"date\"\n" +
            "        }," +
            "        \""+ BOOLEAN_FIELD + "\": {\n" +
            "          \"type\": \"boolean\"\n" +
            "        }," +
            "        \""+ NUMERICAL_FIELD + "\": {\n" +
            "          \"type\": \"double\"\n" +
            "        }," +
            "        \""+ DISCRETE_NUMERICAL_FIELD + "\": {\n" +
            "          \"type\": \"unsigned_long\"\n" +
            "        }," +
            "        \""+ TEXT_FIELD + "\": {\n" +
            "          \"type\": \"text\"\n" +
            "        }," +
            "        \""+ KEYWORD_FIELD + "\": {\n" +
            "          \"type\": \"keyword\"\n" +
            "        }," +
            "        \""+ NESTED_FIELD + "\": {\n" +
            "          \"type\": \"keyword\"\n" +
            "        }," +
            "        \""+ ALIAS_TO_KEYWORD_FIELD + "\": {\n" +
            "          \"type\": \"alias\",\n" +
            "          \"path\": \"" + KEYWORD_FIELD + "\"\n" +
            "        }," +
            "        \""+ ALIAS_TO_NESTED_FIELD + "\": {\n" +
            "          \"type\": \"alias\",\n" +
            "          \"path\": \"" + NESTED_FIELD + "\"\n" +
            "        }" +
            "      }\n" +
            "    }";
        if (isDatastream) {
            try {
                createDataStreamAndTemplate(index, mapping);
            } catch (IOException ex) {
                throw new ElasticsearchException(ex);
            }
        } else {
            client().admin().indices().prepareCreate(index)
                .setMapping(mapping)
                .get();
        }
    }

    private static void indexData(String sourceIndex, int numTrainingRows, int numNonTrainingRows, String dependentVariable) {
        BulkRequestBuilder bulkRequestBuilder = client().prepareBulk()
            .setRefreshPolicy(WriteRequest.RefreshPolicy.IMMEDIATE);
        for (int i = 0; i < numTrainingRows; i++) {
            List<Object> source = List.of(
                "@timestamp", "2020-12-12",
                BOOLEAN_FIELD, BOOLEAN_FIELD_VALUES.get(i % BOOLEAN_FIELD_VALUES.size()),
                NUMERICAL_FIELD, NUMERICAL_FIELD_VALUES.get(i % NUMERICAL_FIELD_VALUES.size()),
                DISCRETE_NUMERICAL_FIELD, DISCRETE_NUMERICAL_FIELD_VALUES.get(i % DISCRETE_NUMERICAL_FIELD_VALUES.size()),
                TEXT_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size()),
                KEYWORD_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size()),
                NESTED_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size()));
            IndexRequest indexRequest = new IndexRequest(sourceIndex).source(source.toArray()).opType(DocWriteRequest.OpType.CREATE);
            bulkRequestBuilder.add(indexRequest);
        }
        for (int i = numTrainingRows; i < numTrainingRows + numNonTrainingRows; i++) {
            List<Object> source = new ArrayList<>();
            if (BOOLEAN_FIELD.equals(dependentVariable) == false) {
                source.addAll(List.of(BOOLEAN_FIELD, BOOLEAN_FIELD_VALUES.get(i % BOOLEAN_FIELD_VALUES.size())));
            }
            if (NUMERICAL_FIELD.equals(dependentVariable) == false) {
                source.addAll(List.of(NUMERICAL_FIELD, NUMERICAL_FIELD_VALUES.get(i % NUMERICAL_FIELD_VALUES.size())));
            }
            if (DISCRETE_NUMERICAL_FIELD.equals(dependentVariable) == false) {
                source.addAll(
                    List.of(DISCRETE_NUMERICAL_FIELD, DISCRETE_NUMERICAL_FIELD_VALUES.get(i % DISCRETE_NUMERICAL_FIELD_VALUES.size())));
            }
            if (TEXT_FIELD.equals(dependentVariable) == false) {
                source.addAll(List.of(TEXT_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size())));
            }
            if (KEYWORD_FIELD.equals(dependentVariable) == false) {
                source.addAll(List.of(KEYWORD_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size())));
            }
            if (NESTED_FIELD.equals(dependentVariable) == false) {
                source.addAll(List.of(NESTED_FIELD, KEYWORD_FIELD_VALUES.get(i % KEYWORD_FIELD_VALUES.size())));
            }
            source.addAll(List.of("@timestamp", "2020-12-12"));
            IndexRequest indexRequest = new IndexRequest(sourceIndex).source(source.toArray()).opType(DocWriteRequest.OpType.CREATE);
            bulkRequestBuilder.add(indexRequest);
        }
        BulkResponse bulkResponse = bulkRequestBuilder.get();
        if (bulkResponse.hasFailures()) {
            fail("Failed to index data: " + bulkResponse.buildFailureMessage());
        }
    }

    private static Map<String, Object> getDestDoc(DataFrameAnalyticsConfig config, SearchHit hit) {
        GetResponse destDocGetResponse = client().prepareGet().setIndex(config.getDest().getIndex()).setId(hit.getId()).get();
        assertThat(destDocGetResponse.isExists(), is(true));
        Map<String, Object> sourceDoc = hit.getSourceAsMap();
        Map<String, Object> destDoc = destDocGetResponse.getSource();
        for (String field : sourceDoc.keySet()) {
            assertThat(destDoc, hasKey(field));
            assertThat(destDoc.get(field), equalTo(sourceDoc.get(field)));
        }
        return destDoc;
    }

    private String stateDocId() {
        return jobId + "_regression_state#1";
    }

    @Override
    boolean supportsInference() {
        return true;
    }
}
