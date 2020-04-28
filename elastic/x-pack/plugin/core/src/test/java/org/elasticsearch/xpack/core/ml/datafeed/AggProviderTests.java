/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.ml.datafeed;

import org.elasticsearch.ElasticsearchStatusException;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.DeprecationHandler;
import org.elasticsearch.common.xcontent.NamedXContentRegistry;
import org.elasticsearch.common.xcontent.XContentFactory;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.XContentType;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.search.aggregations.AggregatorFactories;
import org.elasticsearch.test.AbstractSerializingTestCase;
import org.elasticsearch.xpack.core.ml.utils.XContentObjectTransformer;

import java.io.IOException;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;


public class AggProviderTests extends AbstractSerializingTestCase<AggProvider> {

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, Collections.emptyList());
        return new NamedXContentRegistry(searchModule.getNamedXContents());
    }

    @Override
    protected NamedWriteableRegistry writableRegistry() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, Collections.emptyList());
        return new NamedWriteableRegistry(searchModule.getNamedWriteables());
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return writableRegistry();
    }

    @Override
    protected AggProvider createTestInstance() {
        return createRandomValidAggProvider();
    }

    @Override
    protected Writeable.Reader<AggProvider> instanceReader() {
        return AggProvider::fromStream;
    }

    @Override
    protected AggProvider doParseInstance(XContentParser parser) throws IOException {
        return AggProvider.fromXContent(parser, false);
    }

    public static AggProvider createRandomValidAggProvider() {
        return createRandomValidAggProvider(randomAlphaOfLengthBetween(1, 10), randomAlphaOfLengthBetween(1, 10));
    }

    public static AggProvider createRandomValidAggProvider(String name, String field) {
        Map<String, Object> agg = Collections.singletonMap(name,
            Collections.singletonMap("avg", Collections.singletonMap("field", field)));
        try {
            SearchModule searchModule = new SearchModule(Settings.EMPTY, Collections.emptyList());
            AggregatorFactories.Builder aggs =
                XContentObjectTransformer.aggregatorTransformer(new NamedXContentRegistry(searchModule.getNamedXContents()))
                    .fromMap(agg);
            return new AggProvider(agg, aggs, null, false);
        } catch (IOException ex) {
            fail(ex.getMessage());
        }
        return null;
    }

    public void testEmptyAggMap() throws IOException {
        XContentParser parser = XContentFactory.xContent(XContentType.JSON)
            .createParser(xContentRegistry(), DeprecationHandler.THROW_UNSUPPORTED_OPERATION, "{}");
        ElasticsearchStatusException e = expectThrows(ElasticsearchStatusException.class,
            () -> AggProvider.fromXContent(parser, false));
        assertThat(e.status(), equalTo(RestStatus.BAD_REQUEST));
        assertThat(e.getMessage(), equalTo("Datafeed aggregations are not parsable"));
    }

    public void testRewriteBadNumericInterval() {
        long numericInterval = randomNonNegativeLong();
        Map<String, Object> maxTime = Collections.singletonMap("max", Collections.singletonMap("field", "time"));
        Map<String, Object> numericDeprecated = new HashMap<>(){{
            put("interval", numericInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> expected = new HashMap<>() {{
            put("fixed_interval", numericInterval + "ms");
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> deprecated = Collections.singletonMap("buckets", Collections.singletonMap("date_histogram", numericDeprecated));
        assertTrue(AggProvider.rewriteDateHistogramInterval(deprecated, false));
        assertThat(deprecated, equalTo(Collections.singletonMap("buckets", Collections.singletonMap("date_histogram", expected))));

        numericDeprecated = new HashMap<>(){{
            put("interval", numericInterval + "ms");
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        deprecated = Collections.singletonMap("date_histogram", Collections.singletonMap("date_histogram", numericDeprecated));
        assertTrue(AggProvider.rewriteDateHistogramInterval(deprecated, false));
        assertThat(deprecated,
            equalTo(Collections.singletonMap("date_histogram", Collections.singletonMap("date_histogram", expected))));
    }

    public void testRewriteBadCalendarInterval() {
        String calendarInterval = "1w";
        Map<String, Object> maxTime = Collections.singletonMap("max", Collections.singletonMap("field", "time"));
        Map<String, Object> calendarDeprecated = new HashMap<>(){{
            put("interval", calendarInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> expected = new HashMap<>() {{
            put("calendar_interval", calendarInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> deprecated = Collections.singletonMap("buckets",
            Collections.singletonMap("date_histogram", calendarDeprecated));
        assertTrue(AggProvider.rewriteDateHistogramInterval(deprecated, false));
        assertThat(deprecated, equalTo(Collections.singletonMap("buckets", Collections.singletonMap("date_histogram", expected))));

        calendarDeprecated = new HashMap<>(){{
            put("interval", calendarInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        deprecated = Collections.singletonMap("date_histogram", Collections.singletonMap("date_histogram", calendarDeprecated));
        assertTrue(AggProvider.rewriteDateHistogramInterval(deprecated, false));
        assertThat(deprecated,
            equalTo(Collections.singletonMap("date_histogram", Collections.singletonMap("date_histogram", expected))));
    }

    public void testRewriteWhenNoneMustOccur() {
        String calendarInterval = "1w";
        Map<String, Object> maxTime = Collections.singletonMap("max", Collections.singletonMap("field", "time"));
        Map<String, Object> calendarDeprecated = new HashMap<>(){{
            put("calendar_interval", calendarInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> expected = new HashMap<>() {{
            put("calendar_interval", calendarInterval);
            put("field", "foo");
            put("aggs", Collections.singletonMap("time", maxTime));
        }};
        Map<String, Object> current = Collections.singletonMap("buckets", Collections.singletonMap("date_histogram", calendarDeprecated));
        assertFalse(AggProvider.rewriteDateHistogramInterval(current, false));
        assertThat(current,
            equalTo(Collections.singletonMap("buckets", Collections.singletonMap("date_histogram", expected))));
    }

    @Override
    protected AggProvider mutateInstance(AggProvider instance) throws IOException {
        Exception parsingException = instance.getParsingException();
        AggregatorFactories.Builder parsedAggs = instance.getParsedAggs();
        switch (between(0, 1)) {
            case 0:
                parsingException = parsingException == null ? new IOException("failed parsing") : null;
                break;
            case 1:
                parsedAggs = parsedAggs == null ?
                    XContentObjectTransformer.aggregatorTransformer(xContentRegistry()).fromMap(instance.getAggs()) :
                    null;
                break;
            default:
                throw new AssertionError("Illegal randomisation branch");
        }
        return new AggProvider(instance.getAggs(), parsedAggs, parsingException, false);
    }
}
