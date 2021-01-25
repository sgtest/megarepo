package org.elasticsearch.xpack.transform.integration.continuous;

import org.apache.lucene.util.LuceneTestCase.AwaitsFix;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.support.IndicesOptions;
import org.elasticsearch.client.transform.transforms.DestConfig;
import org.elasticsearch.client.transform.transforms.SourceConfig;
import org.elasticsearch.client.transform.transforms.TransformConfig;
import org.elasticsearch.client.transform.transforms.pivot.GroupConfig;
import org.elasticsearch.client.transform.transforms.pivot.HistogramGroupSource;
import org.elasticsearch.client.transform.transforms.pivot.PivotConfig;
import org.elasticsearch.common.xcontent.support.XContentMapValues;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.search.aggregations.BucketOrder;
import org.elasticsearch.search.aggregations.bucket.histogram.Histogram;
import org.elasticsearch.search.aggregations.bucket.histogram.Histogram.Bucket;
import org.elasticsearch.search.aggregations.bucket.histogram.HistogramAggregationBuilder;
import org.elasticsearch.search.builder.SearchSourceBuilder;

import java.io.IOException;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.hamcrest.Matchers.equalTo;

@AwaitsFix(bugUrl="https://github.com/elastic/elasticsearch/issues/67887") 
public class HistogramGroupByIT extends ContinuousTestCase {
    private static final String NAME = "continuous-histogram-pivot-test";

    private final String metricField;

    public HistogramGroupByIT() {
        metricField = randomFrom(METRIC_FIELDS);
    }

    @Override
    public String getName() {
        return NAME;
    }

    @Override
    public TransformConfig createConfig() {
        TransformConfig.Builder transformConfigBuilder = new TransformConfig.Builder();
        addCommonBuilderParameters(transformConfigBuilder);
        transformConfigBuilder.setSource(new SourceConfig(CONTINUOUS_EVENTS_SOURCE_INDEX));
        transformConfigBuilder.setDest(new DestConfig(NAME, INGEST_PIPELINE));
        transformConfigBuilder.setId(NAME);
        PivotConfig.Builder pivotConfigBuilder = new PivotConfig.Builder();
        pivotConfigBuilder.setGroups(
            new GroupConfig.Builder().groupBy("metric", new HistogramGroupSource.Builder().setField(metricField).setInterval(50.0).build())
                .build()
        );
        AggregatorFactories.Builder aggregations = new AggregatorFactories.Builder();
        addCommonAggregations(aggregations);

        pivotConfigBuilder.setAggregations(aggregations);
        transformConfigBuilder.setPivotConfig(pivotConfigBuilder.build());
        return transformConfigBuilder.build();
    }

    @Override
    public void testIteration(int iteration, Set<String> modifiedEvents) throws IOException {
        SearchRequest searchRequestSource = new SearchRequest(CONTINUOUS_EVENTS_SOURCE_INDEX).allowPartialSearchResults(false)
            .indicesOptions(IndicesOptions.LENIENT_EXPAND_OPEN);
        SearchSourceBuilder sourceBuilderSource = new SearchSourceBuilder().size(0);
        HistogramAggregationBuilder metricBuckets = new HistogramAggregationBuilder("metric").field(metricField)
            .interval(50.0)
            .order(BucketOrder.key(true));
        sourceBuilderSource.aggregation(metricBuckets);
        searchRequestSource.source(sourceBuilderSource);
        SearchResponse responseSource = search(searchRequestSource);

        SearchRequest searchRequestDest = new SearchRequest(NAME).allowPartialSearchResults(false)
            .indicesOptions(IndicesOptions.LENIENT_EXPAND_OPEN);
        SearchSourceBuilder sourceBuilderDest = new SearchSourceBuilder().size(10000).sort("metric");
        searchRequestDest.source(sourceBuilderDest);
        SearchResponse responseDest = search(searchRequestDest);

        List<? extends Bucket> buckets = ((Histogram) responseSource.getAggregations().get("metric")).getBuckets();

        Iterator<? extends Bucket> sourceIterator = buckets.iterator();
        Iterator<SearchHit> destIterator = responseDest.getHits().iterator();

        while (sourceIterator.hasNext() && destIterator.hasNext()) {
            Bucket bucket = sourceIterator.next();
            SearchHit searchHit = destIterator.next();
            Map<String, Object> source = searchHit.getSourceAsMap();

            Long transformBucketKey = ((Integer) XContentMapValues.extractValue("metric", source)).longValue();

            // aggs return buckets with 0 doc_count while composite aggs skip over them
            while (bucket.getDocCount() == 0L) {
                assertTrue(sourceIterator.hasNext());
                bucket = sourceIterator.next();
            }
            long bucketKey = ((Double) bucket.getKey()).longValue();

            // test correctness, the results from the aggregation and the results from the transform should be the same
            assertThat(
                "Buckets did not match, source: " + source + ", expected: " + bucketKey + ", iteration: " + iteration,
                transformBucketKey,
                equalTo(bucketKey)
            );
            assertThat(
                "Doc count did not match, source: " + source + ", expected: " + bucket.getDocCount() + ", iteration: " + iteration,
                ((Integer) XContentMapValues.extractValue("count", source)).longValue(),
                equalTo(bucket.getDocCount())
            );

            // TODO: gh#63801 transform is not optimized for histogram it, it should only rewrite documents that require it
        }

        assertFalse(sourceIterator.hasNext());
        assertFalse(destIterator.hasNext());
    }

}
