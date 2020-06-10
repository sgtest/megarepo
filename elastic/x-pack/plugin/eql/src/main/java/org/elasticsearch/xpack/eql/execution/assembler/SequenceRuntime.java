/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.eql.execution.assembler;

import org.elasticsearch.action.ActionListener;
import org.elasticsearch.common.unit.TimeValue;
import org.elasticsearch.search.SearchHit;
import org.elasticsearch.xpack.eql.execution.payload.Payload;
import org.elasticsearch.xpack.eql.execution.sequence.Sequence;
import org.elasticsearch.xpack.eql.execution.sequence.SequenceKey;
import org.elasticsearch.xpack.eql.execution.sequence.SequenceStateMachine;
import org.elasticsearch.xpack.eql.session.Results;
import org.elasticsearch.xpack.ql.execution.search.extractor.HitExtractor;

import java.util.ArrayList;
import java.util.List;

import static org.elasticsearch.action.ActionListener.wrap;

/**
 * Executable tracking sequences at runtime.
 */
class SequenceRuntime implements Executable {

    private final List<Criterion> criteria;
    // NB: just like in a list, this represents the total number of stages yet counting starts at 0
    private final int numberOfStages;
    private final SequenceStateMachine stateMachine;
    private final QueryClient queryClient;
    private long startTime;

    SequenceRuntime(List<Criterion> criteria, QueryClient queryClient) {
        this.criteria = criteria;
        this.numberOfStages = criteria.size();
        this.queryClient = queryClient;
        boolean hasTiebreaker = criteria.get(0).tiebreakerExtractor() != null;
        this.stateMachine = new SequenceStateMachine(numberOfStages, hasTiebreaker);
    }

    @Override
    public void execute(ActionListener<Results> resultsListener) {
        startTime = System.currentTimeMillis();
        startSequencing(resultsListener);
    }

    private void startSequencing(ActionListener<Results> resultsListener) {
        Criterion firstStage = criteria.get(0);
        queryClient.query(firstStage.searchSource(), wrap(payload -> {

            // 1. execute last stage (find keys)
            startTracking(payload, resultsListener);

            // 2. go descending through the rest of the stages, while adjusting the query
            inspectStage(1, resultsListener);

        }, resultsListener::onFailure));
    }

    private void startTracking(Payload<SearchHit> payload, ActionListener<Results> resultsListener) {
        Criterion lastCriterion = criteria.get(0);
        List<SearchHit> hits = payload.values();

        // nothing matches the first query, bail out early
        if (hits.isEmpty()) {
            resultsListener.onResponse(assembleResults());
            return;
        }
        
        long tMin = Long.MAX_VALUE;
        long tMax = Long.MIN_VALUE;
        
        Comparable<Object> bMin = null;
        // we could have extracted that in the hit loop but that if would have been evaluated
        // for every document
        if (hits.isEmpty() == false) {
            tMin = lastCriterion.timestamp(hits.get(0));
            tMax = lastCriterion.timestamp(hits.get(hits.size() - 1));
            
            if (lastCriterion.tiebreakerExtractor() != null) {
               bMin = lastCriterion.tiebreaker(hits.get(0));
            }
        }

        for (SearchHit hit : hits) {
            KeyAndOrdinal ko = findKey(hit, lastCriterion);
            Sequence seq = new Sequence(ko.key, numberOfStages, ko.timestamp, ko.tiebreaker, hit);
            stateMachine.trackSequence(seq, tMin, tMax);
        }
        stateMachine.setTimestampMarker(0, tMin);
        if (bMin != null) {
            stateMachine.setTiebreakerMarker(0, bMin);
        }
    }

    private void inspectStage(int stage, ActionListener<Results> resultsListener) {
        // sequencing is done, return results
        if (stage == numberOfStages) {
            resultsListener.onResponse(assembleResults());
            return;
        }
        // else continue finding matches
        Criterion currentCriterion = criteria.get(stage);
        // narrow by the previous stage timestamp marker
        currentCriterion.fromMarkers(stateMachine.getMarkers(stage - 1));
        
        queryClient.query(currentCriterion.searchSource(), wrap(payload -> {
            findMatches(stage, payload);
            inspectStage(stage + 1, resultsListener);
        }, resultsListener::onFailure));
    }

    private void findMatches(int currentStage, Payload<SearchHit> payload) {
        Criterion currentCriterion = criteria.get(currentStage);
        List<SearchHit> hits = payload.values();
        
        // break the results per key
        for (SearchHit hit : hits) {
            KeyAndOrdinal ko = findKey(hit, currentCriterion);
            stateMachine.match(currentStage, ko.key, ko.timestamp, ko.tiebreaker, hit);
        }
    }

    private KeyAndOrdinal findKey(SearchHit hit, Criterion criterion) {
        List<HitExtractor> keyExtractors = criterion.keyExtractors();

        SequenceKey key;
        if (criterion.keyExtractors().isEmpty()) {
            key = SequenceKey.NONE;
        } else {
            Object[] docKeys = new Object[keyExtractors.size()];
            for (int i = 0; i < docKeys.length; i++) {
                docKeys[i] = keyExtractors.get(i).extract(hit);
            }
            key = new SequenceKey(docKeys);
        }

        return new KeyAndOrdinal(key, criterion.timestamp(hit), criterion.tiebreaker(hit));
    }

    private Results assembleResults() {
        List<Sequence> done = stateMachine.completeSequences();
        List<org.elasticsearch.xpack.eql.action.EqlSearchResponse.Sequence> response = new ArrayList<>(done.size());
        for (Sequence s : done) {
            response.add(new org.elasticsearch.xpack.eql.action.EqlSearchResponse.Sequence(s.key().asStringList(), s.hits()));
        }
        
        TimeValue tookTime = new TimeValue(System.currentTimeMillis() - startTime);
        return Results.fromSequences(tookTime, response);
    }
}