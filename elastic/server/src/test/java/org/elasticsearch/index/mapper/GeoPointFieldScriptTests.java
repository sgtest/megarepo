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
import org.elasticsearch.script.GeoPointFieldScript;
import org.elasticsearch.script.ScriptContext;
import org.elasticsearch.search.lookup.SearchLookup;

import java.io.IOException;
import java.util.List;
import java.util.Map;

import static org.hamcrest.Matchers.equalTo;

public class GeoPointFieldScriptTests extends FieldScriptTestCase<GeoPointFieldScript.Factory> {
    public static final GeoPointFieldScript.Factory DUMMY = (fieldName, params, lookup) -> ctx -> new GeoPointFieldScript(
        fieldName,
        params,
        lookup,
        ctx
    ) {
        @Override
        public void execute() {
            emit(0, 0);
        }
    };

    @Override
    protected ScriptContext<GeoPointFieldScript.Factory> context() {
        return GeoPointFieldScript.CONTEXT;
    }

    @Override
    protected GeoPointFieldScript.Factory dummyScript() {
        return DUMMY;
    }

    public void testAsDocValues() {
        GeoPointFieldScript script = new GeoPointFieldScript(
                "test",
                Map.of(),
                new SearchLookup(field -> null, (ft, lookup) -> null),
                null
        ) {
            @Override
            public void execute() {
                emit(78.96, 12.12);
                emit(13.45, 56.78);
            }
        };
        script.execute();

        assertArrayEquals(new long[] {1378381707499043786L, 8091971733044486384L}, script.asDocValues());
    }

    public void testTooManyValues() throws IOException {
        try (Directory directory = newDirectory(); RandomIndexWriter iw = new RandomIndexWriter(random(), directory)) {
            iw.addDocument(List.of(new StoredField("_source", new BytesRef("{}"))));
            try (DirectoryReader reader = iw.getReader()) {
                GeoPointFieldScript script = new GeoPointFieldScript(
                    "test",
                    Map.of(),
                    new SearchLookup(field -> null, (ft, lookup) -> null),
                    reader.leaves().get(0)
                ) {
                    @Override
                    public void execute() {
                        for (int i = 0; i <= AbstractFieldScript.MAX_VALUES; i++) {
                            emit(0, 0);
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
