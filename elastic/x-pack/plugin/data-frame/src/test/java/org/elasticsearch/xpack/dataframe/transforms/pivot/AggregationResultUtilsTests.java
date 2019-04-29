/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.dataframe.transforms.pivot;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ContextParser;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.script.Script;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.AggregationBuilder;
import org.elasticsearch.search.aggregations.AggregationBuilders;
import org.elasticsearch.search.aggregations.PipelineAggregationBuilder;
import org.elasticsearch.search.aggregations.PipelineAggregatorBuilders;
import org.elasticsearch.search.aggregations.bucket.composite.CompositeAggregation;
import org.elasticsearch.search.aggregations.bucket.composite.ParsedComposite;
import org.elasticsearch.search.aggregations.bucket.terms.DoubleTerms;
import org.elasticsearch.search.aggregations.bucket.terms.LongTerms;
import org.elasticsearch.search.aggregations.bucket.terms.ParsedDoubleTerms;
import org.elasticsearch.search.aggregations.bucket.terms.ParsedLongTerms;
import org.elasticsearch.search.aggregations.bucket.terms.ParsedStringTerms;
import org.elasticsearch.search.aggregations.bucket.terms.StringTerms;
import org.elasticsearch.search.aggregations.metrics.AvgAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.CardinalityAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.ExtendedStatsAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.MaxAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.MinAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.ParsedAvg;
import org.elasticsearch.search.aggregations.metrics.ParsedCardinality;
import org.elasticsearch.search.aggregations.metrics.ParsedExtendedStats;
import org.elasticsearch.search.aggregations.metrics.ParsedMax;
import org.elasticsearch.search.aggregations.metrics.ParsedMin;
import org.elasticsearch.search.aggregations.metrics.ParsedScriptedMetric;
import org.elasticsearch.search.aggregations.metrics.ParsedStats;
import org.elasticsearch.search.aggregations.metrics.ParsedSum;
import org.elasticsearch.search.aggregations.metrics.ParsedValueCount;
import org.elasticsearch.search.aggregations.metrics.ScriptedMetricAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.StatsAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.SumAggregationBuilder;
import org.elasticsearch.search.aggregations.metrics.ValueCountAggregationBuilder;
import org.elasticsearch.search.aggregations.pipeline.BucketScriptPipelineAggregationBuilder;
import org.elasticsearch.search.aggregations.pipeline.ParsedSimpleValue;
import org.elasticsearch.search.aggregations.pipeline.ParsedStatsBucket;
import org.elasticsearch.search.aggregations.pipeline.StatsBucketPipelineAggregationBuilder;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.dataframe.DataFrameField;
import org.elasticsearch.xpack.core.dataframe.transforms.DataFrameIndexerTransformStats;
import org.elasticsearch.xpack.core.dataframe.transforms.pivot.GroupConfig;

import java.io.IOException;
import java.util.Collection;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Collectors;

import static java.util.Arrays.asList;

public class AggregationResultUtilsTests extends ESTestCase {

    private final NamedXContentRegistry namedXContentRegistry = new NamedXContentRegistry(namedXContents);

    private final String KEY = Aggregation.CommonFields.KEY.getPreferredName();
    private final String DOC_COUNT = Aggregation.CommonFields.DOC_COUNT.getPreferredName();

    // aggregations potentially useful for writing tests, to be expanded as necessary
    private static final List<NamedXContentRegistry.Entry> namedXContents;
    static {
        Map<String, ContextParser<Object, ? extends Aggregation>> map = new HashMap<>();
        map.put(CardinalityAggregationBuilder.NAME, (p, c) -> ParsedCardinality.fromXContent(p, (String) c));
        map.put(MinAggregationBuilder.NAME, (p, c) -> ParsedMin.fromXContent(p, (String) c));
        map.put(MaxAggregationBuilder.NAME, (p, c) -> ParsedMax.fromXContent(p, (String) c));
        map.put(SumAggregationBuilder.NAME, (p, c) -> ParsedSum.fromXContent(p, (String) c));
        map.put(AvgAggregationBuilder.NAME, (p, c) -> ParsedAvg.fromXContent(p, (String) c));
        map.put(BucketScriptPipelineAggregationBuilder.NAME, (p, c) -> ParsedSimpleValue.fromXContent(p, (String) c));
        map.put(ScriptedMetricAggregationBuilder.NAME, (p, c) -> ParsedScriptedMetric.fromXContent(p, (String) c));
        map.put(ValueCountAggregationBuilder.NAME, (p, c) -> ParsedValueCount.fromXContent(p, (String) c));
        map.put(StatsAggregationBuilder.NAME, (p, c) -> ParsedStats.fromXContent(p, (String) c));
        map.put(StatsBucketPipelineAggregationBuilder.NAME, (p, c) -> ParsedStatsBucket.fromXContent(p, (String) c));
        map.put(ExtendedStatsAggregationBuilder.NAME, (p, c) -> ParsedExtendedStats.fromXContent(p, (String) c));
        map.put(StringTerms.NAME, (p, c) -> ParsedStringTerms.fromXContent(p, (String) c));
        map.put(LongTerms.NAME, (p, c) -> ParsedLongTerms.fromXContent(p, (String) c));
        map.put(DoubleTerms.NAME, (p, c) -> ParsedDoubleTerms.fromXContent(p, (String) c));

        namedXContents = map.entrySet().stream()
                .map(entry -> new NamedXContentRegistry.Entry(Aggregation.class, new ParseField(entry.getKey()), entry.getValue()))
                .collect(Collectors.toList());
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        return namedXContentRegistry;
    }

    public void testExtractCompositeAggregationResults() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);

        GroupConfig groupBy = parseGroupConfig("{ \"" + targetField + "\" : {"
                + "\"terms\" : {"
                + "   \"field\" : \"doesn't_matter_for_this_test\""
                + "} } }");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;
        Collection<AggregationBuilder> aggregationBuilders = Collections.singletonList(AggregationBuilders.avg(aggName));

        Map<String, Object> input = asMap(
                "buckets",
                    asList(
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID1"),
                                  aggTypedName, asMap(
                                          "value", 42.33),
                                  DOC_COUNT, 8),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID2"),
                                  aggTypedName, asMap(
                                          "value", 28.99),
                                  DOC_COUNT, 3),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID3"),
                                  aggTypedName, asMap(
                                          "value", 12.55),
                                  DOC_COUNT, 9)
                    ));

        List<Map<String, Object>> expected = asList(
                asMap(
                        targetField, "ID1",
                        aggName, 42.33
                        ),
                asMap(
                        targetField, "ID2",
                        aggName, 28.99
                        ),
                asMap(
                        targetField, "ID3",
                        aggName, 12.55
                        )
                );
        Map<String, String> fieldTypeMap = asStringMap(
            targetField, "keyword",
            aggName, "double"
        );
        executeTest(groupBy, aggregationBuilders, Collections.emptyList(), input, fieldTypeMap, expected, 20);
    }

    public void testExtractCompositeAggregationResultsMultipleGroups() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);
        String targetField2 = randomAlphaOfLengthBetween(5, 10) + "_2";

        GroupConfig groupBy = parseGroupConfig("{"
                + "\"" + targetField + "\" : {"
                + "  \"terms\" : {"
                + "     \"field\" : \"doesn't_matter_for_this_test\""
                + "  } },"
                + "\"" + targetField2 + "\" : {"
                + "  \"terms\" : {"
                + "     \"field\" : \"doesn't_matter_for_this_test\""
                + "  } }"
                + "}");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;
        Collection<AggregationBuilder> aggregationBuilders = Collections.singletonList(AggregationBuilders.avg(aggName));

        Map<String, Object> input = asMap(
                "buckets",
                    asList(
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID1",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 42.33),
                                  DOC_COUNT, 1),
                            asMap(
                                    KEY, asMap(
                                            targetField, "ID1",
                                            targetField2, "ID2_2"
                                            ),
                                    aggTypedName, asMap(
                                            "value", 8.4),
                                    DOC_COUNT, 2),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID2",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 28.99),
                                  DOC_COUNT, 3),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID3",
                                          targetField2, "ID2_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 12.55),
                                  DOC_COUNT, 4)
                    ));

        List<Map<String, Object>> expected = asList(
                asMap(
                        targetField, "ID1",
                        targetField2, "ID1_2",
                        aggName, 42.33
                        ),
                asMap(
                        targetField, "ID1",
                        targetField2, "ID2_2",
                        aggName, 8.4
                        ),
                asMap(
                        targetField, "ID2",
                        targetField2, "ID1_2",
                        aggName, 28.99
                        ),
                asMap(
                        targetField, "ID3",
                        targetField2, "ID2_2",
                        aggName, 12.55
                        )
                );
        Map<String, String> fieldTypeMap = asStringMap(
            aggName, "double",
            targetField, "keyword",
            targetField2, "keyword"
        );
        executeTest(groupBy, aggregationBuilders, Collections.emptyList(), input, fieldTypeMap, expected, 10);
    }

    public void testExtractCompositeAggregationResultsMultiAggregations() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);

        GroupConfig groupBy = parseGroupConfig("{\"" + targetField + "\" : {"
                + "\"terms\" : {"
                + "   \"field\" : \"doesn't_matter_for_this_test\""
                + "} } }");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;

        String aggName2 = randomAlphaOfLengthBetween(5, 10) + "_2";
        String aggTypedName2 = "max#" + aggName2;

        Collection<AggregationBuilder> aggregationBuilders = asList(AggregationBuilders.avg(aggName), AggregationBuilders.max(aggName2));

        Map<String, Object> input = asMap(
                "buckets",
                    asList(
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID1"),
                                  aggTypedName, asMap(
                                          "value", 42.33),
                                  aggTypedName2, asMap(
                                          "value", 9.9),
                                  DOC_COUNT, 111),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID2"),
                                  aggTypedName, asMap(
                                          "value", 28.99),
                                  aggTypedName2, asMap(
                                          "value", 222.33),
                                  DOC_COUNT, 88),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID3"),
                                  aggTypedName, asMap(
                                          "value", 12.55),
                                  aggTypedName2, asMap(
                                          "value", -2.44),
                                  DOC_COUNT, 1)
                    ));

        List<Map<String, Object>> expected = asList(
                asMap(
                        targetField, "ID1",
                        aggName, 42.33,
                        aggName2, 9.9
                        ),
                asMap(
                        targetField, "ID2",
                        aggName, 28.99,
                        aggName2, 222.33
                        ),
                asMap(
                        targetField, "ID3",
                        aggName, 12.55,
                        aggName2, -2.44
                        )
                );
        Map<String, String> fieldTypeMap = asStringMap(
            targetField, "keyword",
            aggName, "double",
            aggName2, "double"
        );
        executeTest(groupBy, aggregationBuilders, Collections.emptyList(), input, fieldTypeMap, expected, 200);
    }

    public void testExtractCompositeAggregationResultsMultiAggregationsAndTypes() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);
        String targetField2 = randomAlphaOfLengthBetween(5, 10) + "_2";

        GroupConfig groupBy = parseGroupConfig("{"
            + "\"" + targetField + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } },"
            + "\"" + targetField2 + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } }"
            + "}");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;

        String aggName2 = randomAlphaOfLengthBetween(5, 10) + "_2";
        String aggTypedName2 = "max#" + aggName2;

        Collection<AggregationBuilder> aggregationBuilders = asList(AggregationBuilders.avg(aggName), AggregationBuilders.max(aggName2));

        Map<String, Object> input = asMap(
            "buckets",
            asList(
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", 42.33),
                    aggTypedName2, asMap(
                        "value", 9.9,
                        "value_as_string", "9.9F"),
                    DOC_COUNT, 1),
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value", 8.4),
                    aggTypedName2, asMap(
                        "value", 222.33,
                        "value_as_string", "222.33F"),
                    DOC_COUNT, 2),
                asMap(
                    KEY, asMap(
                        targetField, "ID2",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", 28.99),
                    aggTypedName2, asMap(
                        "value", -2.44,
                        "value_as_string", "-2.44F"),
                    DOC_COUNT, 3),
                asMap(
                    KEY, asMap(
                        targetField, "ID3",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value", 12.55),
                    aggTypedName2, asMap(
                        "value", -100.44,
                        "value_as_string", "-100.44F"),
                    DOC_COUNT, 4)
            ));

        List<Map<String, Object>> expected = asList(
            asMap(
                targetField, "ID1",
                targetField2, "ID1_2",
                aggName, 42.33,
                aggName2, "9.9F"
            ),
            asMap(
                targetField, "ID1",
                targetField2, "ID2_2",
                aggName, 8.4,
                aggName2, "222.33F"
            ),
            asMap(
                targetField, "ID2",
                targetField2, "ID1_2",
                aggName, 28.99,
                aggName2, "-2.44F"
            ),
            asMap(
                targetField, "ID3",
                targetField2, "ID2_2",
                aggName, 12.55,
                aggName2, "-100.44F"
            )
        );
        Map<String, String> fieldTypeMap = asStringMap(
            aggName, "double",
            aggName2, "keyword", // If the second aggregation was some non-numeric mapped field
            targetField, "keyword",
            targetField2, "keyword"
        );
        executeTest(groupBy, aggregationBuilders, Collections.emptyList(), input, fieldTypeMap, expected, 10);
    }

    public void testExtractCompositeAggregationResultsWithDynamicType() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);
        String targetField2 = randomAlphaOfLengthBetween(5, 10) + "_2";

        GroupConfig groupBy = parseGroupConfig("{"
            + "\"" + targetField + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } },"
            + "\"" + targetField2 + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } }"
            + "}");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "scripted_metric#" + aggName;

        Collection<AggregationBuilder> aggregationBuilders = asList(AggregationBuilders.scriptedMetric(aggName));

        Map<String, Object> input = asMap(
            "buckets",
            asList(
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", asMap("field", 123.0)),
                    DOC_COUNT, 1),
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value", asMap("field", 1.0)),
                    DOC_COUNT, 2),
                asMap(
                    KEY, asMap(
                        targetField, "ID2",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", asMap("field", 2.13)),
                    DOC_COUNT, 3),
                asMap(
                    KEY, asMap(
                        targetField, "ID3",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value", asMap("field", 12.0)),
                    DOC_COUNT, 4)
            ));

        List<Map<String, Object>> expected = asList(
            asMap(
                targetField, "ID1",
                targetField2, "ID1_2",
                aggName,  asMap("field", 123.0)
            ),
            asMap(
                targetField, "ID1",
                targetField2, "ID2_2",
                aggName, asMap("field", 1.0)
            ),
            asMap(
                targetField, "ID2",
                targetField2, "ID1_2",
                aggName, asMap("field", 2.13)
            ),
            asMap(
                targetField, "ID3",
                targetField2, "ID2_2",
                aggName, asMap("field", 12.0)
            )
        );
        Map<String, String> fieldTypeMap = asStringMap(
            targetField, "keyword",
            targetField2, "keyword"
        );
        executeTest(groupBy, aggregationBuilders, Collections.emptyList(), input, fieldTypeMap, expected, 10);
    }

    public void testExtractCompositeAggregationResultsWithPipelineAggregation() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);
        String targetField2 = randomAlphaOfLengthBetween(5, 10) + "_2";

        GroupConfig groupBy = parseGroupConfig("{"
            + "\"" + targetField + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } },"
            + "\"" + targetField2 + "\" : {"
            + "  \"terms\" : {"
            + "     \"field\" : \"doesn't_matter_for_this_test\""
            + "  } }"
            + "}");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;
        String pipelineAggName = randomAlphaOfLengthBetween(5, 10) + "_2";
        String pipelineAggTypedName = "bucket_script#" + pipelineAggName;

        Collection<AggregationBuilder> aggregationBuilders = asList(AggregationBuilders.scriptedMetric(aggName));
        Collection<PipelineAggregationBuilder> pipelineAggregationBuilders =
            asList(PipelineAggregatorBuilders.bucketScript(pipelineAggName,
                Collections.singletonMap("param_1", aggName),
                new Script("return params.param_1")));

        Map<String, Object> input = asMap(
            "buckets",
            asList(
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", 123.0),
                    pipelineAggTypedName, asMap(
                        "value", 123.0),
                    DOC_COUNT, 1),
                asMap(
                    KEY, asMap(
                        targetField, "ID1",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value",  1.0),
                    pipelineAggTypedName, asMap(
                        "value", 1.0),
                    DOC_COUNT, 2),
                asMap(
                    KEY, asMap(
                        targetField, "ID2",
                        targetField2, "ID1_2"
                    ),
                    aggTypedName, asMap(
                        "value", 2.13),
                    pipelineAggTypedName, asMap(
                        "value", 2.13),
                    DOC_COUNT, 3),
                asMap(
                    KEY, asMap(
                        targetField, "ID3",
                        targetField2, "ID2_2"
                    ),
                    aggTypedName, asMap(
                        "value", 12.0),
                    pipelineAggTypedName, asMap(
                        "value", 12.0),
                    DOC_COUNT, 4)
            ));

        List<Map<String, Object>> expected = asList(
            asMap(
                targetField, "ID1",
                targetField2, "ID1_2",
                aggName, 123.0,
                pipelineAggName, 123.0
            ),
            asMap(
                targetField, "ID1",
                targetField2, "ID2_2",
                aggName, 1.0,
                pipelineAggName, 1.0
            ),
            asMap(
                targetField, "ID2",
                targetField2, "ID1_2",
                aggName, 2.13,
                pipelineAggName, 2.13
            ),
            asMap(
                targetField, "ID3",
                targetField2, "ID2_2",
                aggName, 12.0,
                pipelineAggName, 12.0
            )
        );
        Map<String, String> fieldTypeMap = asStringMap(
            targetField, "keyword",
            targetField2, "keyword",
            aggName, "double"
        );
        executeTest(groupBy, aggregationBuilders, pipelineAggregationBuilders, input, fieldTypeMap, expected, 10);
    }

    public void testExtractCompositeAggregationResultsDocIDs() throws IOException {
        String targetField = randomAlphaOfLengthBetween(5, 10);
        String targetField2 = randomAlphaOfLengthBetween(5, 10) + "_2";

        GroupConfig groupBy = parseGroupConfig("{"
                + "\"" + targetField + "\" : {"
                + "  \"terms\" : {"
                + "     \"field\" : \"doesn't_matter_for_this_test\""
                + "  } },"
                + "\"" + targetField2 + "\" : {"
                + "  \"terms\" : {"
                + "     \"field\" : \"doesn't_matter_for_this_test\""
                + "  } }"
                + "}");

        String aggName = randomAlphaOfLengthBetween(5, 10);
        String aggTypedName = "avg#" + aggName;
        Collection<AggregationBuilder> aggregationBuilders = Collections.singletonList(AggregationBuilders.avg(aggName));

        Map<String, Object> inputFirstRun = asMap(
                "buckets",
                    asList(
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID1",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 42.33),
                                  DOC_COUNT, 1),
                            asMap(
                                    KEY, asMap(
                                            targetField, "ID1",
                                            targetField2, "ID2_2"
                                            ),
                                    aggTypedName, asMap(
                                            "value", 8.4),
                                    DOC_COUNT, 2),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID2",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 28.99),
                                  DOC_COUNT, 3),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID3",
                                          targetField2, "ID2_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 12.55),
                                  DOC_COUNT, 4)
                    ));

        Map<String, Object> inputSecondRun = asMap(
                "buckets",
                    asList(
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID1",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 433.33),
                                  DOC_COUNT, 12),
                            asMap(
                                    KEY, asMap(
                                            targetField, "ID1",
                                            targetField2, "ID2_2"
                                            ),
                                    aggTypedName, asMap(
                                            "value", 83.4),
                                    DOC_COUNT, 32),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID2",
                                          targetField2, "ID1_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 21.99),
                                  DOC_COUNT, 2),
                            asMap(
                                  KEY, asMap(
                                          targetField, "ID3",
                                          targetField2, "ID2_2"
                                          ),
                                  aggTypedName, asMap(
                                          "value", 122.55),
                                  DOC_COUNT, 44)
                    ));
        DataFrameIndexerTransformStats stats = DataFrameIndexerTransformStats.withDefaultTransformId();

        Map<String, String> fieldTypeMap = asStringMap(
                aggName, "double",
                targetField, "keyword",
                targetField2, "keyword"
            );

        List<Map<String, Object>> resultFirstRun =
            runExtraction(groupBy, aggregationBuilders, Collections.emptyList(), inputFirstRun, fieldTypeMap, stats);
        List<Map<String, Object>> resultSecondRun =
            runExtraction(groupBy, aggregationBuilders, Collections.emptyList(), inputSecondRun, fieldTypeMap, stats);

        assertNotEquals(resultFirstRun, resultSecondRun);

        Set<String> documentIdsFirstRun = new HashSet<>();
        resultFirstRun.forEach(m -> {
            documentIdsFirstRun.add((String) m.get(DataFrameField.DOCUMENT_ID_FIELD));
        });

        assertEquals(4, documentIdsFirstRun.size());

        Set<String> documentIdsSecondRun = new HashSet<>();
        resultSecondRun.forEach(m -> {
            documentIdsSecondRun.add((String) m.get(DataFrameField.DOCUMENT_ID_FIELD));
        });

        assertEquals(4, documentIdsSecondRun.size());
        assertEquals(documentIdsFirstRun, documentIdsSecondRun);
    }

    private void executeTest(GroupConfig groups,
                             Collection<AggregationBuilder> aggregationBuilders,
                             Collection<PipelineAggregationBuilder> pipelineAggregationBuilders,
                             Map<String, Object> input,
                             Map<String, String> fieldTypeMap,
                             List<Map<String, Object>> expected,
                             long expectedDocCounts) throws IOException {
        DataFrameIndexerTransformStats stats = DataFrameIndexerTransformStats.withDefaultTransformId();
        XContentBuilder builder = XContentFactory.contentBuilder(randomFrom(XContentType.values()));
        builder.map(input);

        List<Map<String, Object>> result = runExtraction(groups,
            aggregationBuilders,
            pipelineAggregationBuilders,
            input,
            fieldTypeMap,
            stats);

        // remove the document ids and test uniqueness
        Set<String> documentIds = new HashSet<>();
        result.forEach(m -> {
            documentIds.add((String) m.remove(DataFrameField.DOCUMENT_ID_FIELD));
        });

        assertEquals(result.size(), documentIds.size());
        assertEquals(expected, result);
        assertEquals(expectedDocCounts, stats.getNumDocuments());

    }

    private List<Map<String, Object>> runExtraction(GroupConfig groups,
                                                    Collection<AggregationBuilder> aggregationBuilders,
                                                    Collection<PipelineAggregationBuilder> pipelineAggregationBuilders,
                                                    Map<String, Object> input,
                                                    Map<String, String> fieldTypeMap,
                                                    DataFrameIndexerTransformStats stats) throws IOException {

        XContentBuilder builder = XContentFactory.contentBuilder(randomFrom(XContentType.values()));
        builder.map(input);

        try (XContentParser parser = createParser(builder)) {
            CompositeAggregation agg = ParsedComposite.fromXContent(parser, "my_feature");
            return AggregationResultUtils.extractCompositeAggregationResults(agg,
                groups,
                aggregationBuilders,
                pipelineAggregationBuilders,
                fieldTypeMap,
                stats).collect(Collectors.toList());
        }
    }

    private GroupConfig parseGroupConfig(String json) throws IOException {
        final XContentParser parser = XContentType.JSON.xContent().createParser(xContentRegistry(),
                DeprecationHandler.THROW_UNSUPPORTED_OPERATION, json);
        return GroupConfig.fromXContent(parser, false);
    }

    static Map<String, Object> asMap(Object... fields) {
        assert fields.length % 2 == 0;
        final Map<String, Object> map = new HashMap<>();
        for (int i = 0; i < fields.length; i += 2) {
            String field = (String) fields[i];
            map.put(field, fields[i + 1]);
        }
        return map;
    }

    static Map<String, String> asStringMap(String... strings) {
        assert strings.length % 2 == 0;
        final Map<String, String> map = new HashMap<>();
        for (int i = 0; i < strings.length; i += 2) {
            String field = strings[i];
            map.put(field, strings[i + 1]);
        }
        return map;
    }
}
