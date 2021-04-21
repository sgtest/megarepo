/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.script;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.common.time.DateFormatter;
import org.elasticsearch.search.lookup.SearchLookup;

import java.util.Map;

public abstract class DateFieldScript extends AbstractLongFieldScript {
    public static final ScriptContext<Factory> CONTEXT = newContext("date_field", Factory.class);

    @SuppressWarnings("unused")
    public static final String[] PARAMETERS = {};

    public interface Factory extends ScriptFactory {
        LeafFactory newFactory(String fieldName, Map<String, Object> params, SearchLookup searchLookup, DateFormatter formatter);
    }

    public interface LeafFactory {
        DateFieldScript newInstance(LeafReaderContext ctx);
    }

    private final DateFormatter formatter;

    public DateFieldScript(
        String fieldName,
        Map<String, Object> params,
        SearchLookup searchLookup,
        DateFormatter formatter,
        LeafReaderContext ctx
    ) {
        super(fieldName, params, searchLookup, ctx);
        this.formatter = formatter;
    }

    public static class Emit {
        private final DateFieldScript script;

        public Emit(DateFieldScript script) {
            this.script = script;
        }

        public void emit(long v) {
            script.emit(v);
        }
    }

    /**
     * Temporary parse method that takes into account the date format. We'll
     * remove this when we have "native" source parsing fields.
     */
    public static class Parse {
        private final DateFieldScript script;

        public Parse(DateFieldScript script) {
            this.script = script;
        }

        public long parse(Object str) {
            return script.formatter.parseMillis(str.toString());
        }
    }
}
