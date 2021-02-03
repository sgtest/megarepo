/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.eql.execution.assembler;

import org.elasticsearch.search.SearchHit;
import org.elasticsearch.xpack.eql.EqlIllegalArgumentException;
import org.elasticsearch.xpack.eql.execution.search.Ordinal;
import org.elasticsearch.xpack.eql.execution.search.QueryRequest;
import org.elasticsearch.xpack.ql.execution.search.extractor.HitExtractor;

import java.util.List;

public class Criterion<Q extends QueryRequest> {

    private final int stage;
    private final Q queryRequest;
    private final List<HitExtractor> keys;
    private final HitExtractor timestamp;
    private final HitExtractor tiebreaker;

    private final boolean descending;
    private final int keySize;

    public Criterion(int stage,
              Q queryRequest,
              List<HitExtractor> keys,
              HitExtractor timestamp,
              HitExtractor tiebreaker,
              boolean descending) {
        this.stage = stage;
        this.queryRequest = queryRequest;
        this.keys = keys;
        this.timestamp = timestamp;
        this.tiebreaker = tiebreaker;

        this.descending = descending;

        this.keySize = keys.size();
    }

    public int keySize() {
        return keySize;
    }

    public int stage() {
        return stage;
    }

    public boolean descending() {
        return descending;
    }

    public Q queryRequest() {
        return queryRequest;
    }

    public Object[] key(SearchHit hit) {
        Object[] key = null;
        if (keySize > 0) {
            Object[] docKeys = new Object[keySize];
            for (int i = 0; i < keySize; i++) {
                docKeys[i] = keys.get(i).extract(hit);
            }
            key = docKeys;
        }
        return key;
    }

    @SuppressWarnings({ "unchecked" })
    public Ordinal ordinal(SearchHit hit) {

        Object ts = timestamp.extract(hit);
        if (ts instanceof Number == false) {
            throw new EqlIllegalArgumentException("Expected timestamp as long but got {}", ts);
        }

        long timestamp = ((Number) ts).longValue();
        Comparable<Object> tbreaker = null;

        if (tiebreaker != null) {
            Object tb = tiebreaker.extract(hit);
            if (tb != null && tb instanceof Comparable == false) {
                throw new EqlIllegalArgumentException("Expected tiebreaker to be Comparable but got {}", tb);
            }
            tbreaker = (Comparable<Object>) tb;
        }
        return new Ordinal(timestamp, tbreaker);
    }

    @Override
    public String toString() {
        return "[" + stage + "][" + descending + "]";
    }
}
