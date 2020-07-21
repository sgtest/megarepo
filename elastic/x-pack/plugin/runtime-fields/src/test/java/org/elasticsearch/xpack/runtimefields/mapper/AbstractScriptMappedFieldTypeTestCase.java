/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.mapper;

import org.elasticsearch.index.mapper.MapperService;
import org.elasticsearch.index.query.QueryShardContext;
import org.elasticsearch.search.lookup.SearchLookup;
import org.elasticsearch.test.ESTestCase;

import static org.mockito.Matchers.any;
import static org.mockito.Matchers.anyString;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

abstract class AbstractScriptMappedFieldTypeTestCase extends ESTestCase {
    protected QueryShardContext mockContext() {
        return mockContext(true);
    }

    protected static QueryShardContext mockContext(boolean allowExpensiveQueries) {
        return mockContext(allowExpensiveQueries, null);
    }

    protected static QueryShardContext mockContext(boolean allowExpensiveQueries, AbstractScriptMappedFieldType mappedFieldType) {
        MapperService mapperService = mock(MapperService.class);
        when(mapperService.fieldType(anyString())).thenReturn(mappedFieldType);
        QueryShardContext context = mock(QueryShardContext.class);
        if (mappedFieldType != null) {
            when(context.fieldMapper(anyString())).thenReturn(mappedFieldType);
            when(context.getSearchAnalyzer(any())).thenReturn(mappedFieldType.getTextSearchInfo().getSearchAnalyzer());
        }
        when(context.allowExpensiveQueries()).thenReturn(allowExpensiveQueries);
        when(context.lookup()).thenReturn(new SearchLookup(mapperService, mft -> null));
        return context;
    }
}
