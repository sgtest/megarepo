/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.runtimefields.query;

import org.elasticsearch.script.Script;

import static org.hamcrest.Matchers.equalTo;

public class BooleanScriptFieldTermQueryTests extends AbstractBooleanScriptFieldQueryTestCase<BooleanScriptFieldTermQuery> {
    @Override
    protected BooleanScriptFieldTermQuery createTestInstance() {
        return createTestInstance(randomBoolean());
    }

    private BooleanScriptFieldTermQuery createTestInstance(boolean term) {
        return new BooleanScriptFieldTermQuery(randomScript(), leafFactory, randomAlphaOfLength(5), term);
    }

    @Override
    protected BooleanScriptFieldTermQuery copy(BooleanScriptFieldTermQuery orig) {
        return new BooleanScriptFieldTermQuery(orig.script(), leafFactory, orig.fieldName(), orig.term());
    }

    @Override
    protected BooleanScriptFieldTermQuery mutate(BooleanScriptFieldTermQuery orig) {
        Script script = orig.script();
        String fieldName = orig.fieldName();
        boolean term = orig.term();
        switch (randomInt(2)) {
            case 0:
                script = randomValueOtherThan(script, this::randomScript);
                break;
            case 1:
                fieldName += "modified";
                break;
            case 2:
                term = !term;
                break;
            default:
                fail();
        }
        return new BooleanScriptFieldTermQuery(script, leafFactory, fieldName, term);
    }

    @Override
    public void testMatches() {
        assertTrue(createTestInstance(true).matches(between(1, Integer.MAX_VALUE), 0));
        assertFalse(createTestInstance(true).matches(0, between(1, Integer.MAX_VALUE)));
        assertTrue(createTestInstance(true).matches(between(1, Integer.MAX_VALUE), between(1, Integer.MAX_VALUE)));

        assertFalse(createTestInstance(false).matches(between(1, Integer.MAX_VALUE), 0));
        assertTrue(createTestInstance(false).matches(0, between(1, Integer.MAX_VALUE)));
        assertTrue(createTestInstance(false).matches(between(1, Integer.MAX_VALUE), between(1, Integer.MAX_VALUE)));

        assertFalse(createTestInstance().matches(0, 0));
    }

    @Override
    protected void assertToString(BooleanScriptFieldTermQuery query) {
        assertThat(query.toString(query.fieldName()), equalTo(Boolean.toString(query.term())));
    }
}
