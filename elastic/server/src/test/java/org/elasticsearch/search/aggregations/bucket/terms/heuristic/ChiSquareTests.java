/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.search.aggregations.bucket.terms.heuristic;

import org.elasticsearch.search.aggregations.bucket.terms.AbstractSignificanceHeuristicTests;

public class ChiSquareTests extends AbstractSignificanceHeuristicTests {
    @Override
    protected SignificanceHeuristic getHeuristic() {
        return new ChiSquare(randomBoolean(), randomBoolean());
    }

    @Override
    protected boolean testZeroScore() {
        return false;
    }

    @Override
    public void testAssertions() {
        testBackgroundAssertions(new ChiSquare(true, true), new ChiSquare(true, false));
    }
}
