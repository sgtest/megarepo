/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.profiling;

import org.apache.lucene.util.SetOnce;
import org.elasticsearch.common.bytes.BytesArray;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.index.query.QueryBuilder;
import org.elasticsearch.rest.RestRequest;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.test.rest.FakeRestRequest;
import org.elasticsearch.test.rest.RestActionTestCase;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentType;
import org.junit.Before;

import java.util.Collections;

import static java.util.Collections.emptyList;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.instanceOf;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;

public class RestGetStackTracesActionTests extends RestActionTestCase {
    @Before
    public void setUpAction() {
        controller().registerHandler(new RestGetStackTracesAction());
    }

    public void testPrepareEmptyRequest() {
        SetOnce<Boolean> executeCalled = new SetOnce<>();
        verifyingClient.setExecuteLocallyVerifier((actionType, request) -> {
            assertThat(request, instanceOf(GetStackTracesRequest.class));
            GetStackTracesRequest getStackTracesRequest = (GetStackTracesRequest) request;
            assertThat(getStackTracesRequest.getSampleSize(), nullValue());
            assertThat(getStackTracesRequest.getQuery(), nullValue());
            executeCalled.set(true);
            return new GetStackTracesResponse(
                Collections.emptyMap(),
                Collections.emptyMap(),
                Collections.emptyMap(),
                Collections.emptyMap(),
                0,
                1.0
            );
        });
        RestRequest profilingRequest = new FakeRestRequest.Builder(xContentRegistry()).withMethod(RestRequest.Method.POST)
            .withPath("/_profiling/stacktraces")
            .withContent(new BytesArray("{}"), XContentType.JSON)
            .build();
        dispatchRequest(profilingRequest);
        assertThat(executeCalled.get(), equalTo(true));
    }

    public void testPrepareParameterizedRequest() {
        SetOnce<Boolean> executeCalled = new SetOnce<>();
        verifyingClient.setExecuteLocallyVerifier((actionType, request) -> {
            assertThat(request, instanceOf(GetStackTracesRequest.class));
            GetStackTracesRequest getStackTracesRequest = (GetStackTracesRequest) request;
            assertThat(getStackTracesRequest.getSampleSize(), is(10000));
            assertThat(getStackTracesRequest.getQuery(), notNullValue(QueryBuilder.class));
            executeCalled.set(true);
            return new GetStackTracesResponse(
                Collections.emptyMap(),
                Collections.emptyMap(),
                Collections.emptyMap(),
                Collections.emptyMap(),
                0,
                0.0
            );
        });
        RestRequest request = new FakeRestRequest.Builder(xContentRegistry()).withMethod(RestRequest.Method.POST)
            .withPath("/_profiling/stacktraces")
            .withContent(new BytesArray("""
                            {
                              "sample_size": 10000,
                              "query": {
                                "bool": {
                                  "filter": [
                                    {
                                      "range": {
                                        "@timestamp": {
                                          "gte": "2022-10-05",
                                          "lt": "2022-12-05"
                                        }
                                      }
                                    }
                                  ]
                                }
                              }
                            }
                """), XContentType.JSON)
            .build();
        dispatchRequest(request);
        assertThat(executeCalled.get(), equalTo(true));
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        // to register the query parser
        return new NamedXContentRegistry(new SearchModule(Settings.EMPTY, emptyList()).getNamedXContents());
    }
}
