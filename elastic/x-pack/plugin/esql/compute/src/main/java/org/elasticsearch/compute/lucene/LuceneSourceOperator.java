/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.compute.lucene;

import org.apache.lucene.search.LeafCollector;
import org.apache.lucene.search.Query;
import org.apache.lucene.search.Scorable;
import org.apache.lucene.search.ScoreMode;
import org.elasticsearch.compute.data.DocVector;
import org.elasticsearch.compute.data.IntBlock;
import org.elasticsearch.compute.data.IntVector;
import org.elasticsearch.compute.data.Page;
import org.elasticsearch.compute.operator.DriverContext;
import org.elasticsearch.compute.operator.SourceOperator;
import org.elasticsearch.search.internal.SearchContext;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.List;
import java.util.function.Function;

/**
 * Source operator that incrementally runs Lucene searches
 */
public class LuceneSourceOperator extends LuceneOperator {

    private int currentPagePos = 0;
    private int remainingDocs;

    private IntVector.Builder docsBuilder;
    private final LeafCollector leafCollector;
    private final int minPageSize;

    public static class Factory implements LuceneOperator.Factory {
        private final DataPartitioning dataPartitioning;
        private final int taskConcurrency;
        private final int maxPageSize;
        private final int limit;
        private final LuceneSliceQueue sliceQueue;

        public Factory(
            List<SearchContext> searchContexts,
            Function<SearchContext, Query> queryFunction,
            DataPartitioning dataPartitioning,
            int taskConcurrency,
            int maxPageSize,
            int limit
        ) {
            this.maxPageSize = maxPageSize;
            this.limit = limit;
            this.dataPartitioning = dataPartitioning;
            var weightFunction = weightFunction(queryFunction, ScoreMode.COMPLETE_NO_SCORES);
            this.sliceQueue = LuceneSliceQueue.create(searchContexts, weightFunction, dataPartitioning, taskConcurrency);
            this.taskConcurrency = Math.min(sliceQueue.totalSlices(), taskConcurrency);
        }

        @Override
        public SourceOperator get(DriverContext driverContext) {
            return new LuceneSourceOperator(maxPageSize, sliceQueue, limit);
        }

        @Override
        public int taskConcurrency() {
            return taskConcurrency;
        }

        public int maxPageSize() {
            return maxPageSize;
        }

        public int limit() {
            return limit;
        }

        @Override
        public String describe() {
            return "LuceneSourceOperator[dataPartitioning = "
                + dataPartitioning
                + ", maxPageSize = "
                + maxPageSize
                + ", limit = "
                + limit
                + "]";
        }
    }

    public LuceneSourceOperator(int maxPageSize, LuceneSliceQueue sliceQueue, int limit) {
        super(maxPageSize, sliceQueue);
        this.minPageSize = Math.max(1, maxPageSize / 2);
        this.remainingDocs = limit;
        this.docsBuilder = IntVector.newVectorBuilder(Math.min(limit, maxPageSize));
        this.leafCollector = new LeafCollector() {
            @Override
            public void setScorer(Scorable scorer) {

            }

            @Override
            public void collect(int doc) {
                if (remainingDocs > 0) {
                    --remainingDocs;
                    docsBuilder.appendInt(doc);
                    currentPagePos++;
                }
            }
        };
    }

    @Override
    public boolean isFinished() {
        return doneCollecting;
    }

    @Override
    public void finish() {
        doneCollecting = true;
    }

    @Override
    public Page getOutput() {
        if (isFinished()) {
            assert currentPagePos == 0 : currentPagePos;
            return null;
        }
        try {
            final LuceneScorer scorer = getCurrentOrLoadNextScorer();
            if (scorer == null) {
                return null;
            }
            scorer.scoreNextRange(
                leafCollector,
                scorer.leafReaderContext().reader().getLiveDocs(),
                // Note: if (maxPageSize - currentPagePos) is a small "remaining" interval, this could lead to slow collection with a
                // highly selective filter. Having a large "enough" difference between max- and minPageSize (and thus currentPagePos)
                // alleviates this issue.
                maxPageSize - currentPagePos
            );
            Page page = null;
            if (currentPagePos >= minPageSize || remainingDocs <= 0 || scorer.isDone()) {
                pagesEmitted++;
                page = new Page(
                    currentPagePos,
                    new DocVector(
                        IntBlock.newConstantBlockWith(scorer.shardIndex(), currentPagePos).asVector(),
                        IntBlock.newConstantBlockWith(scorer.leafReaderContext().ord, currentPagePos).asVector(),
                        docsBuilder.build(),
                        true
                    ).asBlock()
                );
                docsBuilder = IntVector.newVectorBuilder(Math.min(remainingDocs, maxPageSize));
                currentPagePos = 0;
            }
            return page;
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    @Override
    protected void describe(StringBuilder sb) {
        sb.append(", remainingDocs=").append(remainingDocs);
    }
}
