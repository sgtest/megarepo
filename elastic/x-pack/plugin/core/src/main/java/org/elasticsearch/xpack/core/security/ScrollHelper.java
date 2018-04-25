/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.core.security;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.action.search.ClearScrollRequest;
import org.elasticsearch.action.search.SearchRequest;
import org.elasticsearch.action.search.SearchResponse;
import org.elasticsearch.action.search.SearchScrollRequest;
import org.elasticsearch.action.support.ContextPreservingActionListener;
import org.elasticsearch.client.Client;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.index.IndexNotFoundException;
import org.elasticsearch.search.SearchHit;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.Function;

public final class ScrollHelper {

    private ScrollHelper() {}

    /**
     * This method fetches all results for the given search request, parses them using the given hit parser and calls the
     * listener once done.
     */
    public static <T> void fetchAllByEntity(Client client, SearchRequest request, final ActionListener<Collection<T>> listener,
                                            Function<SearchHit, T> hitParser) {
        final List<T> results = new ArrayList<>();
        if (request.scroll() == null) { // we do scroll by default lets see if we can get rid of this at some point.
            request.scroll(TimeValue.timeValueSeconds(10L));
        }
        final Consumer<SearchResponse> clearScroll = (response) -> {
            if (response != null && response.getScrollId() != null) {
                ClearScrollRequest clearScrollRequest = new ClearScrollRequest();
                clearScrollRequest.addScrollId(response.getScrollId());
                client.clearScroll(clearScrollRequest, ActionListener.wrap((r) -> {}, (e) -> {}));
            }
        };
        // This function is MADNESS! But it works, don't think about it too hard...
        // simon edit: just watch this if you got this far https://www.youtube.com/watch?v=W-lF106Dgk8
        client.search(request, new ContextPreservingActionListener<>(client.threadPool().getThreadContext().newRestorableContext(true),
                new ActionListener<SearchResponse>() {
            private volatile SearchResponse lastResponse = null;

            @Override
            public void onResponse(SearchResponse resp) {
                try {
                    lastResponse = resp;
                    if (resp.getHits().getHits().length > 0) {
                        for (SearchHit hit : resp.getHits().getHits()) {
                            final T oneResult = hitParser.apply(hit);
                            if (oneResult != null) {
                                results.add(oneResult);
                            }
                        }

                        if (results.size() > resp.getHits().getTotalHits()) {
                            clearScroll.accept(lastResponse);
                            listener.onFailure(new IllegalStateException("scrolling returned more hits [" + results.size()
                                    + "] than expected [" + resp.getHits().getTotalHits() + "] so bailing out to prevent unbounded "
                                    + "memory consumption."));
                        } else if (results.size() == resp.getHits().getTotalHits()) {
                            clearScroll.accept(resp);
                            // Finally, return the list of the entity
                            listener.onResponse(Collections.unmodifiableList(results));
                        } else {
                            SearchScrollRequest scrollRequest = new SearchScrollRequest(resp.getScrollId());
                            scrollRequest.scroll(request.scroll().keepAlive());
                            client.searchScroll(scrollRequest, this);
                        }
                    } else {
                        clearScroll.accept(resp);
                        // Finally, return the list of the entity
                        listener.onResponse(Collections.unmodifiableList(results));
                    }
                } catch (Exception e){
                    onFailure(e); // lets clean up things
                }
            }

            @Override
            public void onFailure(Exception t) {
                try {
                    // attempt to clear the scroll request
                    clearScroll.accept(lastResponse);
                } finally {
                    if (t instanceof IndexNotFoundException) {
                        // since this is expected to happen at times, we just call the listener with an empty list
                        listener.onResponse(Collections.<T>emptyList());
                    } else {
                        listener.onFailure(t);
                    }
                }
            }
        }));
    }
}
