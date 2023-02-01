/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.vectors;

import org.apache.lucene.search.Query;
import org.elasticsearch.TransportVersion;
import org.elasticsearch.action.support.PlainActionFuture;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.query.AbstractQueryBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.index.query.QueryBuilders;
import org.elasticsearch.index.query.QueryRewriteContext;
import org.elasticsearch.index.query.Rewriteable;
import org.elasticsearch.index.query.SearchExecutionContext;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.AbstractXContentSerializingTestCase;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentBuilder;
import org.elasticsearch.xcontent.XContentParser;
import org.junit.Before;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

import static java.util.Collections.emptyList;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.nullValue;

public class KnnSearchBuilderTests extends AbstractXContentSerializingTestCase<KnnSearchBuilder> {
    private NamedWriteableRegistry namedWriteableRegistry;
    private NamedXContentRegistry namedXContentRegistry;

    public static KnnSearchBuilder randomTestInstance() {
        String field = randomAlphaOfLength(6);
        int dim = randomIntBetween(2, 30);
        float[] vector = randomVector(dim);
        int k = randomIntBetween(1, 100);
        int numCands = randomIntBetween(k + 20, 1000);

        KnnSearchBuilder builder = new KnnSearchBuilder(field, vector, k, numCands);
        if (randomBoolean()) {
            builder.boost(randomFloat());
        }

        int numFilters = randomIntBetween(0, 3);
        for (int i = 0; i < numFilters; i++) {
            builder.addFilterQuery(QueryBuilders.termQuery(randomAlphaOfLength(5), randomAlphaOfLength(10)));
        }

        return builder;
    }

    @Before
    public void registerNamedXContents() {
        SearchModule searchModule = new SearchModule(Settings.EMPTY, emptyList());
        namedXContentRegistry = new NamedXContentRegistry(searchModule.getNamedXContents());
        namedWriteableRegistry = new NamedWriteableRegistry(searchModule.getNamedWriteables());
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        return namedXContentRegistry;
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return namedWriteableRegistry;
    }

    @Override
    protected KnnSearchBuilder doParseInstance(XContentParser parser) throws IOException {
        return KnnSearchBuilder.fromXContent(parser);
    }

    @Override
    protected Writeable.Reader<KnnSearchBuilder> instanceReader() {
        return KnnSearchBuilder::new;
    }

    @Override
    protected KnnSearchBuilder createTestInstance() {
        return randomTestInstance();
    }

    @Override
    protected KnnSearchBuilder mutateInstance(KnnSearchBuilder instance) {
        switch (random().nextInt(6)) {

            case 0:
                String newField = randomValueOtherThan(instance.field, () -> randomAlphaOfLength(5));
                return new KnnSearchBuilder(newField, instance.queryVector, instance.k, instance.numCands + 3).boost(instance.boost);
            case 1:
                float[] newVector = randomValueOtherThan(instance.queryVector, () -> randomVector(5));
                return new KnnSearchBuilder(instance.field, newVector, instance.k + 3, instance.numCands).boost(instance.boost);
            case 2:
                return new KnnSearchBuilder(instance.field, instance.queryVector, instance.k + 3, instance.numCands).boost(instance.boost);
            case 3:
                return new KnnSearchBuilder(instance.field, instance.queryVector, instance.k, instance.numCands + 3).boost(instance.boost);
            case 4:
                return new KnnSearchBuilder(instance.field, instance.queryVector, instance.k, instance.numCands).addFilterQueries(
                    instance.filterQueries
                ).addFilterQuery(QueryBuilders.termQuery("new_field", "new-value")).boost(instance.boost);
            case 5:
                float newBoost = randomValueOtherThan(instance.boost, ESTestCase::randomFloat);
                return new KnnSearchBuilder(instance.field, instance.queryVector, instance.k, instance.numCands).addFilterQueries(
                    instance.filterQueries
                ).boost(newBoost);
            default:
                throw new IllegalStateException();
        }
    }

    public void testToQueryBuilder() {
        String field = randomAlphaOfLength(6);
        float[] vector = randomVector(randomIntBetween(2, 30));
        int k = randomIntBetween(1, 100);
        int numCands = randomIntBetween(k, 1000);
        KnnSearchBuilder builder = new KnnSearchBuilder(field, vector, k, numCands);

        float boost = AbstractQueryBuilder.DEFAULT_BOOST;
        if (randomBoolean()) {
            boost = randomFloat();
            builder.boost(boost);
        }

        int numFilters = random().nextInt(3);
        List<QueryBuilder> filterQueries = new ArrayList<>();
        for (int i = 0; i < numFilters; i++) {
            QueryBuilder filter = QueryBuilders.termQuery(randomAlphaOfLength(5), randomAlphaOfLength(5));
            filterQueries.add(filter);
            builder.addFilterQuery(filter);
        }

        QueryBuilder expected = new KnnVectorQueryBuilder(field, vector, numCands).addFilterQueries(filterQueries).boost(boost);
        assertEquals(expected, builder.toQueryBuilder());
    }

    public void testNumCandsLessThanK() {
        IllegalArgumentException e = expectThrows(
            IllegalArgumentException.class,
            () -> new KnnSearchBuilder("field", randomVector(3), 50, 10)
        );
        assertThat(e.getMessage(), containsString("[num_candidates] cannot be less than [k]"));
    }

    public void testNumCandsExceedsLimit() {
        IllegalArgumentException e = expectThrows(
            IllegalArgumentException.class,
            () -> new KnnSearchBuilder("field", randomVector(3), 100, 10002)
        );
        assertThat(e.getMessage(), containsString("[num_candidates] cannot exceed [10000]"));
    }

    public void testInvalidK() {
        IllegalArgumentException e = expectThrows(
            IllegalArgumentException.class,
            () -> new KnnSearchBuilder("field", randomVector(3), 0, 100)
        );
        assertThat(e.getMessage(), containsString("[k] must be greater than 0"));
    }

    public void testRewrite() throws Exception {
        float[] expectedArray = randomVector(randomIntBetween(10, 1024));
        KnnSearchBuilder searchBuilder = new KnnSearchBuilder(
            "field",
            new TestQueryVectorBuilderPlugin.TestQueryVectorBuilder(expectedArray),
            5,
            10
        );
        searchBuilder.boost(randomFloat());
        searchBuilder.addFilterQueries(List.of(new RewriteableQuery()));

        QueryRewriteContext context = new QueryRewriteContext(null, null, null, null);
        PlainActionFuture<KnnSearchBuilder> future = new PlainActionFuture<>();
        Rewriteable.rewriteAndFetch(searchBuilder, context, future);
        KnnSearchBuilder rewritten = future.get();

        assertThat(rewritten.field, equalTo(searchBuilder.field));
        assertThat(rewritten.boost, equalTo(searchBuilder.boost));
        assertThat(rewritten.queryVector, equalTo(expectedArray));
        assertThat(rewritten.queryVectorBuilder, nullValue());
        assertThat(rewritten.filterQueries, hasSize(1));
        assertThat(((RewriteableQuery) rewritten.filterQueries.get(0)).rewrites, equalTo(1));
    }

    static float[] randomVector(int dim) {
        float[] vector = new float[dim];
        for (int i = 0; i < vector.length; i++) {
            vector[i] = randomFloat();
        }
        return vector;
    }

    private static class RewriteableQuery extends AbstractQueryBuilder<RewriteableQuery> {
        private int rewrites;

        @Override
        public String getWriteableName() {
            throw new UnsupportedOperationException();
        }

        @Override
        public TransportVersion getMinimalSupportedVersion() {
            throw new UnsupportedOperationException();
        }

        @Override
        protected void doWriteTo(StreamOutput out) {
            throw new UnsupportedOperationException();
        }

        @Override
        protected void doXContent(XContentBuilder builder, Params params) throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        protected Query doToQuery(SearchExecutionContext context) throws IOException {
            throw new UnsupportedOperationException();
        }

        @Override
        protected boolean doEquals(RewriteableQuery other) {
            return true;
        }

        @Override
        protected int doHashCode() {
            return Objects.hashCode(RewriteableQuery.class);
        }

        @Override
        protected QueryBuilder doRewrite(QueryRewriteContext queryRewriteContext) {
            rewrites++;
            return this;
        }
    }
}
