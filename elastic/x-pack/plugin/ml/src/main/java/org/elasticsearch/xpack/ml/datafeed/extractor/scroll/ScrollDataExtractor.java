/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.ml.datafeed.extractor.scroll;

import org.apache.logging.log4j.Logger;
import org.elasticsearch.action.search.ClearScrollAction;
import org.elasticsearch.action.search.ClearScrollRequest;
import org.elasticsearch.action.search.SearchAction;
import org.elasticsearch.action.search.SearchPhaseExecutionException;
import org.elasticsearch.action.search.SearchRequestBuilder;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchScrollAction;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.logging.Loggers;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.script.Script;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.search.fetch.StoredFieldsContext;
import org.elasticsearch.search.sort.SortOrder;
import org.elasticsearch.xpack.core.ml.MlClientHelper;
import org.elasticsearch.xpack.core.ml.datafeed.extractor.DataExtractor;
import org.elasticsearch.xpack.core.ml.datafeed.extractor.ExtractorUtils;
import org.elasticsearch.xpack.ml.utils.DomainSplitFunction;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.HashMap;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Objects;
import java.util.Optional;
import java.util.concurrent.TimeUnit;

/**
 * An implementation that extracts data from elasticsearch using search and scroll on a client.
 * It supports safe and responsive cancellation by continuing the scroll until a new timestamp
 * is seen.
 * Note that this class is NOT thread-safe.
 */
class ScrollDataExtractor implements DataExtractor {

    private static final Logger LOGGER = Loggers.getLogger(ScrollDataExtractor.class);
    private static final TimeValue SCROLL_TIMEOUT = new TimeValue(30, TimeUnit.MINUTES);

    private final Client client;
    private final ScrollDataExtractorContext context;
    private String scrollId;
    private boolean isCancelled;
    private boolean hasNext;
    private Long timestampOnCancel;
    protected Long lastTimestamp;
    private boolean searchHasShardFailure;

    ScrollDataExtractor(Client client, ScrollDataExtractorContext dataExtractorContext) {
        this.client = Objects.requireNonNull(client);
        context = Objects.requireNonNull(dataExtractorContext);
        hasNext = true;
        searchHasShardFailure = false;
    }

    @Override
    public boolean hasNext() {
        return hasNext;
    }

    @Override
    public boolean isCancelled() {
        return isCancelled;
    }

    @Override
    public void cancel() {
        LOGGER.trace("[{}] Data extractor received cancel request", context.jobId);
        isCancelled = true;
    }

    @Override
    public Optional<InputStream> next() throws IOException {
        if (!hasNext()) {
            throw new NoSuchElementException();
        }
        Optional<InputStream> stream = scrollId == null ?
                Optional.ofNullable(initScroll(context.start)) : Optional.ofNullable(continueScroll());
        if (!stream.isPresent()) {
            hasNext = false;
        }
        return stream;
    }

    protected InputStream initScroll(long startTimestamp) throws IOException {
        LOGGER.debug("[{}] Initializing scroll", context.jobId);
        SearchResponse searchResponse = executeSearchRequest(buildSearchRequest(startTimestamp));
        LOGGER.debug("[{}] Search response was obtained", context.jobId);
        return processSearchResponse(searchResponse);
    }

    protected SearchResponse executeSearchRequest(SearchRequestBuilder searchRequestBuilder) {
        return MlClientHelper.execute(context.headers, client, searchRequestBuilder::get);
    }

    private SearchRequestBuilder buildSearchRequest(long start) {
        SearchRequestBuilder searchRequestBuilder = SearchAction.INSTANCE.newRequestBuilder(client)
                .setScroll(SCROLL_TIMEOUT)
                .addSort(context.extractedFields.timeField(), SortOrder.ASC)
                .setIndices(context.indices)
                .setTypes(context.types)
                .setSize(context.scrollSize)
                .setQuery(ExtractorUtils.wrapInTimeRangeQuery(
                        context.query, context.extractedFields.timeField(), start, context.end));

        for (String docValueField : context.extractedFields.getDocValueFields()) {
            searchRequestBuilder.addDocValueField(docValueField);
        }
        String[] sourceFields = context.extractedFields.getSourceFields();
        if (sourceFields.length == 0) {
            searchRequestBuilder.setFetchSource(false);
            searchRequestBuilder.storedFields(StoredFieldsContext._NONE_);
        } else {
            searchRequestBuilder.setFetchSource(sourceFields, null);
        }
        context.scriptFields.forEach(f -> searchRequestBuilder.addScriptField(
                f.fieldName(), injectDomainSplit(f.script())));
        return searchRequestBuilder;
    }

    private Script injectDomainSplit(Script script) {
        String code = script.getIdOrCode();
        if (code.contains("domainSplit(") && script.getLang().equals("painless")) {
            String modifiedCode = DomainSplitFunction.function + code;
            Map<String, Object> modifiedParams = new HashMap<>(script.getParams().size()
                    + DomainSplitFunction.params.size());

            modifiedParams.putAll(script.getParams());
            modifiedParams.putAll(DomainSplitFunction.params);

            return new Script(script.getType(), script.getLang(), modifiedCode, modifiedParams);
        }
        return script;
    }

    private InputStream processSearchResponse(SearchResponse searchResponse) throws IOException {

        if (searchResponse.getFailedShards() > 0 && searchHasShardFailure == false) {
            LOGGER.debug("[{}] Resetting scroll search after shard failure", context.jobId);
            markScrollAsErrored();
            return initScroll(lastTimestamp == null ? context.start : lastTimestamp);
        }

        ExtractorUtils.checkSearchWasSuccessful(context.jobId, searchResponse);
        scrollId = searchResponse.getScrollId();
        if (searchResponse.getHits().getHits().length == 0) {
            hasNext = false;
            clearScroll(scrollId);
            return null;
        }

        ByteArrayOutputStream outputStream = new ByteArrayOutputStream();
        try (SearchHitToJsonProcessor hitProcessor = new SearchHitToJsonProcessor(context.extractedFields, outputStream)) {
            for (SearchHit hit : searchResponse.getHits().getHits()) {
                if (isCancelled) {
                    Long timestamp = context.extractedFields.timeFieldValue(hit);
                    if (timestamp != null) {
                        if (timestampOnCancel == null) {
                            timestampOnCancel = timestamp;
                        } else if (timestamp.equals(timestampOnCancel) == false) {
                            hasNext = false;
                            clearScroll(scrollId);
                            break;
                        }
                    }
                }
                hitProcessor.process(hit);
            }
            SearchHit lastHit = searchResponse.getHits().getHits()[searchResponse.getHits().getHits().length -1];
            lastTimestamp = context.extractedFields.timeFieldValue(lastHit);
        }
        return new ByteArrayInputStream(outputStream.toByteArray());
    }

    private InputStream continueScroll() throws IOException {
        LOGGER.debug("[{}] Continuing scroll with id [{}]", context.jobId, scrollId);
        SearchResponse searchResponse;
        try {
             searchResponse = executeSearchScrollRequest(scrollId);
        } catch (SearchPhaseExecutionException searchExecutionException) {
            if (searchHasShardFailure == false) {
                LOGGER.debug("[{}] Reinitializing scroll due to SearchPhaseExecutionException", context.jobId);
                markScrollAsErrored();
                searchResponse = executeSearchRequest(buildSearchRequest(lastTimestamp == null ? context.start : lastTimestamp));
            } else {
                throw searchExecutionException;
            }
        }
        LOGGER.debug("[{}] Search response was obtained", context.jobId);
        return processSearchResponse(searchResponse);
    }

    private void markScrollAsErrored() {
        // This could be a transient error with the scroll Id.
        // Reinitialise the scroll and try again but only once.
        resetScroll();
        if (lastTimestamp != null) {
            lastTimestamp++;
        }
        searchHasShardFailure = true;
    }

    protected SearchResponse executeSearchScrollRequest(String scrollId) {
        return MlClientHelper.execute(context.headers, client, () -> SearchScrollAction.INSTANCE.newRequestBuilder(client)
                .setScroll(SCROLL_TIMEOUT)
                .setScrollId(scrollId)
                .get());
    }

    private void resetScroll() {
        clearScroll(scrollId);
        scrollId = null;
    }

    private void clearScroll(String scrollId) {
        if (scrollId != null) {
            ClearScrollRequest request = new ClearScrollRequest();
            request.addScrollId(scrollId);
            MlClientHelper.execute(context.headers, client, () -> client.execute(ClearScrollAction.INSTANCE, request).actionGet());
        }
    }
}
