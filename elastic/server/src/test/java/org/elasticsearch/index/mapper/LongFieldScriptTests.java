/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.index.mapper;

import org.apache.lucene.document.StoredField;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.RandomIndexWriter;
import org.apache.lucene.store.Directory;
import org.apache.lucene.util.BytesRef;
import org.elasticsearch.script.AbstractFieldScript;
import org.elasticsearch.script.LongFieldScript;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;

public class LongFieldScriptTests extends FieldScriptTestCase<LongFieldScript.Factory> {
    public static final LongFieldScript.Factory DUMMY = (fieldName, params, lookup) -> ctx -> new LongFieldScript(
        fieldName,
        params,
        lookup,
        ctx
    ) {
        @Override
        public void execute() {
            emit(1);
        }
    };

    @Override
    protected ScriptContext<LongFieldScript.Factory> context() {
        return LongFieldScript.CONTEXT;
    }

    @Override
    protected LongFieldScript.Factory dummyScript() {
        return DUMMY;
    }

    public void testAsDocValues() {
        LongFieldScript script = new LongFieldScript(
                "test",
                Map.of(),
                new SearchLookup(field -> null, (ft, lookup) -> null),
                null
        ) {
            @Override
            public void execute() {
                emit(3L);
                emit(1L);
                emit(20000000000L);
                emit(10L);
                emit(-1000L);
                emit(0L);
            }
        };
        script.execute();

        assertArrayEquals(new long[] {-1000L, 0L, 1L, 3L, 10L, 20000000000L}, script.asDocValues());
    }

    public void testTooManyValues() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{}"))));
            try (DirectoryReader reader = iw.getReader()) {
                LongFieldScript script = new LongFieldScript(
                    "test",
                    Map.of(),
                    new SearchLookup(field -> null, (ft, lookup) -> null),
                    reader.leaves().get(0)
                ) {
                    @Override
                    public void execute() {
                        for (int i = 0; i <= AbstractFieldScript.MAX_VALUES; i++) {
                            emit(0);
                        }
                    }
                };
                Exception e = expectThrows(IllegalArgumentException.class, script::execute);
                assertThat(
                    e.getMessage(),
                    equalTo("Runtime field [test] is emitting [101] values while the maximum number of values allowed is [100]")
                );
            }
        }
    }
}
