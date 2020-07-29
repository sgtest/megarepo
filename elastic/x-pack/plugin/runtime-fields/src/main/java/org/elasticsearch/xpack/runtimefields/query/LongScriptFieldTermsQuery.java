/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.runtimefields.query;

import com.carrotsearch.hppc.LongSet;

import org.apache.lucene.index.LeafReaderContext;
import org.elasticsearch.common.CheckedFunction;
import org.elasticsearch.script.Script;
import org.elasticsearch.xpack.runtimefields.AbstractLongScriptFieldScript;

import java.io.IOException;
import java.util.Objects;

public class LongScriptFieldTermsQuery extends AbstractLongScriptFieldQuery {
    private final LongSet terms;

    public LongScriptFieldTermsQuery(
        Script script,
        CheckedFunction<LeafReaderContext, AbstractLongScriptFieldScript, IOException> leafFactory,
        String fieldName,
        LongSet terms
    ) {
        super(script, leafFactory, fieldName);
        this.terms = terms;
    }

    @Override
    protected boolean matches(long[] values, int count) {
        for (int i = 0; i < count; i++) {
            if (terms.contains(values[i])) {
                return true;
            }
        }
        return false;
    }

    @Override
    public final String toString(String field) {
        if (fieldName().contentEquals(field)) {
            return terms.toString();
        }
        return fieldName() + ":" + terms;
    }

    @Override
    public int hashCode() {
        return Objects.hash(super.hashCode(), terms);
    }

    @Override
    public boolean equals(Object obj) {
        if (false == super.equals(obj)) {
            return false;
        }
        LongScriptFieldTermsQuery other = (LongScriptFieldTermsQuery) obj;
        return terms.equals(other.terms);
    }

    LongSet terms() {
        return terms;
    }
}
