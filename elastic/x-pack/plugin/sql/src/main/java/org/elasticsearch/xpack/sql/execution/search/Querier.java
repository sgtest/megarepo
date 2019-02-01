/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.execution.search;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import org.apache.lucene.util.PriorityQueue;
import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.ShardSearchFailure;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.Nullable;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.collect.Tuple;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.common.util.CollectionUtils;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.aggregations.Aggregation;
import org.elasticsearch.search.aggregations.Aggregations;
import org.elasticsearch.search.aggregations.bucket.MultiBucketsAggregation.Bucket;
import org.elasticsearch.search.aggregations.bucket.filter.Filters;
import org.elasticsearch.search.builder.SearchSourceBuilder;
import org.elasticsearch.xpack.sql.SqlIllegalArgumentException;
import org.elasticsearch.xpack.sql.execution.PlanExecutor;
import org.elasticsearch.xpack.sql.execution.search.extractor.BucketExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.CompositeKeyExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.ComputingExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.ConstantExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.FieldHitExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.HitExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.MetricAggExtractor;
import org.elasticsearch.xpack.sql.execution.search.extractor.TopHitsAggExtractor;
import org.elasticsearch.xpack.sql.expression.Attribute;
import org.elasticsearch.xpack.sql.expression.ExpressionId;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.AggExtractorInput;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.AggPathInput;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.HitExtractorInput;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.Pipe;
import org.elasticsearch.xpack.sql.expression.gen.pipeline.ReferenceInput;
import org.elasticsearch.xpack.sql.planner.PlanningException;
import org.elasticsearch.xpack.sql.querydsl.agg.Aggs;
import org.elasticsearch.xpack.sql.querydsl.container.ComputedRef;
import org.elasticsearch.xpack.sql.querydsl.container.GlobalCountRef;
import org.elasticsearch.xpack.sql.querydsl.container.GroupByRef;
import org.elasticsearch.xpack.sql.querydsl.container.MetricAggRef;
import org.elasticsearch.xpack.sql.querydsl.container.QueryContainer;
import org.elasticsearch.xpack.sql.querydsl.container.ScriptFieldRef;
import org.elasticsearch.xpack.sql.querydsl.container.SearchHitFieldRef;
import org.elasticsearch.xpack.sql.querydsl.container.TopHitsAggRef;
import org.elasticsearch.xpack.sql.session.Configuration;
import org.elasticsearch.xpack.sql.session.Cursor;
import org.elasticsearch.xpack.sql.session.RowSet;
import org.elasticsearch.xpack.sql.session.Rows;
import org.elasticsearch.xpack.sql.session.SchemaRowSet;
import org.elasticsearch.xpack.sql.session.SqlSession;
import org.elasticsearch.xpack.sql.type.Schema;
import org.elasticsearch.xpack.sql.util.StringUtils;

import java.io.IOException;
import java.util.ArrayList;
import java.util.BitSet;
import java.util.Comparator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;

import static java.util.Collections.singletonList;
// TODO: add retry/back-off
public class Querier {

    private final Logger log = LogManager.getLogger(getClass());

    private final PlanExecutor planExecutor;
    private final Configuration cfg;
    private final TimeValue keepAlive, timeout;
    private final int size;
    private final Client client;
    @Nullable
    private final QueryBuilder filter;

    public Querier(SqlSession sqlSession) {
        this.planExecutor = sqlSession.planExecutor();
        this.client = sqlSession.client();
        this.cfg = sqlSession.configuration();
        this.keepAlive = cfg.requestTimeout();
        this.timeout = cfg.pageTimeout();
        this.filter = cfg.filter();
        this.size = cfg.pageSize();
    }

    public void query(List<Attribute> output, QueryContainer query, String index, ActionListener<SchemaRowSet> listener) {
        // prepare the request
        SearchSourceBuilder sourceBuilder = SourceGenerator.sourceBuilder(query, filter, size);
        // set query timeout
        if (timeout.getSeconds() > 0) {
            sourceBuilder.timeout(timeout);
        }

        if (log.isTraceEnabled()) {
            log.trace("About to execute query {} on {}", StringUtils.toString(sourceBuilder), index);
        }

        SearchRequest search = prepareRequest(client, sourceBuilder, timeout, Strings.commaDelimitedListToStringArray(index));

        @SuppressWarnings("rawtypes")
        List<Tuple<Integer, Comparator>> sortingColumns = query.sortingColumns();
        listener = sortingColumns.isEmpty() ? listener : new LocalAggregationSorterListener(listener, sortingColumns, query.limit());

        ActionListener<SearchResponse> l = null;

        if (query.isAggsOnly()) {
            if (query.aggs().useImplicitGroupBy()) {
                l = new ImplicitGroupActionListener(listener, client, timeout, output, query, search);
            } else {
                l = new CompositeActionListener(listener, client, timeout, output, query, search);
            }
        } else {
            search.scroll(keepAlive);
            l = new ScrollActionListener(listener, client, timeout, output, query);
        }

        client.search(search, l);
    }

    public static SearchRequest prepareRequest(Client client, SearchSourceBuilder source, TimeValue timeout, String... indices) {
        SearchRequest search = client.prepareSearch(indices)
                // always track total hits accurately
                .setTrackTotalHits(true)
                .setAllowPartialSearchResults(false)
                .setSource(source)
                .setTimeout(timeout)
                .request();
            return search;
    }

    /**
     * Listener used for local sorting (typically due to aggregations used inside `ORDER BY`).
     * 
     * This listener consumes the whole result set, sorts it in memory then sends the paginated
     * results back to the client.
     */
    @SuppressWarnings("rawtypes")
    class LocalAggregationSorterListener implements ActionListener<SchemaRowSet> {

        private final ActionListener<SchemaRowSet> listener;

        // keep the top N entries.
        private final PriorityQueue<Tuple<List<?>, Integer>> data;
        private final AtomicInteger counter = new AtomicInteger();
        private volatile Schema schema;

        private static final int MAXIMUM_SIZE = 512;
        private final boolean noLimit;

        LocalAggregationSorterListener(ActionListener<SchemaRowSet> listener, List<Tuple<Integer, Comparator>> sortingColumns, int limit) {
            this.listener = listener;

            int size = MAXIMUM_SIZE;
            if (limit < 0) {
                noLimit = true;
            } else {
                noLimit = false;
                if (limit > MAXIMUM_SIZE) {
                    throw new PlanningException("The maximum LIMIT for aggregate sorting is [{}], received [{}]", limit, MAXIMUM_SIZE);
                } else {
                    size = limit;
                }
            }

            this.data = new PriorityQueue<Tuple<List<?>, Integer>>(size) {

                // compare row based on the received attribute sort
                // if a sort item is not in the list, it is assumed the sorting happened in ES
                // and the results are left as is (by using the row ordering), otherwise it is sorted based on the given criteria.
                //
                // Take for example ORDER BY a, x, b, y
                // a, b - are sorted in ES
                // x, y - need to be sorted client-side
                // sorting on x kicks in, only if the values for a are equal.

                // thanks to @jpountz for the row ordering idea as a way to preserve ordering
                @SuppressWarnings("unchecked")
                @Override
                protected boolean lessThan(Tuple<List<?>, Integer> l, Tuple<List<?>, Integer> r) {
                    for (Tuple<Integer, Comparator> tuple : sortingColumns) {
                        int i = tuple.v1().intValue();
                        Comparator comparator = tuple.v2();

                        Object vl = l.v1().get(i);
                        Object vr = r.v1().get(i);
                        if (comparator != null) {
                            int result = comparator.compare(vl, vr);
                            // if things are equals, move to the next comparator
                            if (result != 0) {
                                return result < 0;
                            }
                        }
                        // no comparator means the existing order needs to be preserved
                        else {
                            // check the values - if they are equal move to the next comparator
                            // otherwise return the row order
                            if (Objects.equals(vl, vr) == false) {
                                return l.v2().compareTo(r.v2()) < 0;
                            }
                        }
                    }
                    // everything is equal, fall-back to the row order
                    return l.v2().compareTo(r.v2()) < 0;
                }
            };
        }

        @Override
        public void onResponse(SchemaRowSet schemaRowSet) {
            schema = schemaRowSet.schema();
            doResponse(schemaRowSet);
        }

        private void doResponse(RowSet rowSet) {
            // 1. consume all pages received
            if (consumeRowSet(rowSet) == false) {
                return;
            }
            Cursor cursor = rowSet.nextPageCursor();
            // 1a. trigger a next call if there's still data
            if (cursor != Cursor.EMPTY) {
                // trigger a next call
                planExecutor.nextPage(cfg, cursor, ActionListener.wrap(this::doResponse, this::onFailure));
                // make sure to bail out afterwards as we'll get called by a different thread
                return;
            }

            // no more data available, the last thread sends the response
            // 2. send the in-memory view to the client
            sendResponse();
        }

        private boolean consumeRowSet(RowSet rowSet) {
            // use a synchronized block for visibility purposes (there's no concurrency)
            ResultRowSet<?> rrs = (ResultRowSet<?>) rowSet;
            synchronized (data) {
                for (boolean hasRows = rrs.hasCurrentRow(); hasRows; hasRows = rrs.advanceRow()) {
                    List<Object> row = new ArrayList<>(rrs.columnCount());
                    rrs.forEachResultColumn(row::add);
                    // if the queue overflows and no limit was specified, bail out
                    if (data.insertWithOverflow(new Tuple<>(row, counter.getAndIncrement())) != null && noLimit) {
                        onFailure(new SqlIllegalArgumentException(
                                "The default limit [{}] for aggregate sorting has been reached; please specify a LIMIT"));
                        return false;
                    }
                }
            }
            return true;
        }

        private void sendResponse() {
            List<List<?>> list = new ArrayList<>(data.size());
            Tuple<List<?>, Integer> pop = null;
            while ((pop = data.pop()) != null) {
                list.add(pop.v1());
            }
            listener.onResponse(new PagingListRowSet(schema, list, schema.size(), cfg.pageSize()));
        }

        @Override
        public void onFailure(Exception e) {
            listener.onFailure(e);
        }
    }

    /**
     * Dedicated listener for implicit/default group-by queries that return only _one_ result.
     */
    static class ImplicitGroupActionListener extends BaseAggActionListener {

        private static List<? extends Bucket> EMPTY_BUCKET = singletonList(new Bucket() {

            @Override
            public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
                throw new SqlIllegalArgumentException("No group-by/aggs defined");
            }

            @Override
            public Object getKey() {
                throw new SqlIllegalArgumentException("No group-by/aggs defined");
            }

            @Override
            public String getKeyAsString() {
                throw new SqlIllegalArgumentException("No group-by/aggs defined");
            }

            @Override
            public long getDocCount() {
                throw new SqlIllegalArgumentException("No group-by/aggs defined");
            }

            @Override
            public Aggregations getAggregations() {
                throw new SqlIllegalArgumentException("No group-by/aggs defined");
            }
        });

        ImplicitGroupActionListener(ActionListener<SchemaRowSet> listener, Client client, TimeValue keepAlive, List<Attribute> output,
                QueryContainer query, SearchRequest request) {
            super(listener, client, keepAlive, output, query, request);
        }

        @Override
        protected void handleResponse(SearchResponse response, ActionListener<SchemaRowSet> listener) {
            Aggregations aggs = response.getAggregations();
            if (aggs != null) {
                Aggregation agg = aggs.get(Aggs.ROOT_GROUP_NAME);
                if (agg instanceof Filters) {
                    handleBuckets(((Filters) agg).getBuckets(), response);
                } else {
                    throw new SqlIllegalArgumentException("Unrecognized root group found; {}", agg.getClass());
                }
            }
            // can happen when only a count is requested which is derived from the response
            else {
                handleBuckets(EMPTY_BUCKET, response);
            }
        }

        private void handleBuckets(List<? extends Bucket> buckets, SearchResponse response) {
            if (buckets.size() == 1) {
                Bucket implicitGroup = buckets.get(0);
                List<BucketExtractor> extractors = initBucketExtractors(response);

                Object[] values = new Object[mask.cardinality()];

                int index = 0;
                for (int i = mask.nextSetBit(0); i >= 0; i = mask.nextSetBit(i + 1)) {
                    values[index++] = extractors.get(i).extract(implicitGroup);
                }
                listener.onResponse(Rows.singleton(schema, values));

            } else if (buckets.isEmpty()) {
                listener.onResponse(Rows.empty(schema));

            } else {
                throw new SqlIllegalArgumentException("Too many groups returned by the implicit group; expected 1, received {}",
                        buckets.size());
            }
        }
    }

        
    /**
     * Dedicated listener for composite aggs/group-by results.
     */
    static class CompositeActionListener extends BaseAggActionListener {

        CompositeActionListener(ActionListener<SchemaRowSet> listener, Client client, TimeValue keepAlive,
                List<Attribute> output, QueryContainer query, SearchRequest request) {
            super(listener, client, keepAlive, output, query, request);
        }


        @Override
        protected void handleResponse(SearchResponse response, ActionListener<SchemaRowSet> listener) {
            // there are some results
            if (response.getAggregations().asList().size() > 0) {

                // retry
                if (CompositeAggregationCursor.shouldRetryDueToEmptyPage(response)) {
                    CompositeAggregationCursor.updateCompositeAfterKey(response, request.source());
                    client.search(request, this);
                    return;
                }

                CompositeAggregationCursor.updateCompositeAfterKey(response, request.source());
                byte[] nextSearch = null;
                try {
                    nextSearch = CompositeAggregationCursor.serializeQuery(request.source());
                } catch (Exception ex) {
                    listener.onFailure(ex);
                    return;
                }

                listener.onResponse(
                        new SchemaCompositeAggsRowSet(schema, initBucketExtractors(response), mask, response, query.limit(),
                                nextSearch,
                                request.indices()));
            }
            // no results
            else {
                listener.onResponse(Rows.empty(schema));
            }
        }
    }

    abstract static class BaseAggActionListener extends BaseActionListener {
        final QueryContainer query;
        final SearchRequest request;
        final BitSet mask;

        BaseAggActionListener(ActionListener<SchemaRowSet> listener, Client client, TimeValue keepAlive, List<Attribute> output,
                QueryContainer query, SearchRequest request) {
            super(listener, client, keepAlive, output);

            this.query = query;
            this.request = request;
            this.mask = query.columnMask(output);
        }

        protected List<BucketExtractor> initBucketExtractors(SearchResponse response) {
            // create response extractors for the first time
            List<Tuple<FieldExtraction, ExpressionId>> refs = query.fields();

            List<BucketExtractor> exts = new ArrayList<>(refs.size());
            ConstantExtractor totalCount = new ConstantExtractor(response.getHits().getTotalHits().value);
            for (Tuple<FieldExtraction, ExpressionId> ref : refs) {
                exts.add(createExtractor(ref.v1(), totalCount));
            }
            return exts;
        }

        private BucketExtractor createExtractor(FieldExtraction ref, BucketExtractor totalCount) {
            if (ref instanceof GroupByRef) {
                GroupByRef r = (GroupByRef) ref;
                return new CompositeKeyExtractor(r.key(), r.property(), r.zoneId());
            }

            if (ref instanceof MetricAggRef) {
                MetricAggRef r = (MetricAggRef) ref;
                return new MetricAggExtractor(r.name(), r.property(), r.innerKey());
            }

            if (ref instanceof TopHitsAggRef) {
                TopHitsAggRef r = (TopHitsAggRef) ref;
                return new TopHitsAggExtractor(r.name(), r.fieldDataType());
            }

            if (ref == GlobalCountRef.INSTANCE) {
                return totalCount;
            }

            if (ref instanceof ComputedRef) {
                Pipe proc = ((ComputedRef) ref).processor();

                // wrap only agg inputs
                proc = proc.transformDown(l -> {
                    BucketExtractor be = createExtractor(l.context(), totalCount);
                    return new AggExtractorInput(l.source(), l.expression(), l.action(), be);
                }, AggPathInput.class);

                return new ComputingExtractor(proc.asProcessor());
            }

            throw new SqlIllegalArgumentException("Unexpected value reference {}", ref.getClass());
        }
    }

    /**
     * Dedicated listener for column retrieval/non-grouped queries (scrolls).
     */
    static class ScrollActionListener extends BaseActionListener {
        private final QueryContainer query;
        private final BitSet mask;

        ScrollActionListener(ActionListener<SchemaRowSet> listener, Client client, TimeValue keepAlive,
                List<Attribute> output, QueryContainer query) {
            super(listener, client, keepAlive, output);
            this.query = query;
            this.mask = query.columnMask(output);
        }

        @Override
        protected void handleResponse(SearchResponse response, ActionListener<SchemaRowSet> listener) {
            SearchHit[] hits = response.getHits().getHits();

            // create response extractors for the first time
            List<Tuple<FieldExtraction, ExpressionId>> refs = query.fields();

            List<HitExtractor> exts = new ArrayList<>(refs.size());
            for (Tuple<FieldExtraction, ExpressionId> ref : refs) {
                exts.add(createExtractor(ref.v1()));
            }

            // there are some results
            if (hits.length > 0) {
                String scrollId = response.getScrollId();
                SchemaSearchHitRowSet hitRowSet = new SchemaSearchHitRowSet(schema, exts, mask, hits, query.limit(), scrollId);
                
                // if there's an id, try to setup next scroll
                if (scrollId != null &&
                        // is all the content already retrieved?
                        (Boolean.TRUE.equals(response.isTerminatedEarly())
                                || response.getHits().getTotalHits().value == hits.length
                                || hitRowSet.isLimitReached())) {
                    // if so, clear the scroll
                    clear(response.getScrollId(), ActionListener.wrap(
                            succeeded -> listener.onResponse(new SchemaSearchHitRowSet(schema, exts, mask, hits, query.limit(), null)),
                            listener::onFailure));
                } else {
                    listener.onResponse(hitRowSet);
                }
            }
            // no hits
            else {
                clear(response.getScrollId(), ActionListener.wrap(succeeded -> listener.onResponse(Rows.empty(schema)),
                        listener::onFailure));
            }
        }

        private HitExtractor createExtractor(FieldExtraction ref) {
            if (ref instanceof SearchHitFieldRef) {
                SearchHitFieldRef f = (SearchHitFieldRef) ref;
                return new FieldHitExtractor(f.name(), f.getDataType(), f.useDocValue(), f.hitName());
            }

            if (ref instanceof ScriptFieldRef) {
                ScriptFieldRef f = (ScriptFieldRef) ref;
                return new FieldHitExtractor(f.name(), null, true);
            }

            if (ref instanceof ComputedRef) {
                Pipe proc = ((ComputedRef) ref).processor();
                // collect hitNames
                Set<String> hitNames = new LinkedHashSet<>();
                proc = proc.transformDown(l -> {
                    HitExtractor he = createExtractor(l.context());
                    hitNames.add(he.hitName());

                    if (hitNames.size() > 1) {
                        throw new SqlIllegalArgumentException("Multi-level nested fields [{}] not supported yet", hitNames);
                    }

                    return new HitExtractorInput(l.source(), l.expression(), he);
                }, ReferenceInput.class);
                String hitName = null;
                if (hitNames.size() == 1) {
                    hitName = hitNames.iterator().next();
                }
                return new ComputingExtractor(proc.asProcessor(), hitName);
            }

            throw new SqlIllegalArgumentException("Unexpected value reference {}", ref.getClass());
        }
    }

    /**
     * Base listener class providing clean-up and exception handling.
     * Handles both scroll queries (scan/scroll) and regular/composite-aggs queries.
     */
    abstract static class BaseActionListener implements ActionListener<SearchResponse> {

        final ActionListener<SchemaRowSet> listener;

        final Client client;
        final TimeValue keepAlive;
        final Schema schema;

        BaseActionListener(ActionListener<SchemaRowSet> listener, Client client, TimeValue keepAlive, List<Attribute> output) {
            this.listener = listener;

            this.client = client;
            this.keepAlive = keepAlive;
            this.schema = Rows.schema(output);
        }

        // TODO: need to handle rejections plus check failures (shard size, etc...)
        @Override
        public void onResponse(final SearchResponse response) {
            try {
                ShardSearchFailure[] failure = response.getShardFailures();
                if (!CollectionUtils.isEmpty(failure)) {
                    cleanup(response, new SqlIllegalArgumentException(failure[0].reason(), failure[0].getCause()));
                } else {
                    handleResponse(response, ActionListener.wrap(listener::onResponse, e -> cleanup(response, e)));
                }
            } catch (Exception ex) {
                cleanup(response, ex);
            }
        }

        protected abstract void handleResponse(SearchResponse response, ActionListener<SchemaRowSet> listener);

        // clean-up the scroll in case of exception
        protected final void cleanup(SearchResponse response, Exception ex) {
            if (response != null && response.getScrollId() != null) {
                client.prepareClearScroll().addScrollId(response.getScrollId())
                    // in case of failure, report the initial exception instead of the one resulting from cleaning the scroll
                    .execute(ActionListener.wrap(r -> listener.onFailure(ex), e -> {
                        ex.addSuppressed(e);
                        listener.onFailure(ex);
                    }));
            } else {
                listener.onFailure(ex);
            }
        }

        protected final void clear(String scrollId, ActionListener<Boolean> listener) {
            if (scrollId != null) {
                client.prepareClearScroll().addScrollId(scrollId).execute(
                        ActionListener.wrap(
                                clearScrollResponse -> listener.onResponse(clearScrollResponse.isSucceeded()),
                                listener::onFailure));
            } else {
                listener.onResponse(false);
            }
        }

        @Override
        public final void onFailure(Exception ex) {
            listener.onFailure(ex);
        }
    }
}
